use super::*;
use crate::artifact::snapshot::tests::Directory;
#[cfg(unix)]
#[test]
fn read_only_counter_snapshots_decode_full_u64_and_reject_short_reads() {
    let directory = Directory::new();
    let path = directory.0.join("registers");
    let mut bytes = vec![0; 24];
    bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    bytes[16..24].copy_from_slice(&7_u64.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    let reader = MsrReader {
        files: Vec::from([("2".into(), File::open(&path).unwrap())]),
        registers: Vec::from([("aperf".into(), 8), ("mperf".into(), 16)]),
    };
    let point = reader.snapshot().unwrap();
    assert_eq!(point["cpus"]["2"]["aperf"], u64::MAX);
    assert_eq!(point["cpus"]["2"]["mperf"], 7);
    assert!(point["start_ns"].as_u64().unwrap() <= point["end_ns"].as_u64().unwrap());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::write(&path, &bytes[..20]).unwrap();
    assert!(
        reader
            .snapshot()
            .unwrap_err()
            .to_string()
            .contains("short MSR")
    );
}
#[test]
fn the_frequency_cohort_retains_every_failed_case_in_all_four_repetitions() {
    let report = json!({"input_sha256":{"member":"original"},"cases":[{"profile":"stream","case":"b","repeat":{"exceeds_repeat_limit":true}},{"profile":"smoke","case":"a","repeat":{"exceeds_repeat_limit":true}},{"profile":"small_file","case":"excluded","repeat":{"exceeds_repeat_limit":false}}]});
    let input = baseline::Inputs {
        provenance: Value::Null,
        counts: json!({"smoke":{"a":3},"stream":{"b":5}}),
        calibration: Value::Null,
        digests: Value::Null,
    };
    let cohort = cohort(&report, &input).unwrap();
    assert_eq!(cohort["failed_cases"], json!(["smoke/a", "stream/b"]));
    let steps = cohort["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 8);
    for (index, step) in steps.iter().enumerate() {
        assert_eq!(step["ordinal"], index);
        assert_eq!(step["repetition"], index / 2);
        assert_eq!(step["iterations"], if index % 2 == 0 { 3 } else { 5 });
    }
    let empty = json!({"cases":[],"input_sha256":{}});
    assert!(super::cohort(&empty, &input).is_err());
}

#[test]
fn counter_contract_preserves_the_tsc_bracketing_order() {
    let contract = vm("contract", json!({})).unwrap();
    let registers: Vec<(String, u64)> = serde_json::from_value(contract["msrs"].clone()).unwrap();
    assert_eq!(
        registers,
        vec![
            ("tsc_before".into(), 0x10),
            ("aperf".into(), 0xe8),
            ("mperf".into(), 0xe7),
            ("tsc_after".into(), 0x10)
        ]
    );
}
