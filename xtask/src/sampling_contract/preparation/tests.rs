use super::*;

fn row(case: &str, batch: u64, index: u64, rng: Value) -> Value {
    json!({"case":case,"calibration_batch":batch,"index":index,
        "kind":"sample","complete":false,"fixture":null,
        "placement":{"cpu":0},"workload":{"seed":18446744073709551615_u64,"rng":rng}})
}

#[test]
fn final_batch_means_last_observed_per_case_with_original_trace_order() {
    let mut records = vec![
        row("a", 9, 0, json!({"next_before":[9,0]})),
        row("b", 1, 0, Value::Null),
        row("a", 2, 0, json!({"next_before":[2,0]})),
        row("b", 1, 1, Value::Null),
        row("a", 2, 1, Value::Null),
        row("a", 2, 2, json!({"next_before":[2,2]})),
    ];
    records[0]["fixture"] = json!({"old":true});
    records[0]["complete"] = json!(true);
    records[2]["fixture"] = json!({"current":true});
    records[2]["complete"] = json!(true);
    records[5]["fixture"] = json!({"incomplete":true});
    records[2]["placement"] = Value::Null;
    let original = records.clone();
    let catalog = Preparation::new(&records, true).unwrap().catalog();
    assert_eq!(records, original);
    assert_eq!(
        catalog,
        json!({
        "fixtures":{"a":{"current":true}},
        "workloads":{
            "a":{"seed":18446744073709551615_u64,"initial_rng":[2,0],"placement":null},
            "b":{"seed":18446744073709551615_u64,"initial_rng":null,"placement":{"cpu":0}}},
        "traces":{"a":[{"next_before":[2,0]},null,{"next_before":[2,2]}]}})
    );
}

#[test]
fn measurement_keeps_null_trace_positions_and_only_sample_fixtures() {
    let mut records = vec![
        row("a", 0, 0, Value::Null),
        row("a", 0, 1, json!({"next_before":[1,1]})),
        row("b", 0, 0, Value::Null),
    ];
    records[0]["fixture"] = json!({"old":true});
    records[1]["fixture"] = json!({"warmup":true});
    records[1]["kind"] = json!("warmup");
    records[2]["fixture"] = json!({"sample":true});
    let catalog = Preparation::new(&records, false).unwrap().catalog();
    assert_eq!(
        catalog["fixtures"],
        json!({"a":{"old":true},"b":{"sample":true}})
    );
    assert_eq!(catalog["traces"], json!({"a":[null,{"next_before":[1,1]}]}));
    assert_eq!(catalog["workloads"]["a"]["initial_rng"], Value::Null);
    assert_eq!(
        Preparation::new(&[], false).unwrap().catalog(),
        json!({"fixtures":{},"workloads":{},"traces":{}})
    );
}

#[test]
fn projections_require_case_names_and_index_zero_workload_objects() {
    for rows in [
        vec![json!({})],
        vec![json!({"case":3})],
        vec![json!({"case":"a","index":0,"workload":null})],
    ] {
        assert!(Preparation::new(&rows, false).is_err());
    }
}

#[test]
fn all_126_preparation_catalogs_match_the_frozen_python_projections() {
    use sha2::{Digest, Sha256};
    let mut catalogs = BTreeMap::new();
    let mut cases = 0;
    for (profile, names) in crate::sampling_contract::case_catalog() {
        let mut rows: Vec<_> = names
            .iter()
            .flat_map(|case| {
                (0..12).map(move |i| crate::sampling_contract::tests::record(profile, case, i))
            })
            .collect();
        cases += names.len();
        let measurement = Preparation::new(&rows, false).unwrap().catalog();
        for row in &mut rows {
            row["complete"] = json!(row["index"] == 11);
            row["kind"] = json!(if row["index"].as_u64().unwrap() < 3 {
                "calibration_warmup"
            } else {
                "calibration"
            });
            row["calibration_batch"] = json!(1);
        }
        let calibration = Preparation::new(&rows, true).unwrap().catalog();
        catalogs.insert(
            profile,
            json!({"measurement":measurement,"calibration":calibration}),
        );
    }
    assert_eq!(cases, 126);
    // Sorted compact JSON from the Python projectors at main 0a85348. RNG
    // previews use the native fixture's index witnesses; all other fields match.
    let digest = Sha256::digest(serde_json::to_vec(&catalogs).unwrap());
    let digest = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "d44fff1895ee2b824dea1362ee84d6ce16706da47daed844a257107d882c77da"
    );
}

#[test]
fn worker_starts_keep_all_rehearsals_in_original_order() {
    let mut rows = vec![
        row("a", 1, 0, Value::Null),
        row("b", 1, 0, Value::Null),
        row("a", 1, 1, Value::Null),
        row("a", 2, 0, Value::Null),
    ];
    for (r, count) in rows.iter_mut().zip([1, 1, 1, 64]) {
        r["iterations"] = json!(count);
    }
    let original = rows.clone();
    let prepared = Preparation::new(&rows, true).unwrap();
    assert_eq!(
        json!(prepared.worker_starts),
        json!([
        {"case":"a","iterations":1,"calibration_batch":1},
        {"case":"b","iterations":1,"calibration_batch":1},
        {"case":"a","iterations":64,"calibration_batch":2}])
    );
    assert_eq!(rows, original);
}
