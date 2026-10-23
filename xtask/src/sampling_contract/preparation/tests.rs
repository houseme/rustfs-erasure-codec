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
