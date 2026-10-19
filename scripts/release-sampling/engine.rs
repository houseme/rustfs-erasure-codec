//! Shared external integration-test sampler; no codec implementation is replaced.
use super::layout::OptionalFixture;
use super::workload::OptionalWorkload;
use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::sync::OnceLock;
use std::time::Instant;

mod service_window;
use service_window::{TARGET_TIMED_NS, TARGET_WALL_NS};
const SAMPLES: usize = 9;
const WARMUPS: usize = 3;
const MAX_CALIBRATION_BATCHES: usize = 12;
const MAX_ITERATIONS: usize = 1_000_000_000;
thread_local! { static ITERATIONS: Cell<usize> = const { Cell::new(1) }; }

pub fn iterations() -> usize {
    ITERATIONS.with(Cell::get)
}

fn revision() -> &'static str {
    static REVISION: OnceLock<String> = OnceLock::new();
    REVISION.get_or_init(|| {
        let result = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(result.status.success());
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    })
}

#[derive(Default)]
pub(super) struct Telemetry {
    available: bool,
    cpu: u64,
    run_ns: u64,
    wait_ns: u64,
    switches: u64,
    process_minor_faults: u64,
    process_major_faults: u64,
    process: Option<String>,
}

impl std::fmt::Display for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.available {
            return f.write_str("null");
        }
        write!(
            f,
            "{{\"cpu\":{},\"run_ns\":{},\"wait_ns\":{},\"switches\":{},\"process_minor_faults\":{},\"process_major_faults\":{}",
            self.cpu,
            self.run_ns,
            self.wait_ns,
            self.switches,
            self.process_minor_faults,
            self.process_major_faults
        )?;
        if let Some(process) = &self.process {
            write!(f, ",\"process\":{process}")?;
        }
        f.write_str("}")
    }
}

// Procfs reports a zero file size. read_to_string/read_to_end can grow and
// shrink heap buffers, changing glibc arena trimming before the next sample.
// Keep the ordinary observer entirely in bounded stack storage instead.
fn read_bounded<'a>(path: &str, buffer: &'a mut [u8]) -> io::Result<&'a str> {
    let mut file = fs::File::open(path)?;
    let mut used = 0;
    while used < buffer.len() {
        match file.read(&mut buffer[used..]) {
            Ok(0) => {
                return std::str::from_utf8(&buffer[..used])
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidData));
            }
            Ok(count) => used += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    // Fail closed instead of parsing a truncated stat/schedstat record.
    Err(io::ErrorKind::InvalidData.into())
}

// /proc/self/stat counts faults for the whole process, including Rayon workers.
// Child-process counters (fields 11/13) are deliberately excluded. No IDs escape.
fn process_faults(stat: &str) -> io::Result<(u64, u64)> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidData);
    let mut fields = stat
        .rsplit_once(") ")
        .ok_or_else(invalid)?
        .1
        .split_whitespace();
    let minor = fields
        .nth(7)
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let major = fields
        .nth(1)
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    Ok((minor, major))
}

#[test]
fn process_fault_offsets_exclude_children_and_survive_comm_parentheses() {
    let mut values = ["0"; 40];
    values[7] = "19";
    values[8] = "999";
    values[9] = "3";
    values[10] = "777";
    let stat = format!("777 (worker ) tricky) {}", values.join(" "));
    assert_eq!(process_faults(&stat).unwrap(), (19, 3));
    for bad in [
        "truncated",
        "777 (comm) R 1 2",
        "777 (comm) R 0 0 0 0 0 0 -1 0 0",
    ] {
        assert!(process_faults(bad).is_err());
    }
}

pub(super) fn telemetry() -> Telemetry {
    let mut stat_buffer = [0u8; 4096];
    let Ok(stat) = read_bounded("/proc/thread-self/stat", &mut stat_buffer) else {
        return Telemetry::default();
    };
    let cpu = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(36)
        .unwrap()
        .parse()
        .unwrap();
    let mut sched_buffer = [0u8; 256];
    let sched = read_bounded("/proc/thread-self/schedstat", &mut sched_buffer).unwrap();
    let mut counters = sched.split_whitespace().map(|s| s.parse::<u64>().unwrap());
    let (run_ns, wait_ns, switches) = (
        counters.next().unwrap(),
        counters.next().unwrap(),
        counters.next().unwrap(),
    );
    assert!(counters.next().is_none());
    let mut process_stat_buffer = [0u8; 4096];
    let (process_minor_faults, process_major_faults) =
        process_faults(read_bounded("/proc/self/stat", &mut process_stat_buffer).unwrap()).unwrap();
    let process = if std::env::var("RSE_SAMPLER_DIAGNOSTIC_PROCESS")
        .ok()
        .as_deref()
        == Some("1")
    {
        Some(super::process::snapshot())
    } else {
        None
    };
    Telemetry {
        available: true,
        cpu,
        run_ns,
        wait_ns,
        switches,
        process_minor_faults,
        process_major_faults,
        process,
    }
}

// Read the measuring thread's mask, never infer placement from CPU endpoints.
fn affinity_list(status: &str) -> Option<&str> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:").map(str::trim))
}

pub(super) fn affinity_cpu() -> Option<usize> {
    let mut buffer = [0u8; 4096];
    affinity_list(read_bounded("/proc/thread-self/status", &mut buffer).ok()?)?
        .parse()
        .ok()
}

struct Placement {
    cpu: usize,
    constructor_budget: usize,
    constructor_affinity: Vec<usize>,
    pool_threads: usize,
}
impl Placement {
    fn prepare(constructor_budget: usize) -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                std::thread::available_parallelism().unwrap().get(),
                constructor_budget
            );
            let cpu = std::env::var("RSE_SAMPLER_CPU")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let mut buffer = [0u8; 4096];
            let inherited =
                affinity_list(read_bounded("/proc/thread-self/status", &mut buffer).unwrap())
                    .unwrap()
                    .to_owned();
            // A lazily-created pool would otherwise inherit the pinned single
            // CPU. Start and check every worker before changing only this TID.
            rayon::broadcast(|_| {
                let mut buffer = [0u8; 4096];
                assert_eq!(
                    affinity_list(read_bounded("/proc/thread-self/status", &mut buffer).unwrap()),
                    Some(inherited.as_str())
                );
            });
            let pool_threads = rayon::current_num_threads();
            let tid = fs::read_link("/proc/thread-self").unwrap();
            let output = std::process::Command::new("taskset")
                .args(["--pid", "--cpu-list"])
                .arg(cpu.to_string())
                .arg(tid.file_name().unwrap())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "measuring-thread affinity change failed"
            );
            assert_eq!(affinity_cpu(), Some(cpu));
            assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
            // Existing workers must keep the complete reservation after the
            // measuring thread is pinned, including on parallel API paths.
            rayon::broadcast(|_| {
                let mut buffer = [0u8; 4096];
                assert_eq!(
                    affinity_list(read_bounded("/proc/thread-self/status", &mut buffer).unwrap()),
                    Some(inherited.as_str())
                );
            });
            let constructor_affinity = inherited
                .split(',')
                .flat_map(|part| {
                    let (first, last) = part.split_once('-').unwrap_or((part, part));
                    first.parse::<usize>().unwrap()..=last.parse::<usize>().unwrap()
                })
                .collect();
            Some(Self {
                cpu,
                constructor_budget,
                constructor_affinity,
                pool_threads,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = constructor_budget;
            None
        }
    }
    fn is_current(&self) -> bool {
        affinity_cpu() == Some(self.cpu)
    }
    fn check(&self) {
        assert!(self.is_current(), "measuring thread escaped its fixed CPU");
    }
}
#[derive(Clone, Copy)]
struct OptionalPlacement<'a>(Option<&'a Placement>, bool);
impl std::fmt::Display for OptionalPlacement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(p) => write!(
                f,
                "{{\"cpu\":{},\"constructor_budget\":{},\"constructor_affinity\":{:?},\"pool_threads\":{},\"affinity_checked\":{}}}",
                p.cpu, p.constructor_budget, p.constructor_affinity, p.pool_threads, self.1
            ),
            None => f.write_str("null"),
        }
    }
}

struct Observation<R> {
    result: R,
    wall_ns: u128,
    timed_ns: f64,
    before: Telemetry,
    after: Telemetry,
    fixture: OptionalFixture,
    workload: OptionalWorkload,
    affinity_checked: bool,
}

impl<R> Observation<R> {
    fn target_met(&self) -> bool {
        self.wall_ns as f64 >= TARGET_WALL_NS as f64 && self.timed_ns >= TARGET_TIMED_NS as f64
    }
}

pub(super) struct Output<R> {
    pub(super) result: R,
    pub(super) fixture: OptionalFixture,
    pub(super) workload: OptionalWorkload,
}

fn measure_fixed<R>(
    key: &str,
    count: usize,
    operation: &mut impl FnMut(usize) -> Output<R>,
    ns_per_iter: &impl Fn(&R) -> f64,
    placement: OptionalPlacement<'_>,
) -> (Vec<Observation<R>>, std::thread::Result<()>) {
    let mut observations = Vec::with_capacity(WARMUPS + SAMPLES);
    // A failed operation is rethrown after completed observations are exported.
    // No further workload runs against possibly interrupted mutable state.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for index in 0..WARMUPS + SAMPLES {
            ITERATIONS.with(|n| n.set(count));
            if let Some(p) = placement.0 {
                p.check();
            }
            let before = telemetry();
            let start = Instant::now();
            let Output {
                result,
                fixture,
                workload,
            } = std::hint::black_box(operation(count));
            let wall_ns = start.elapsed().as_nanos();
            let after = telemetry();
            let affinity_checked = placement.0.is_none_or(Placement::is_current);
            let record_placement = OptionalPlacement(placement.0, affinity_checked);
            let timed_ns = ns_per_iter(&result) * count as f64;
            assert!(timed_ns.is_finite() && timed_ns > 0.0);
            // Keep a mode-independent timing journal even if a process is
            // interrupted before the deferred original artifact export.
            println!(
                "\nRSE_TIMING {{\"case\":\"{key}\",\"index\":{index},\"iterations\":{count},\"wall_ns\":{wall_ns},\"timed_ns\":{timed_ns},\"fixture\":{fixture},\"workload\":{workload},\"placement\":{record_placement},\"before\":{before},\"after\":{after}}}"
            );
            observations.push(Observation {
                result,
                wall_ns,
                timed_ns,
                before,
                after,
                fixture,
                workload,
                affinity_checked,
            });
            // Keep the completed numeric row, marked invalid, before stopping.
            // A failed mask check must not erase an operation that already ran.
            assert!(
                affinity_checked,
                "measuring thread escaped after timing; completed row retained"
            );
        }
    }));
    (observations, outcome)
}

pub(super) fn run_prepared<R, F: FnMut(usize) -> Output<R>>(
    key: &str,
    filename: &str,
    prepare: impl FnOnce(usize) -> F,
    ns_per_iter: impl Fn(&R) -> f64,
    export: impl Fn(&R),
) {
    let selected = std::env::var("RSE_SAMPLER_CASE").ok();
    if selected.as_deref().is_some_and(|case| case != key) {
        return;
    }
    let isolated_case = selected.is_some();
    let calibrating = std::env::var("RSE_SAMPLER_MODE").unwrap() == "calibrate";
    // Both modes receive identical primitive inputs. Frozen-count parsing and
    // adaptive calibration live in the controller, outside the measured process.
    let count = std::env::var("RSE_SAMPLER_ITERATIONS")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!((1..=MAX_ITERATIONS).contains(&count));
    let calibration_batch = std::env::var("RSE_SAMPLER_BATCH")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(if calibrating {
        (1..=MAX_CALIBRATION_BATCHES).contains(&calibration_batch)
    } else {
        calibration_batch == 0
    });
    let page_size = std::env::var("RSE_SAMPLER_PAGE_SIZE")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(page_size.is_power_of_two() && (1024..=1048576).contains(&page_size));
    let constructor_budget = std::thread::available_parallelism().unwrap().get();
    let mut operation = prepare(page_size);
    let prepared_placement = Placement::prepare(constructor_budget);
    let placement = OptionalPlacement(prepared_placement.as_ref(), true);
    let (observations, outcome) =
        measure_fixed(key, count, &mut operation, &ns_per_iter, placement);
    let complete_batch = observations.len() == WARMUPS + SAMPLES
        && observations
            .iter()
            .skip(WARMUPS)
            .all(|r| r.target_met() && r.affinity_checked);
    // Git queries and original JSON/CSV writers run only after the complete
    // measurement kernel, so their heap/file/child-process state cannot seed
    // another observation in this worker.
    for (record_index, observation) in observations.into_iter().enumerate() {
        let target_met = observation.target_met();
        let Observation {
            result,
            wall_ns,
            timed_ns,
            before,
            after,
            fixture,
            workload,
            affinity_checked,
        } = observation;
        let placement = OptionalPlacement(prepared_placement.as_ref(), affinity_checked);
        let warming = record_index < WARMUPS;
        let complete = if calibrating {
            record_index + 1 == WARMUPS + SAMPLES && complete_batch
        } else {
            target_met && affinity_checked
        };
        // Export after timing using the unchanged original artifact writer.
        export(&result);
        let raw = fs::read_to_string(format!("target/benchmark-smoke/{filename}")).unwrap();
        let kind = match (calibrating, warming) {
            (true, true) => "calibration_warmup",
            (true, false) => "calibration",
            (false, true) => "warmup",
            (false, false) => "sample",
        };
        println!(
            "\nRSE_SAMPLE {{\"kind\":\"{kind}\",\"case\":\"{key}\",\"revision\":\"{}\",\"index\":{record_index},\"iterations\":{count},\"wall_ns\":{wall_ns},\"timed_ns\":{timed_ns},\"complete\":{complete},\"target_met\":{target_met},\"isolated_case\":{isolated_case},\"calibration_batch\":{calibration_batch},\"fixture\":{fixture},\"workload\":{workload},\"placement\":{placement},\"before\":{before},\"after\":{after},\"rows\":{}}}",
            revision(),
            raw.replace(['\n', '\r'], "")
        );
    }
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn bounded_procfs_read_rejects_truncation_and_matches_all_text() {
        let mut buffer = [0u8; 4096];
        let actual = read_bounded("/proc/thread-self/stat", &mut buffer).unwrap();
        assert!(actual.contains(") "));
        assert!(actual.ends_with('\n'));
        let mut tiny = [0u8; 1];
        assert_eq!(
            read_bounded("/proc/thread-self/stat", &mut tiny)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let mut empty = [];
        assert_eq!(
            read_bounded("/proc/thread-self/stat", &mut empty)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn affinity_masks_distinguish_single_cpu_from_sets() {
        assert_eq!(
            affinity_list("Name: test\nCpus_allowed_list:\t24\n"),
            Some("24")
        );
        for mask in ["24-31", "24,26", ""] {
            assert!(
                affinity_list(&format!("Cpus_allowed_list:\t{mask}\n"))
                    .unwrap()
                    .parse::<usize>()
                    .is_err()
            );
        }
        assert_eq!(affinity_list("Name: test\n"), None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn post_timer_affinity_failure_retains_completed_invalid_observation() {
        fn set_mask(mask: &str, tid: &std::ffi::OsStr) {
            assert!(
                std::process::Command::new("taskset")
                    .args(["--pid", "--cpu-list", mask])
                    .arg(tid)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        let mut buffer = [0u8; 4096];
        let original =
            affinity_list(read_bounded("/proc/thread-self/status", &mut buffer).unwrap())
                .unwrap()
                .to_owned();
        if original.parse::<usize>().is_ok() {
            eprintln!(
                "affinity expansion regression requires the multi-CPU mask used by formal captures"
            );
            return;
        }
        let cpu = original
            .split([',', '-'])
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let tid = fs::read_link("/proc/thread-self").unwrap();
        struct Restore {
            mask: String,
            tid: std::path::PathBuf,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                set_mask(&self.mask, self.tid.file_name().unwrap());
            }
        }
        let restore = Restore {
            mask: original,
            tid,
        };
        set_mask(&cpu.to_string(), restore.tid.file_name().unwrap());
        let p = Placement {
            cpu,
            constructor_budget: 1,
            constructor_affinity: vec![cpu],
            pool_threads: 4,
        };
        let (observations, outcome) = measure_fixed(
            "affinity-failure",
            1,
            &mut |_| {
                set_mask(&restore.mask, restore.tid.file_name().unwrap());
                Output {
                    result: 42usize,
                    fixture: OptionalFixture(None),
                    workload: OptionalWorkload(None),
                }
            },
            &|_| 1.0,
            OptionalPlacement(Some(&p), true),
        );
        assert!(outcome.is_err());
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].result, 42);
        assert!(observations[0].wall_ns > 0);
        assert!(!observations[0].affinity_checked);
        assert!(
            OptionalPlacement(Some(&p), false)
                .to_string()
                .contains("\"affinity_checked\":false")
        );
    }

    #[test]
    fn kernel_keeps_the_complete_fixed_schedule() {
        let mut calls = 0;
        let (observations, outcome) = measure_fixed(
            "test",
            1,
            &mut |_| {
                calls += 1;
                Output {
                    result: calls,
                    fixture: OptionalFixture(None),
                    workload: OptionalWorkload(None),
                }
            },
            &|_| 1.0,
            OptionalPlacement(None, true),
        );
        assert!(outcome.is_ok());
        assert_eq!(calls, WARMUPS + SAMPLES);
        assert_eq!(observations.len(), calls);
    }

    #[test]
    fn kernel_preserves_completed_observations_before_failure() {
        let mut calls = 0;
        let (observations, outcome) = measure_fixed(
            "test",
            1,
            &mut |_| {
                calls += 1;
                assert!(calls != 3, "intentional workload failure");
                Output {
                    result: calls,
                    fixture: OptionalFixture(None),
                    workload: OptionalWorkload(None),
                }
            },
            &|_| 1.0,
            OptionalPlacement(None, true),
        );
        assert!(outcome.is_err());
        assert_eq!(
            observations.iter().map(|r| r.result).collect::<Vec<_>>(),
            [1, 2]
        );
    }
}
