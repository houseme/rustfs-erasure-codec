use super::*;
use serde_json::json;

pub(crate) fn records(profile: &str) -> Vec<Value> {
    let mut rows = Vec::new();
    for case in &crate::sampling_contract::case_catalog()[profile] {
        for index in 0..protocol::WIDTH {
            let mut record = crate::sampling_contract::tests::record(profile, case, index as u64);
            record["revision"] = json!("a".repeat(40));
            record["kind"] = json!(if index < protocol::WARMUPS {
                "calibration_warmup"
            } else {
                "calibration"
            });
            record["complete"] = json!(index == protocol::WIDTH - 1);
            record["target_met"] = json!(true);
            record["calibration_batch"] = json!(1);
            let row = &mut record["rows"][0];
            row.as_object_mut().unwrap().extend(json!({"schema_version":1,"benchmark_metrics_enabled":false,
                "artifact_kind":match profile {"smoke"=>"smoke-results","small_file"=>"small-file-results","hotspot"=>"reconstruction-hotspot-results",_=>"stream-path-results"},
                "git_revision":"aaaaaaa","iterations":1,"profile":if profile=="smoke" {"extended"}else{"fast"},
                "target_triple":"fixture-target","backend":"scalar","backend_id":"scalar","backend_kind":"scalar",
                "backend_override":"auto","override_honored":true,"features":"std|simd-accel"}).as_object().unwrap().clone());
            if profile == "hotspot" {
                row["scenario"] = json!(case);
                row["baseline_mb_s"] = json!(5.0);
                row["speedup"] = json!(1.0);
            } else {
                let fields: Vec<_> = case.split(':').collect();
                row["operation"] = json!(fields[0]);
                for (key, value) in ["data_shards", "parity_shards", "shard_size"]
                    .into_iter()
                    .zip(&fields[1..4])
                {
                    row[key] = json!(value.parse::<u64>().unwrap());
                }
                row["throughput_mb_s"] = json!(100.0);
                if profile == "stream" {
                    row["stream_block_size"] = json!(fields[4].parse::<u64>().unwrap());
                    row["ns_per_block"] = json!(200_000_000.0);
                    row["stream_io_mode"] = json!("auto");
                }
            }
            rows.push(record);
        }
    }
    rows
}
#[test]
fn all_profiles_keep_original_order_counts_and_inputs_without_normalizing_rows() {
    for profile in ["smoke", "small_file", "hotspot", "stream"] {
        let rows = records(profile);
        let before = rows.clone();
        let counts = analyze(&rows, profile, &"a".repeat(40), Some(4096)).unwrap();
        assert_eq!(rows, before);
        let output = counts.as_array().unwrap();
        assert_eq!(output.len() * protocol::WIDTH, rows.len());
        assert_eq!(output[0][0], rows[0]["case"]);
        assert!(output.iter().all(|r| r[1] == 1));
    }
}
#[test]
fn an_incomplete_or_completed_rehearsal_cannot_hide_missing_or_extra_records() {
    let rows = records("small_file");
    let mut changed = rows.clone();
    changed.pop();
    assert!(analyze(&changed, "small_file", &"a".repeat(40), Some(4096)).is_err());
    let mut changed = rows.clone();
    changed.extend_from_slice(&rows[..12]);
    assert!(
        analyze(&changed, "small_file", &"a".repeat(40), Some(4096))
            .unwrap_err()
            .to_string()
            .contains("extra calibration")
    );
    for (key, value) in [
        ("iterations", json!(true)),
        ("isolated_case", json!(false)),
        ("complete", json!(false)),
        ("calibration_batch", json!(0)),
    ] {
        let mut changed = rows.clone();
        changed[11][key] = value;
        assert!(
            analyze(&changed, "small_file", &"a".repeat(40), Some(4096)).is_err(),
            "{key}"
        );
    }
    let mut changed = rows;
    changed[1]["rows"][0]["backend_id"] = json!("changed");
    assert!(analyze(&changed, "small_file", &"a".repeat(40), Some(4096)).is_err());
}
#[test]
fn growth_is_driven_by_the_worst_probe_and_requires_a_fresh_complete_worker() {
    let rows = records("small_file");
    let case = rows[0]["case"].clone();
    let mut group = rows[..12].to_vec();
    for row in &mut group {
        row["wall_ns"] = json!(1_000_000);
        row["timed_ns"] = json!(1_000_000);
        row["rows"][0]["ns_per_iter"] = json!(1_000_000);
        row["complete"] = json!(false);
        row["target_met"] = json!(false);
    }
    let mut next = rows[..12].to_vec();
    for row in &mut next {
        row["iterations"] = json!(64);
        row["rows"][0]["iterations"] = json!(64);
        row["timed_ns"] = json!(200_000_000_u64 * 64);
        row["calibration_batch"] = json!(2);
    }
    group.extend(next);
    group.extend_from_slice(&rows[12..]);
    let counts = analyze(&group, "small_file", &"a".repeat(40), Some(4096)).unwrap();
    assert_eq!(counts[0], json!([case, 64]));
    group[12]["iterations"] = json!(1);
    assert!(analyze(&group, "small_file", &"a".repeat(40), Some(4096)).is_err());
}
