//! Shared external integration-test sampler; no codec implementation is replaced.
use super::layout::OptionalFixture;
use std::cell::Cell;
use std::fs;
use std::sync::OnceLock;
use std::time::Instant;

const TARGET_WALL_NS: f64 = 100_000_000.0;
const TARGET_TIMED_NS: f64 = 5_000_000.0;
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
struct Telemetry {
    available: bool,
    cpu: u64,
    run_ns: u64,
    wait_ns: u64,
    switches: u64,
    process: Option<String>,
}

impl std::fmt::Display for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.available {
            return f.write_str("null");
        }
        write!(
            f,
            "{{\"cpu\":{},\"run_ns\":{},\"wait_ns\":{},\"switches\":{}",
            self.cpu, self.run_ns, self.wait_ns, self.switches
        )?;
        if let Some(process) = &self.process {
            write!(f, ",\"process\":{process}")?;
        }
        f.write_str("}")
    }
}

fn telemetry() -> Telemetry {
    let Ok(stat) = fs::read_to_string("/proc/thread-self/stat") else {
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
    let sched = fs::read_to_string("/proc/thread-self/schedstat").unwrap();
    let mut counters = sched.split_whitespace().map(|s| s.parse::<u64>().unwrap());
    let (run_ns, wait_ns, switches) = (
        counters.next().unwrap(),
        counters.next().unwrap(),
        counters.next().unwrap(),
    );
    assert!(counters.next().is_none());
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
        process,
    }
}

struct Observation<R> {
    result: R,
    wall_ns: u128,
    timed_ns: f64,
    before: Telemetry,
    after: Telemetry,
    fixture: OptionalFixture,
}

impl<R> Observation<R> {
    fn target_met(&self) -> bool {
        self.wall_ns as f64 >= TARGET_WALL_NS && self.timed_ns >= TARGET_TIMED_NS
    }
}

pub(super) struct Output<R> {
    pub(super) result: R,
    pub(super) fixture: OptionalFixture,
}

fn measure_fixed<R>(
    key: &str,
    count: usize,
    operation: &mut impl FnMut(usize) -> Output<R>,
    ns_per_iter: &impl Fn(&R) -> f64,
) -> (Vec<Observation<R>>, std::thread::Result<()>) {
    let mut observations = Vec::with_capacity(WARMUPS + SAMPLES);
    // A failed operation is rethrown after completed observations are exported.
    // No further workload runs against possibly interrupted mutable state.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for index in 0..WARMUPS + SAMPLES {
            ITERATIONS.with(|n| n.set(count));
            let before = telemetry();
            let start = Instant::now();
            let Output { result, fixture } = std::hint::black_box(operation(count));
            let wall_ns = start.elapsed().as_nanos();
            let after = telemetry();
            let timed_ns = ns_per_iter(&result) * count as f64;
            assert!(timed_ns.is_finite() && timed_ns > 0.0);
            // Keep a mode-independent timing journal even if a process is
            // interrupted before the deferred original artifact export.
            println!(
                "\nRSE_TIMING {{\"case\":\"{key}\",\"index\":{index},\"iterations\":{count},\"wall_ns\":{wall_ns},\"timed_ns\":{timed_ns},\"fixture\":{fixture},\"before\":{before},\"after\":{after}}}"
            );
            observations.push(Observation {
                result,
                wall_ns,
                timed_ns,
                before,
                after,
                fixture,
            });
        }
    }));
    (observations, outcome)
}

pub fn run<R>(
    key: &str,
    filename: &str,
    mut operation: impl FnMut(usize) -> R,
    ns_per_iter: impl Fn(&R) -> f64,
    export: impl Fn(&R),
) {
    run_prepared(
        key,
        filename,
        || {
            move |n| Output {
                result: operation(n),
                fixture: OptionalFixture(None),
            }
        },
        ns_per_iter,
        export,
    );
}

pub(super) fn run_prepared<R, F: FnMut(usize) -> Output<R>>(
    key: &str,
    filename: &str,
    prepare: impl FnOnce() -> F,
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
    let mut operation = prepare();
    let (observations, outcome) = measure_fixed(key, count, &mut operation, &ns_per_iter);
    let complete_batch = observations.len() == WARMUPS + SAMPLES
        && observations
            .iter()
            .skip(WARMUPS)
            .all(Observation::target_met);
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
        } = observation;
        let warming = record_index < WARMUPS;
        let complete = if calibrating {
            record_index + 1 == WARMUPS + SAMPLES && complete_batch
        } else {
            target_met
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
            "\nRSE_SAMPLE {{\"kind\":\"{kind}\",\"case\":\"{key}\",\"revision\":\"{}\",\"index\":{record_index},\"iterations\":{count},\"wall_ns\":{wall_ns},\"timed_ns\":{timed_ns},\"complete\":{complete},\"target_met\":{target_met},\"isolated_case\":{isolated_case},\"calibration_batch\":{calibration_batch},\"fixture\":{fixture},\"before\":{before},\"after\":{after},\"rows\":{}}}",
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
                }
            },
            &|_| 1.0,
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
                }
            },
            &|_| 1.0,
        );
        assert!(outcome.is_err());
        assert_eq!(
            observations.iter().map(|r| r.result).collect::<Vec<_>>(),
            [1, 2]
        );
    }
}
