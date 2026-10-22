//! Read-only, synchronous diagnostic counters. Never part of a release overlay.
use std::fmt;
use std::sync::OnceLock;
use std::time::Instant;

const REGISTERS: [u64; 4] = [0x10, 0xe8, 0xe7, 0x10];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Unsupported,
    Configuration,
    Io(Option<i32>),
    ShortRead,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (reason, errno) = match self {
            Self::Unsupported => ("unsupported", None),
            Self::Configuration => ("configuration", None),
            Self::Io(errno) => ("io", *errno),
            Self::ShortRead => ("short-read", None),
        };
        write!(f, "\"reason\":\"{reason}\",\"errno\":")?;
        match errno {
            Some(value) => write!(f, "{value}"),
            None => f.write_str("null"),
        }
    }
}

trait RegisterReader {
    fn read_register(&self, register: u64) -> Result<u64, Failure>;
}

fn read_counters(reader: &impl RegisterReader) -> Result<[u64; 4], Failure> {
    let mut values = [0; 4];
    for (value, register) in values.iter_mut().zip(REGISTERS) {
        *value = reader.read_register(register)?;
    }
    Ok(values)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct Device {
    cpu: usize,
    file: std::fs::File,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl RegisterReader for Device {
    fn read_register(&self, register: u64) -> Result<u64, Failure> {
        use std::os::unix::fs::FileExt;
        let mut bytes = [0; 8];
        loop {
            match self.file.read_at(&mut bytes, register) {
                Ok(8) => return Ok(u64::from_le_bytes(bytes)),
                Ok(_) => return Err(Failure::ShortRead),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(Failure::Io(error.raw_os_error())),
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn read_device() -> Result<(usize, [u64; 4]), Failure> {
    static DEVICE: OnceLock<Result<Device, Failure>> = OnceLock::new();
    let device = DEVICE.get_or_init(|| {
        let cpu = std::env::var("RSE_SAMPLER_CPU")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or(Failure::Configuration)?;
        let status = std::fs::read_to_string("/proc/thread-self/status")
            .map_err(|error| Failure::Io(error.raw_os_error()))?;
        let mask = status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:").map(str::trim));
        if mask.and_then(|value| value.parse::<usize>().ok()) != Some(cpu) {
            return Err(Failure::Configuration);
        }
        let file = std::fs::File::open(format!("/dev/cpu/{cpu}/msr"))
            .map_err(|error| Failure::Io(error.raw_os_error()))?;
        Ok(Device { cpu, file })
    });
    match device {
        Ok(device) => read_counters(device).map(|values| (device.cpu, values)),
        Err(error) => Err(*error),
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn read_device() -> Result<(usize, [u64; 4]), Failure> {
    Err(Failure::Unsupported)
}

fn now_ns() -> u128 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_nanos()
}

// The enclosing process snapshot and counter reads finish synchronously before
// returning to the timer. I/O errors are data, so an after-timer failure cannot
// discard a completed benchmark observation or masquerade as a zero delta.
fn snapshot_with(
    process: impl FnOnce() -> String,
    read: impl FnOnce() -> Result<(usize, [u64; 4]), Failure>,
) -> String {
    let start_ns = now_ns();
    let mut details = process();
    let counters = read();
    let end_ns = now_ns();
    if !details.ends_with('}') {
        return details;
    }
    details.pop();
    use std::fmt::Write;
    let _ = write!(
        details,
        ",\"service_clock\":{{\"schema\":\"native-service-clock-v1\",\"start_ns\":{start_ns},\"end_ns\":{end_ns},"
    );
    match counters {
        Ok((cpu, [tsc_before, aperf, mperf, tsc_after])) => {
            let _ = write!(
                details,
                "\"available\":true,\"cpu\":{cpu},\"counters\":{{\"tsc_before\":{tsc_before},\"aperf\":{aperf},\"mperf\":{mperf},\"tsc_after\":{tsc_after}}}"
            );
        }
        Err(error) => {
            let _ = write!(details, "\"available\":false,{error}");
        }
    }
    details.push_str("}}");
    details
}

pub(super) fn snapshot(process: impl FnOnce() -> String) -> String {
    snapshot_with(process, read_device)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) fn probe() {
    let cpu = std::env::var("RSE_SAMPLER_CPU").expect("probe requires the reserved CPU");
    let tid = std::fs::read_link("/proc/thread-self").expect("Linux thread identity");
    assert!(
        std::process::Command::new("taskset")
            .args(["--pid", "--cpu-list", &cpu])
            .arg(tid.file_name().expect("thread identity"))
            .output()
            .expect("pin only this probe thread")
            .status
            .success()
    );
    let before = snapshot(|| "{\"available\":true}".to_owned());
    let start = Instant::now();
    let mut value = 1u64;
    while start.elapsed() < std::time::Duration::from_millis(100) {
        value = std::hint::black_box(value.wrapping_mul(6364136223846793005).wrapping_add(1));
    }
    std::hint::black_box(value);
    let wall_ns = start.elapsed().as_nanos();
    let after = snapshot(|| "{\"available\":true}".to_owned());
    println!(
        "RSE_NATIVE_CLOCK_PROBE {{\"wall_ns\":{wall_ns},\"before\":{before},\"after\":{after}}}"
    );
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub(super) fn probe() {
    panic!("native service counters require Linux x86_64");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Reader {
        calls: RefCell<Vec<u64>>,
        fail_at: Option<usize>,
    }
    impl RegisterReader for Reader {
        fn read_register(&self, register: u64) -> Result<u64, Failure> {
            let mut calls = self.calls.borrow_mut();
            calls.push(register);
            if self.fail_at == Some(calls.len()) {
                Err(Failure::Io(Some(5)))
            } else {
                Ok(calls.len() as u64)
            }
        }
    }

    #[test]
    fn counter_order_brackets_both_service_counters() {
        let reader = Reader {
            calls: RefCell::new(Vec::new()),
            fail_at: None,
        };
        assert_eq!(read_counters(&reader), Ok([1, 2, 3, 4]));
        assert_eq!(*reader.calls.borrow(), REGISTERS);
    }

    #[test]
    fn each_read_failure_refuses_partial_counter_values() {
        for index in 1..=4 {
            let reader = Reader {
                calls: RefCell::new(Vec::new()),
                fail_at: Some(index),
            };
            assert_eq!(read_counters(&reader), Err(Failure::Io(Some(5))));
            assert_eq!(reader.calls.borrow().len(), index);
        }
    }

    #[test]
    fn available_payload_has_complete_counter_fields() {
        let value = snapshot_with(
            || "{\"available\":true,\"marker\":19}".to_owned(),
            || Ok((0, [100, 200, 300, 101])),
        );
        println!("\nRSE_NATIVE_FIXTURE {value}");
        assert!(value.contains("\"available\":true,\"cpu\":0,\"counters\":{"));
    }

    #[test]
    fn process_payload_is_preserved_even_when_counters_are_unavailable() {
        let value = snapshot_with(
            || "{\"available\":true,\"marker\":19}".to_owned(),
            || Err(Failure::Io(Some(5))),
        );
        println!("\nRSE_NATIVE_FIXTURE {value}");
        assert!(value.starts_with("{\"available\":true,\"marker\":19,\"service_clock\":{"));
        assert!(value.ends_with("}}"));
        assert!(value.contains("\"available\":false,\"reason\":\"io\",\"errno\":5"));
    }
}
