use super::*;
fn records(case: &str) -> Vec<Value> {
    let fields: Vec<_> = case.split(':').collect();
    (0..12).map(|index| {
        let mut row=crate::sampling_contract::tests::record("small_file",case,index);
        row["revision"]=json!("a".repeat(40));
        row["rows"][0].as_object_mut().unwrap().extend(json!({"operation":fields[0],"data_shards":4,"parity_shards":2,"shard_size":fields[3].parse::<u64>().unwrap(),
            "throughput_mb_s":100.0,"schema_version":1,"benchmark_metrics_enabled":false,"artifact_kind":"small-file-results","git_revision":"aaaaaaa","iterations":1,"profile":"fast",
            "target_triple":"fixture-target","backend":"scalar","backend_id":"scalar","backend_kind":"scalar","backend_override":"auto","override_honored":true,"features":"simd-accel|std","origin":index}).as_object().unwrap().clone());
        row
    }).collect()
}
fn request(records: Vec<Value>, output: Value) -> Value {
    let expected: BTreeSet<_> = records
        .iter()
        .map(|r| r["case"].as_str().unwrap().to_owned())
        .collect();
    json!({"profile":"small_file","revision":"a".repeat(40),"counts":expected.iter().map(|c|(c,1)).collect::<BTreeMap<_,_>>(),"expected":expected,
        "records":records,"allowed_cpus":[0,1,2,3,4,5,6,7],"expected_page_size":4096,"output":output})
}
fn execute(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.execute()
}
#[test]
fn interleaved_cases_keep_first_seen_order_original_indices_and_stable_ties() {
    let a = records("encode:4:2:4096");
    let b = records("encode:4:2:1024");
    let data = a.into_iter().zip(b).flat_map(|(a, b)| [a, b]).collect();
    let req = request(
        data,
        json!({"kind":"select","metric":"ns_per_iter","diagnostic":false}),
    );
    let before = req.clone();
    let result = execute(req.clone()).unwrap();
    assert_eq!(req, before);
    assert_eq!(result["groups"][0][0], "encode:4:2:4096");
    assert_eq!(
        result["groups"][0][1],
        json!((0..24).step_by(2).collect::<Vec<_>>())
    );
    assert_eq!(result["selected_indices"], json!([14, 15]));
    assert_eq!(result["spreads"][0]["central_spread"], 0.0);
}
#[test]
fn every_original_row_is_checked_before_selection_and_without_type_coercion() {
    let data = records("encode:4:2:1024");
    for (key, value) in [
        ("schema_version", json!("1")),
        ("iterations", json!(true)),
        ("operation", json!("verify")),
        ("git_revision", json!("bbbbbbb")),
    ] {
        let mut rows = data.clone();
        rows[3]["rows"][0][key] = value;
        assert!(
            execute(request(rows, json!({"kind":"groups"}))).is_err(),
            "{key}"
        );
    }
    let mut rows = data.clone();
    rows.pop();
    assert!(execute(request(rows, json!({"kind":"groups"}))).is_err());
    let mut req = request(data, json!({"kind":"groups"}));
    req["expected"] = json!(["encode:4:2:1024", "encode:4:2:4096"]);
    assert!(execute(req).is_err());
}
#[test]
fn short_samples_are_retained_for_groups_and_diagnosis_but_refused_for_release_selection() {
    let mut data = records("encode:4:2:1024");
    for row in &mut data {
        row["wall_ns"] = json!(1);
    }
    execute(request(data.clone(), json!({"kind":"groups"}))).unwrap();
    let req = request(
        data.clone(),
        json!({"kind":"select","metric":"ns_per_iter","diagnostic":false}),
    );
    assert!(
        execute(req)
            .unwrap_err()
            .to_string()
            .contains("interval is insufficient")
    );
    let report = execute(request(
        data.clone(),
        json!({"kind":"select","metric":"ns_per_iter","diagnostic":true}),
    ))
    .unwrap();
    assert_eq!(
        report["spreads"][0]["short_samples"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    let report = execute(request(data, json!({"kind":"describe","fixtures":{}}))).unwrap();
    let detail = &report["descriptions"][0]["detail"];
    assert_eq!(detail["short_samples"], 9);
    assert!(detail["measured_service"]["main_run_central_spread"].is_null());
    assert!(detail["trace"][3]["wall_minus_timed_ns"].as_f64().unwrap() < 0.0);
}
#[test]
fn integer_selection_keeps_precision_and_the_selected_original_observation() {
    let mut data = records("encode:4:2:1024");
    for r in &mut data {
        r["rows"][0]["ns_per_iter"] = json!(9007199254740993_u64);
        r["timed_ns"] = r["rows"][0]["ns_per_iter"].clone();
    }
    data[3]["rows"][0]["ns_per_iter"] = json!(9007199254740992.0);
    data[3]["timed_ns"] = json!(9007199254740992.0);
    let result = execute(request(
        data,
        json!({"kind":"select","metric":"ns_per_iter","diagnostic":false}),
    ))
    .unwrap();
    assert_eq!(result["selected_indices"], json!([7]));
}
