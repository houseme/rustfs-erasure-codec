use super::*;
fn process(tick: u64) -> Value {
    json!({"available":true,"complete":true,"minor_faults":tick,"major_faults":0,"snapshot_ns":10,
    "threads":[{"slot":0,"measuring_thread":true,"cpu":0,"minor_faults":tick,"major_faults":0,"run_ns":tick,"wait_ns":tick,"timeslices":tick}]})
}
fn records() -> Vec<Value> {
    (1..10).map(|i|json!({"kind":"sample","index":i,"case":"fixture","iterations":1,"timed_ns":100.0,"wall_ns":1000,
    "before":{"process":process(i)},"after":{"process":process(i+1)},"retained":{"bytes":[1,2,3]}})).collect()
}
fn run(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.execute()
}
fn clock_point(start: u64, end: u64, offset: u64) -> Value {
    json!({"schema":"native-service-clock-v1","start_ns":start,"end_ns":end,"available":true,"cpu":0,
    "counters":{"tsc_before":100+offset,"tsc_after":101+offset,"aperf":200+offset,"mperf":300+offset}})
}
fn clock_record() -> Value {
    json!({"wall_ns":1000,"placement":{"cpu":0},"before":{"process":{"service_clock":clock_point(0,10,0)}},"after":{"process":{"service_clock":clock_point(1010,1020,2000)}}})
}
#[test]
fn prepare_returns_independent_basic_records_and_the_same_complete_diagnostic() {
    let rows = records();
    let original = rows.clone();
    let mut prepared =
        run(json!({"operation":"prepare","records":rows,"profile":"fixture"})).unwrap();
    let separate = run(json!({"operation":"analyze","records":rows,"profile":"fixture"})).unwrap();
    assert_eq!(prepared["analysis"], separate);
    assert_eq!(
        prepared["analysis"][0]["samples"].as_array().unwrap().len(),
        9
    );
    assert_eq!(
        prepared["analysis"][0]["samples"][0]["process_faults"],
        json!({"minor_faults":1,"major_faults":0})
    );
    assert_eq!(
        prepared["analysis"][0]["samples"][0]["interval_sufficient"],
        false
    );
    assert!(prepared["basic"][0]["before"].get("process").is_none());
    prepared["basic"][0]["retained"]["bytes"][0] = json!(9);
    assert_eq!(rows, original);
}
#[test]
fn incomplete_duplicate_negative_and_untyped_process_snapshots_are_refused() {
    let original = records();
    for mutation in 0..5 {
        let mut rows = original.clone();
        let p = &mut rows[0]["before"]["process"];
        match mutation {
            0 => p["complete"] = json!(false),
            1 => {
                let t = p["threads"][0].clone();
                p["threads"].as_array_mut().unwrap().push(t);
            }
            2 => p["threads"][0]["minor_faults"] = json!(-1),
            3 => p["threads"][0]["measuring_thread"] = json!("true"),
            _ => p["threads"] = json!([]),
        }
        assert!(run(json!({"operation":"basic","records":rows})).is_err());
    }
    let mut rows = original;
    rows.pop();
    assert!(run(json!({"operation":"analyze","profile":"fixture","records":rows})).is_err());
}
#[test]
fn a_reset_in_one_stable_thread_is_never_canceled_by_another_threads_progress() {
    for field in ["run_ns", "wait_ns", "timeslices"] {
        let mut rows = records();
        for (side, value) in [("before", 1), ("after", 201)] {
            let p = &mut rows[0][side]["process"];
            let mut t = p["threads"][0].clone();
            t["slot"] = json!(1);
            t["measuring_thread"] = json!(false);
            t[field] = json!(value);
            p["threads"].as_array_mut().unwrap().push(t);
        }
        rows[0]["before"]["process"]["threads"][0][field] = json!(100);
        run(json!({"operation":"basic","records":rows})).unwrap();
        let error =
            run(json!({"operation":"analyze","profile":"fixture","records":rows})).unwrap_err();
        assert!(error.to_string().contains("thread counters reset"));
    }
}
#[test]
fn partial_thread_sets_retain_only_shared_service_and_original_sample_order() {
    let mut rows = records();
    let mut thread = rows[0]["after"]["process"]["threads"][0].clone();
    thread["slot"] = json!(7);
    thread["measuring_thread"] = json!(false);
    rows[0]["after"]["process"]["threads"]
        .as_array_mut()
        .unwrap()
        .push(thread);
    let report = run(json!({"operation":"analyze","records":rows,"profile":"fixture"})).unwrap();
    assert_eq!(report[0]["samples"][0]["stable_thread_set"], false);
    assert_eq!(report[0]["samples"][0]["thread_count"], 2);
    assert_eq!(report[0]["samples"][0]["shared_thread_run_ms"], 0.000001);
    assert_eq!(report[0]["samples"][0]["index"], 1);
}
#[test]
fn synchronous_windows_keep_unavailable_and_wide_reads_without_invented_ratios() {
    let row = clock_record();
    let report = run(json!({"operation":"window","record":row})).unwrap();
    assert_eq!(report["interpretation_qualified"], true);
    assert_eq!(report["enclosure_overhead_fraction"], 0.02);
    assert_eq!(report["counters"]["0"]["aperf_per_mperf"], 1.0);
    assert_eq!(report["time_normalization"], false);
    assert_eq!(report["exclusive_api_cycles"], false);
    let mut boundary = row.clone();
    boundary["after"]["process"]["service_clock"]["end_ns"] = json!(1050);
    assert_eq!(
        run(json!({"operation":"window","record":boundary})).unwrap()["interpretation_qualified"],
        true
    );
    let mut wide = row.clone();
    wide["after"]["process"]["service_clock"]["end_ns"] = json!(1200);
    let result = run(json!({"operation":"window","record":wide})).unwrap();
    assert_eq!(result["refusal"], "counter-enclosure-too-wide");
    assert!(result["counters"].is_null());
    let mut unavailable = row;
    unavailable["after"]["process"]["service_clock"] = json!({"schema":"native-service-clock-v1","available":false,"start_ns":1010,"end_ns":1020,"reason":"io","errno":5});
    let result = run(json!({"operation":"window","record":unavailable})).unwrap();
    assert_eq!(result["refusal"], "counter-unavailable");
    assert!(result["counters"].is_null());
}
#[test]
fn wrong_cpu_reset_missing_boolean_and_non_enclosing_clock_points_are_refused() {
    for mutation in 0..8 {
        let mut row = clock_record();
        let p = &mut row["after"]["process"]["service_clock"];
        match mutation {
            0 => *p = Value::Null,
            1 => p["unexpected"] = json!(true),
            2 => p["counters"]["aperf"] = json!(true),
            3 => p["end_ns"] = json!(1000),
            4 => p["start_ns"] = json!(900),
            5 => p["cpu"] = json!(1),
            6 => p["counters"]["aperf"] = json!(0),
            _ => {
                p["counters"]["aperf"] = json!(200);
                p["counters"]["mperf"] = json!(300);
            }
        }
        assert!(run(json!({"operation":"window","record":row})).is_err());
    }
}
fn snapshot(tick: u64, start: u64, end: u64, cpus: &[u64]) -> Value {
    json!({"start_ns":start,"end_ns":end,"cpus":cpus.iter().map(|cpu|(cpu.to_string(),json!({"tsc_before":tick*1000,"tsc_after":tick*1000+2,"aperf":tick*400,"mperf":tick*300}))).collect::<std::collections::BTreeMap<_,_>>()})
}
#[test]
fn whole_worker_frequency_preserves_read_skew_raw_ratios_and_idle_cpus() {
    let before = snapshot(1, 1, 2, &[0, 1]);
    let mut after = snapshot(3, 30, 31, &[0, 1]);
    after["cpus"]["1"]["aperf"] = json!(400);
    after["cpus"]["1"]["mperf"] = json!(300);
    let result=run(json!({"operation":"frequency","before":before,"after":after,"cpus":[0,1],"start":3,"end":29})).unwrap();
    assert_eq!(result["0"]["aperf_delta"], 800);
    assert_eq!(result["0"]["mperf_delta"], 600);
    assert_eq!(result["0"]["tsc_delta_bounds"], json!([1998, 2002]));
    assert_eq!(result["0"]["aperf_per_mperf"], json!(4.0 / 3.0));
    assert_eq!(result["0"]["read_skew_tsc"], json!([2, 2]));
    assert!(result["1"]["aperf_per_mperf"].is_null());
    after["cpus"]["0"]["aperf"] = json!(400);
    after["cpus"]["0"]["mperf"] = json!(300);
    assert!(run(json!({"operation":"frequency","before":before,"after":after,"cpus":[0,1],"start":3,"end":29})).is_err());
}
#[test]
fn native_contract_keeps_register_order_and_probe_requires_one_complete_line() {
    let contract = run(json!({"operation":"contract"})).unwrap();
    assert_eq!(
        contract["msrs"],
        json!([
            ["tsc_before", 16],
            ["aperf", 232],
            ["mperf", 231],
            ["tsc_after", 16]
        ])
    );
    let r = clock_record();
    let probe = json!({"wall_ns":r["wall_ns"],"before":r["before"]["process"],"after":r["after"]["process"]});
    let line = format!("RSE_NATIVE_CLOCK_PROBE {probe}\n");
    assert_eq!(
        run(json!({"operation":"probe","text":format!("noise\n{line}ok\n"),"cpu":0})).unwrap()["interpretation_qualified"],
        true
    );
    for text in [
        line.trim_end().to_owned(),
        format!("{line}{line}"),
        format!("{line}RSE_NATIVE_CLOCK_PROBE {{"),
        format!("test name ... {line}"),
    ] {
        assert!(run(json!({"operation":"probe","text":text,"cpu":0})).is_err());
    }
}

#[test]
fn external_frequency_refuses_reset_wrap_missing_cpu_and_invalid_outer_brackets() {
    let before = snapshot(1, 1, 2, &[0]);
    let original = snapshot(3, 30, 31, &[0]);
    for mutation in 0..8 {
        let mut after = original.clone();
        match mutation {
            0 => after["cpus"]["0"]["aperf"] = json!(0),
            1 => after["cpus"]["0"]["mperf"] = json!(0),
            2 => after["cpus"]["0"]["tsc_before"] = json!(0),
            3 => after["cpus"]["0"]["tsc_after"] = json!(0),
            4 => after["cpus"]["0"]["aperf"] = json!(true),
            5 => after["cpus"] = json!({}),
            6 => after["start_ns"] = json!(10),
            _ => {
                after["cpus"]["0"]["aperf"] = json!(400);
                after["cpus"]["0"]["mperf"] = json!(300);
            }
        }
        assert!(run(json!({"operation":"frequency","before":before,"after":after,"cpus":[0],"start":3,"end":29})).is_err());
    }
    for (start, end) in [(json!(true), json!(29)), (json!(3), json!(32))] {
        assert!(run(json!({"operation":"frequency","before":before,"after":original,"cpus":[0],"start":start,"end":end})).is_err());
    }
}
