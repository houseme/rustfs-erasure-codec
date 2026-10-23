use super::*;
pub(crate) fn fixture(profile: &str, case: &str) -> Value {
    let records = crate::sampling_contract::calibration_profile::tests::records(profile);
    let mut rows: Vec<_> = records.into_iter().filter(|r| r["case"] == case).collect();
    for row in &mut rows {
        row["kind"] = json!(if row["index"].as_u64().unwrap() < 3 {
            "warmup"
        } else {
            "sample"
        });
        row["calibration_batch"] = json!(0);
    }
    let prepared = crate::sampling_contract::preparation::Preparation::new(&rows, false)
        .unwrap()
        .catalog();
    let raw: String = rows
        .iter()
        .map(|r| format!("RSE_SAMPLE {r}\nRSE_TIMING {r}\n"))
        .collect();
    json!({"rows":rows,"raw":raw,"profile":profile,"case":case,"iterations":1,"revision":"a".repeat(40),
        "calibration":{"reference":[rows[0]["rows"][0],rows[0]["placement"]],"fixture":prepared["fixtures"][case],
            "workload":prepared["workloads"][case],"trace":prepared["traces"][case]},
        "allowed_cpus":[0,1,2,3,4,5,6,7],"expected_page_size":4096})
}
fn execute(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.validate()
}
#[test]
fn all_126_original_workers_share_selection_and_calibration_replay() {
    let mut count = 0;
    for (profile, cases) in crate::sampling_contract::case_catalog() {
        for case in cases {
            let value = fixture(profile, &case);
            let before = value.clone();
            let summary = execute(value.clone()).unwrap();
            assert_eq!(
                summary["metric"],
                crate::release_rows::metric(profile).unwrap()
            );
            assert_eq!(summary["central_spread"], 0.0);
            assert_eq!(value, before);
            count += 1;
        }
    }
    assert_eq!(count, 126);
}
#[test]
fn raw_journals_keep_truncated_tail_rules_and_exact_numeric_agreement() {
    let valid = fixture("small_file", "encode:4:2:1024");
    let raw = valid["raw"].as_str().unwrap();
    let mut value = valid.clone();
    value["raw"] = json!(format!("{raw}RSE_SAMPLE {{"));
    execute(value).unwrap();
    let mut value = valid.clone();
    value["raw"] = json!(format!("{raw}RSE_SAMPLE {{\n"));
    assert!(execute(value).is_err());
    for field in ["index", "wall_ns"] {
        let mut value = valid.clone();
        value["rows"][0][field] = json!(value["rows"][0][field].as_u64().unwrap() as f64);
        assert!(
            execute(value)
                .unwrap_err()
                .to_string()
                .contains("journal differs")
        );
    }
    let mut duplicate = valid.clone();
    duplicate["raw"] = json!(raw.replacen("\"index\":0", "\"index\":999,\"index\":0", 1));
    execute(duplicate).unwrap(); // Preserve the former journal parser's last-key behavior.
    assert!(!same(&json!(0.0), &json!(-0.0)));
    assert!(!same(&json!(1), &json!(1.0)));
}
#[test]
fn frozen_preparation_and_nonmedian_provenance_are_not_optional() {
    let valid = fixture("small_file", "encode:4:2:1024");
    for field in ["reference", "fixture", "workload", "trace"] {
        let mut value = valid.clone();
        value["calibration"][field] = json!({});
        assert!(execute(value).is_err(), "{field}");
    }
    let mut value = valid;
    value["calibration"]["reference"][0]["backend_id"] = json!("changed");
    assert!(
        execute(value)
            .unwrap_err()
            .to_string()
            .contains("incompatible")
    );
}
