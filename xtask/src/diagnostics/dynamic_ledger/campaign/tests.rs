use super::*;
fn fixture() -> Value {
    let candidate =
        json!({"commit":"c".repeat(40),"tree":"d".repeat(40),"lock_sha256":"f".repeat(64)});
    let baseline = json!({"commit":"9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6","tree":"e".repeat(40),"lock_sha256":"f".repeat(64)});
    let context = json!({"compiler":"rustc fixture","machine":"fixture","cpu_identity":"fixture","cpu_affinity":null,"page_size":4096,
        "kernel_memory_policy":null,"libc":["",""],"allocator_environment":{},"settings":sampling_contract::protocol::sampling_environment()});
    let mut report = json!({"schema":"dynamic-buffer-check-v1","release_approval":false,"candidate_attribution":null,"formal_performance_geometry_equivalence":false,
        "tool":candidate,"sources":{"candidate":candidate,"baseline":baseline},"cases":{"candidate":{},"baseline":{}},
        "source_overlays":{"candidate":{},"baseline":{}},"harness_sha256":{"candidate":"a".repeat(64),"baseline":"a".repeat(64)},
        "overlay_sha256":{"candidate":"b".repeat(64),"baseline":"b".repeat(64)},"context":context,"page_size":4096,"cpu_affinity":null,
        "native_sha256":"1".repeat(64),"sampler_implementation_sha256":"2".repeat(64),"controller_sha256":"3".repeat(64)});
    let mut plan = report.clone();
    plan["steps"] = json!(&*STEPS);
    let mut events = Vec::new();
    for step in &*STEPS {
        let role = step["role"].as_str().unwrap();
        let key = format!(
            "{}/{}/{}",
            step["mode"].as_str().unwrap(),
            step["profile"].as_str().unwrap(),
            step["case"].as_str().unwrap()
        );
        let entry = json!({"iterations":2,"events":1,"bytes":u64::MAX,"ratio":-0.0});
        report["cases"][role][&key] = entry.clone();
        events.push(json!({"stage":"begin","step":step,"context":context}).to_string());
        events.push(json!({"stage":"end","step":step,"context":context,"entry":entry}).to_string());
    }
    json!({"report":report,"plan":plan,"execution":events.join("\n")+"\n"})
}
fn run(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.execute()
}
#[test]
fn complete_plan_and_every_boundary_preserve_the_original_values() {
    assert_eq!(steps().len(), 516);
    let value = fixture();
    let before = value.clone();
    assert_eq!(run(value.clone()).unwrap(), value["report"]["sources"]);
    assert_eq!(value, before);
    let mut crlf = value.clone();
    crlf["execution"] = json!(value["execution"].as_str().unwrap().replace('\n', "\r\n"));
    run(crlf).unwrap();
    assert!(equivalent(
        &json!({"n":[1,-0.0,true,u64::MAX]}),
        &json!({"n":[1.0,0.0,1,u64::MAX]})
    ));
    assert!(!equivalent(
        &json!(u64::MAX),
        &json!(18446744073709551616.0)
    ));
    for (a, b, expected) in [
        (
            json!(-9007199254740993i64),
            json!(-9007199254740992i64),
            false,
        ),
        (
            json!(-9007199254740993i64),
            json!(-9007199254740992.0),
            false,
        ),
        (json!(i64::MIN), json!(i64::MIN as f64), true),
        (json!(i64::MIN + 1), json!(i64::MIN as f64), false),
        (json!(i64::MIN), json!(u64::MAX), false),
    ] {
        assert_eq!(equivalent(&a, &b), expected, "{a} / {b}");
        assert_eq!(equivalent(&b, &a), expected, "{b} / {a}");
    }
}
#[test]
fn altered_plan_runtime_source_or_incomplete_envelope_is_refused() {
    for mode in 0..10 {
        let mut value = fixture();
        match mode {
            0 => {
                value["plan"]["steps"].as_array_mut().unwrap().pop();
            }
            1 => value["report"]["extra"] = json!(1),
            2 => value["report"]["context"]["settings"]["RAYON_NUM_THREADS"] = json!("1"),
            3 => value["report"]["tool"]["commit"] = json!("--help"),
            4 => value["report"]["context"]["allocator_environment"] = json!({"SECRET":"x"}),
            5 => value["report"]["cpu_affinity"] = json!([1, 0]),
            6 => value["report"]["context"]["kernel_memory_policy"] = json!({}),
            7 => value["report"]["page_size"] = json!(false),
            8 => value["report"]["sources"]["candidate"]["tree"] = json!("e".repeat(40)),
            _ => {
                value["report"]["source_overlays"]
                    .as_object_mut()
                    .unwrap()
                    .remove("baseline");
            }
        }
        assert!(run(value).is_err(), "{mode}");
    }
}
#[test]
fn missing_reordered_or_changed_execution_rows_are_refused() {
    let value = fixture();
    let original: Vec<Value> = value["execution"]
        .as_str()
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    for mode in 0..5 {
        let mut rows = original.clone();
        match mode {
            0 => {
                rows.pop();
            }
            1 => rows.swap(0, 2),
            2 => rows[33]["context"]["settings"]["RAYON_NUM_THREADS"] = json!("1"),
            3 => rows[33]["entry"] = json!({"forged":true}),
            _ => rows.push(rows[0].clone()),
        }
        let mut changed = value.clone();
        changed["execution"] = json!(
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        );
        assert!(run(changed).is_err(), "{mode}");
    }
}

#[test]
fn plan_and_capture_settings_match_frozen_python_values() {
    use crate::artifact::snapshot::digest;
    assert_eq!(
        digest(&serde_json::to_vec(steps()).unwrap()),
        "a2eb3bea835104be73d157e6e0dd04f0e0ad8fd0730a33cf3e265da4bf17c52e"
    );
    let settings = json!({"base":sampling_contract::protocol::base_environment(),"sampling":sampling_contract::protocol::sampling_environment()});
    assert_eq!(
        digest(&serde_json::to_vec(&settings).unwrap()),
        "8c312afbd715e3846412951e0500423811d71e7e45f668790fc17181382f8927"
    );
}

#[test]
fn native_memory_policy_and_legacy_execution_numeric_equality_are_retained() {
    let mut value = fixture();
    let mut context = value["report"]["context"].clone();
    context["cpu_affinity"] = json!([0, 1]);
    context["kernel_memory_policy"] = json!({"kernel_release":"fixture","process_thp_disable":0,
        "thp":{"enabled":"madvise","defrag":"madvise","shmem_enabled":null,"sizes_kib":{}}});
    value["report"]["context"] = context.clone();
    value["report"]["cpu_affinity"] = json!([0, 1]);
    value["plan"]["context"] = context.clone();
    value["plan"]["cpu_affinity"] = json!([0, 1]);
    let mut rows: Vec<Value> = value["execution"]
        .as_str()
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    for row in &mut rows {
        row["context"] = context.clone();
        row["context"]["page_size"] = json!(4096.0);
        row["context"]["cpu_affinity"] = json!([false, 1.0]);
        if row["stage"] == "end" {
            row["entry"]["iterations"] = json!(2.0);
            row["entry"]["ratio"] = json!(0.0);
        }
    }
    value["execution"] = json!(
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    );
    run(value.clone()).unwrap();
    value["report"]["context"]["kernel_memory_policy"]["process_thp_disable"] = json!(true);
    assert!(run(value).is_err());
}

#[test]
fn fixed_plan_covers_both_sources_every_case_and_all_decoder_supplements_once() {
    use std::collections::BTreeSet;
    let catalog = sampling_contract::case_catalog();
    let expected: BTreeSet<_> = sampling_contract::PROFILES
        .into_iter()
        .flat_map(|profile| {
            catalog[profile]
                .iter()
                .map(move |case| (profile, case.as_str()))
        })
        .collect();
    let supplements: BTreeSet<_> = super::super::payload::SUPPLEMENTS
        .iter()
        .map(|s| s.case)
        .collect();
    for role in ROLES {
        for mode in MODES {
            let cases: BTreeSet<_> = steps()
                .iter()
                .filter(|s| s["role"] == role && s["mode"] == mode)
                .map(|s| (s["profile"].as_str().unwrap(), s["case"].as_str().unwrap()))
                .collect();
            assert_eq!(cases, expected);
            assert_eq!(cases.len(), 126);
        }
        let decoder: BTreeSet<_> = steps()
            .iter()
            .filter(|s| s["role"] == role && s["profile"] == "decoder")
            .map(|s| s["case"].as_str().unwrap())
            .collect();
        assert_eq!(decoder, supplements);
    }
    let unique: BTreeSet<_> = steps().iter().map(Value::to_string).collect();
    assert_eq!(unique.len(), steps().len());
}
