use super::*;
fn request() -> Value {
    let replay = crate::sampling_contract::replay::tests::fixture("small_file", "encode:4:2:1024");
    let step = json!({"ordinal":0,"profile":"small_file","case":"encode:4:2:1024","iterations":1,"repetition":0});
    let context =
        json!({"isolation":{"cpu_affinity":[0,1,2,3,4,5,6,7]},"runtime":{"page_size":4096}});
    let point = |tick, start, end| json!({"start_ns":start,"end_ns":end,"cpus":(0..8).map(|cpu| (cpu.to_string(),json!({"tsc_before":tick*1000_u64,"tsc_after":tick*1000_u64+2,"aperf":tick*400_u64,"mperf":tick*300_u64}))).collect::<std::collections::BTreeMap<_,_>>()});
    let begin = json!({"step":step,"before":context,"start_ns":100,"counters_before":point(1,1,2)});
    let entry = json!({"step":step,"before":context,"after":context,"start_ns":100,"end_ns":30_000_000_100_u64,
        "counters_before":point(1,1,2),"counters_after":point(2,30_000_000_150_u64,30_000_000_160_u64),"samples_sha256":"a".repeat(64),"log_sha256":"b".repeat(64)});
    json!({"replay":replay,"step":step,"entry":entry,"begin":begin,"context":context,"last_end":0})
}
fn execute(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.execute()
}
#[test]
fn complete_worker_keeps_original_service_fields_and_counter_enclosure() {
    let value = request();
    let original = value.clone();
    let result = execute(value.clone()).unwrap();
    assert_eq!(result["worker_elapsed_ns"], 30_000_000_000_u64);
    assert_eq!(result["original_wall_ns"], 28_800_000_000_u64);
    assert_eq!(result["original_timed_ns"], 2_400_000_000.0);
    assert_eq!(result["counters"]["0"]["mperf_delta"], 300);
    assert_eq!(result["service"]["timed_ns_per_iteration"], 200_000_000.0);
    assert_eq!(value, original);
}
#[test]
fn reordered_context_windows_and_measuring_cpu_inactivity_are_refused() {
    for mode in 0..8 {
        let mut value = request();
        match mode {
            0 => value["entry"]["step"]["ordinal"] = json!(1),
            1 => value["entry"]["after"] = Value::Null,
            2 => value["begin"]["start_ns"] = json!(99),
            3 => value["last_end"] = json!(101),
            4 => value["entry"]["end_ns"] = json!(1000),
            5 => {
                value["entry"]["counters_after"]["cpus"]["0"]["mperf"] = json!(300);
                value["entry"]["counters_after"]["cpus"]["0"]["aperf"] = json!(400);
            }
            6 => value["replay"]["case"] = json!("other"),
            _ => value["context"]["runtime"]["page_size"] = json!(8192),
        }
        assert!(execute(value).is_err(), "{mode}");
    }
}

#[test]
fn interval_exports_preserve_integer_and_float_types() {
    let mut value = request();
    let rows = value["replay"]["rows"].as_array_mut().unwrap();
    for row in &mut *rows {
        row["wall_ns"] = json!(row["wall_ns"].as_u64().unwrap() as f64);
        row["timed_ns"] = json!(row["timed_ns"].as_f64().unwrap() as u64);
    }
    let raw: String = rows
        .iter()
        .map(|row| format!("RSE_SAMPLE {row}\nRSE_TIMING {row}\n"))
        .collect();
    value["replay"]["raw"] = json!(raw);
    let result = execute(value).unwrap();
    assert!(result["original_wall_ns"].is_f64());
    assert_eq!(result["original_wall_ns"], 28_800_000_000.0);
    assert!(result["original_timed_ns"].is_u64());
    assert_eq!(result["original_timed_ns"], 2_400_000_000_u64);
}
