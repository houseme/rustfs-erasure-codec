//! Shared external integration-test sampler; no codec implementation is replaced.
use std::cell::Cell;
use std::collections::BTreeMap;
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

fn telemetry() -> String {
    let Ok(stat) = fs::read_to_string("/proc/thread-self/stat") else {
        return "null".into();
    };
    let cpu = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(36)
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let counters: Vec<u64> = fs::read_to_string("/proc/thread-self/schedstat")
        .unwrap()
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(counters.len(), 3);
    let extra = if std::env::var("RSE_SAMPLER_DIAGNOSTIC_PROCESS")
        .ok()
        .as_deref()
        == Some("1")
    {
        format!(",\"process\":{}", super::process::snapshot())
    } else {
        String::new()
    };
    format!(
        "{{\"cpu\":{cpu},\"run_ns\":{},\"wait_ns\":{},\"switches\":{}{extra}}}",
        counters[0], counters[1], counters[2]
    )
}

fn common_iterations(key: &str) -> usize {
    static COUNTS: OnceLock<BTreeMap<String, usize>> = OnceLock::new();
    let counts = COUNTS.get_or_init(|| {
        let text = fs::read_to_string(std::env::var("RSE_SAMPLER_COUNTS").unwrap()).unwrap();
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let (name, count) = line.split_once('\t').unwrap();
            let count: usize = count.parse().unwrap();
            assert!((1..=MAX_ITERATIONS).contains(&count));
            assert!(values.insert(name.to_owned(), count).is_none());
        }
        values
    });
    *counts
        .get(key)
        .expect("all cases need frozen common iteration counts")
}

pub fn run<R>(
    key: &str,
    filename: &str,
    mut operation: impl FnMut(usize) -> R,
    ns_per_iter: impl Fn(&R) -> f64,
    export: impl Fn(&R),
) {
    let selected = std::env::var("RSE_SAMPLER_CASE").ok();
    if selected.as_deref().is_some_and(|case| case != key) {
        return;
    }
    let isolated_case = selected.is_some();
    let calibrating = std::env::var("RSE_SAMPLER_MODE").unwrap() == "calibrate";
    let count = if calibrating {
        let count = std::env::var("RSE_SAMPLER_INITIAL_COUNT")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!((1..=MAX_ITERATIONS).contains(&count));
        count
    } else {
        common_iterations(key)
    };
    let calibration_batch = if calibrating {
        let batch = std::env::var("RSE_SAMPLER_BATCH")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!((1..=MAX_CALIBRATION_BATCHES).contains(&batch));
        batch
    } else {
        0
    };
    let mut all_targets_met = true;
    for record_index in 0..WARMUPS + SAMPLES {
        ITERATIONS.with(|n| n.set(count));
        let before = telemetry();
        let start = Instant::now();
        let result = std::hint::black_box(operation(count));
        let wall_ns = start.elapsed().as_nanos();
        let after = telemetry();
        let timed_ns = ns_per_iter(&result) * count as f64;
        assert!(timed_ns.is_finite() && timed_ns > 0.0);
        let target_met = wall_ns as f64 >= TARGET_WALL_NS && timed_ns >= TARGET_TIMED_NS;
        let warming = record_index < WARMUPS;
        if !warming {
            all_targets_met &= target_met;
        }
        let complete = if calibrating {
            record_index + 1 == WARMUPS + SAMPLES && all_targets_met
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
            "\nRSE_SAMPLE {{\"kind\":\"{kind}\",\"case\":\"{key}\",\"revision\":\"{}\",\"index\":{record_index},\"iterations\":{count},\"wall_ns\":{wall_ns},\"timed_ns\":{timed_ns},\"complete\":{complete},\"target_met\":{target_met},\"isolated_case\":{isolated_case},\"calibration_batch\":{calibration_batch},\"before\":{before},\"after\":{after},\"rows\":{}}}",
            revision(),
            raw.replace(['\n', '\r'], "")
        );
    }
}
