//! Frozen v15 contract exported from the same constants used by analysis.
use serde_json::{Value, json};
pub(super) const WARMUPS: usize = 3;
pub(super) const SAMPLES: usize = 9;
pub(super) const WIDTH: usize = WARMUPS + SAMPLES;
pub(super) const MAX_CALIBRATION_BATCHES: u64 = 12;
pub(super) const MAX_CALIBRATION_RECORDS: usize = WIDTH * MAX_CALIBRATION_BATCHES as usize;
pub(super) fn value() -> Value {
    json!({
        "name": "shared-external-duration-sampler",
        "version": 15,
        "samples": SAMPLES,
        "warmups": WARMUPS,
        "case_isolation": "fresh-process-per-count",
        "case_order": "lexicographic",
        "max_calibration_batches": MAX_CALIBRATION_BATCHES,
        "max_calibration_records": MAX_CALIBRATION_RECORDS,
        "minimum_timed_ns": super::service_window::MINIMUM_TIMED_NS,
        "minimum_wall_ns": super::service_window::MINIMUM_WALL_NS,
        "target_timed_ns": super::service_window::TARGET_TIMED_NS,
        "target_wall_ns": super::service_window::TARGET_WALL_NS,
        "service_window_policy": "uniform-duration-before-calibration; all-126-cases-and-paired-layouts; no-count-floor-or-case-exceptions",
        "central_spread_limit": super::CENTRAL_SPREAD_LIMIT,
        "observer_buffer_policy": "bounded-stack-procfs-no-rust-heap",
        "fault_counter_scope": "guest-process-including-workers-excluding-children; no IDs; outside inner timers",
        "result_metadata_policy": "bounded-inline-hotspot-patterns-deferred-formatting",
        "stream_output_policy": "independent-capacity-per-writer-per-iteration",
        "workload_contract": {
            "scope": "all-126-cases",
            "codec_lifetime": "worker",
            "hotspot_input": "worker-seeded-SmallRng-u32-low-byte",
            "state_witness": "generated-byte-count-and-next-two-u64-words",
            "geometry_scope": "unchanged-caller-only; dynamic destinations and internal views untracked"
        },
        "thread_placement": {
            "measuring_cpu": "minimum-reserved-cpu",
            "pin": "thread-only-after-codec-and-pool-preparation",
            "pool_affinity": "complete-reservation",
            "observation_checks": "actual-mask-before-and-after-timer"
        },
        "fixture_layout": {
            "scope": "small_file:all;stream:all;caller-storage-only",
            "codec_lifetime": "worker",
            "dynamic_outputs": "original per-iteration policy; offsets explicitly untracked",
            "entry": "Vec<Vec<u8>>",
            "lifetime": "worker",
            "construction": "caller-buffers-before-codec",
            "offset_moduli": [
                64,
                4096,
                "os_base_page_size"
            ],
            "page_size_source": "controller-sysconf",
            "untracked_cases": "original allocation contract; no layout guarantee"
        },
        "timing_scope": {
            "smoke": "original per-iteration workload",
            "small_file": "operation loops excluding one-time fixture preparation",
            "stream": "operation loops excluding one-time fixture preparation",
            "hotspot": "original inner recovery intervals"
        }
    })
}

#[cfg(test)]
#[test]
fn frozen_v15_protocol_retains_the_original_canonical_digest() {
    use sha2::{Digest, Sha256};
    assert_eq!(
        Sha256::digest(serde_json::to_vec(&value()).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "e9dca6cbcd5d3132a6ef26582a9aef844a63e4ca101ecf14a4aed2f4e29bc21e"
    );
}
