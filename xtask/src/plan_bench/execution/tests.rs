use super::*;
use crate::artifact::snapshot::tests::Directory;
fn put(path: &Path, value: &Value) -> String {
    artifact::write_json(path, value).unwrap();
    crate::fingerprint::file(path).unwrap()
}
fn resource() -> Value {
    json!({"compiler":"test compiler","cpu_model":"fixture CPU","settings":sampling_contract::protocol::sampling_environment(),
      "runtime":{"page_size":4096,"libc":{"name":"fixture","version":"1"},"allocator_environment":{},"kernel_memory_policy":{"kernel_release":"fixture-kernel","process_thp_disable":0,"thp":{"enabled":"madvise","defrag":"madvise","shmem_enabled":null,"sizes_kib":{"16":{"anonymous":"never","shmem":null},"2048":{"anonymous":"inherit","shmem":null}}}}},
      "isolation":{"scope":"exclusive-vm-cpu-partition","reserved_cpu_count":8,"cpu_affinity":[0,1,2,3,4,5,6,7],"physical_host_exclusivity_proven":false}})
}
pub(crate) fn fixture() -> (Directory, String) {
    let root = Directory::new();
    let (corpus, cpin) = samples::tests::fixture();
    fs::rename(&corpus.0, root.0.join("corpus")).unwrap();
    let original = baseline::tests::fixture();
    fs::rename(&original.0, root.0.join("baseline")).unwrap();
    let mut provenance: Value =
        artifact::read_json(&root.0.join("baseline/provenance.json")).unwrap();
    provenance["schema_version"] = "baseline-qualification-v2".into();
    provenance["context"] = resource();
    provenance["source"] =
        json!({"commit":baseline::BASELINE,"tree":"a".repeat(40),"lock_sha256":"b".repeat(64)});
    provenance["tool"] =
        json!({"commit":"c".repeat(40),"tree":"d".repeat(40),"lock_sha256":"e".repeat(64)});
    let mut protocol = sampling_contract::protocol_value();
    protocol["implementation_sha256"] = "e".repeat(64).into();
    protocol["overlay_sha256"] = "f".repeat(64).into();
    provenance["sampling"] = protocol;
    provenance["release_approval"] = false.into();
    provenance["candidate_attribution"] = Value::Null;
    provenance.as_object_mut().unwrap().remove("protocol");
    put(&root.0.join("baseline/provenance.json"), &provenance);
    provenance["harness_sha256"] = "a".repeat(64).into();
    put(&root.0.join("baseline/provenance.json"), &provenance);
    let mut terminal = provenance.clone();
    terminal["baseline_controls_passed"] = true.into();
    terminal["calibration"] = json!({});
    terminal["phases"] = json!({});
    terminal["repeat_checks"] = json!([]);
    terminal["sampling"]["common_iterations"] = json!({});
    let catalog = sampling_contract::case_catalog();
    for profile in sampling_contract::PROFILES {
        for phase in ["calibration", "A1", "A2"] {
            let name = format!("{phase}/{profile}-samples.json");
            let entry = json!({"path":name,"sha256":crate::fingerprint::file(&root.0.join("baseline").join(&name)).unwrap()});
            if phase == "calibration" {
                terminal["calibration"][profile] = entry;
            } else {
                terminal["phases"][phase][profile] = json!({"samples":entry});
            }
        }
        let rows: Vec<Value> =
            artifact::read_json(&root.0.join(format!("baseline/A1/{profile}-samples.json")))
                .unwrap();
        for key in &catalog[profile] {
            terminal["sampling"]["common_iterations"][profile][key] = json!(1);
            let metric = crate::release_rows::metric(profile).unwrap();
            let row = rows.iter().find(|r| r["case"] == key.as_str()).unwrap();
            let value = row["rows"][0][metric].as_f64().unwrap();
            terminal["repeat_checks"].as_array_mut().unwrap().push(json!({"profile":profile,"case":key,"metric":metric,"first":value,"second":value,"drift":0.0}));
        }
    }
    put(&root.0.join("baseline/qualification.json"), &terminal);
    let members: BTreeMap<_, _> = baseline_catalog()
        .into_iter()
        .map(|name| {
            let pin = crate::fingerprint::file(&root.0.join("baseline").join(&name)).unwrap();
            (name, pin)
        })
        .collect();
    let samples: Value = artifact::read_json(&root.0.join("corpus/samples.json")).unwrap();
    let mut attempts = Vec::new();
    for (index, attempt) in samples["attempts"].as_array().unwrap().iter().enumerate() {
        let repetition = index / 4 + 1;
        let stage = STAGES[index % 4];
        let prefix = root.0.join(format!("contexts/{repetition:02}/{stage}"));
        let offset = index as u64 * 1_000_000;
        let before = put(
            &prefix.join("before.json"),
            &json!({"session_sha256":"a".repeat(64),"observed_ns":offset+1,"context":resource()}),
        );
        let after = put(
            &prefix.join("after.json"),
            &json!({"session_sha256":"a".repeat(64),"observed_ns":offset+900000,"context":resource()}),
        );
        attempts.push(json!({"repetition":repetition,"stage":stage,"manifest_sha256":attempt["manifest_sha256"],"attempt_sha256":digest(&serde_json::to_vec(attempt).unwrap()),"before_sha256":before,"after_sha256":after,"started_ns":offset+2,"ended_ns":offset+800000}));
    }
    let pin = put(
        &root.0.join("execution.json"),
        &json!({"schema":SCHEMA,"session_sha256":"a".repeat(64),"samples_sha256":cpin,"baseline_members":members,"renderer_sha256":"e".repeat(64),"analyzer_sha256":"b".repeat(64),"stage_seconds":1,"window_seconds":10,"gap_seconds":1,"ended_ns":8_000_000,"attempts":attempts}),
    );
    (root, pin)
}
fn mutate(root: &Path, name: &str, f: impl FnOnce(&mut Value)) -> String {
    let path = root.join(name);
    let mut value: Value = artifact::read_json(&path).unwrap();
    f(&mut value);
    put(&path, &value)
}
fn reseal(root: &Path) -> String {
    let mut execution: Value = artifact::read_json(&root.join("execution.json")).unwrap();
    for (index, attempt) in execution["attempts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let prefix = root.join(format!(
            "contexts/{:02}/{}",
            index / 4 + 1,
            STAGES[index % 4]
        ));
        for name in ["before", "after"] {
            attempt[format!("{name}_sha256")] =
                crate::fingerprint::file(&prefix.join(format!("{name}.json")))
                    .unwrap()
                    .into();
        }
    }
    for name in baseline_catalog() {
        execution["baseline_members"][&name] =
            crate::fingerprint::file(&root.join("baseline").join(&name))
                .unwrap()
                .into();
    }
    put(&root.join("execution.json"), &execution)
}
#[test]
fn complete_models_share_one_read_only_snapshot_and_never_approve_performance() {
    let (root, pin) = fixture();
    let before = source_hash(&root.0).unwrap();
    let report = verify(&root.0, &pin, None).unwrap();
    assert!(
        report.bindings_verified && report.baseline_quality_passed,
        "{:?}",
        report.errors
    );
    assert!(
        !report.preflight_passed
            && !report.capture_provenance_verified
            && !report.performance_accepted
    );
    assert_eq!(report.baseline["case_count"], 126);
    assert_eq!(source_hash(&root.0).unwrap(), before);
}
#[test]
fn forged_baseline_success_cannot_hide_original_width_and_repeat_failures() {
    let (root, _) = fixture();
    mutate(&root.0, "baseline/A2/small_file-samples.json", |v| {
        for row in v.as_array_mut().unwrap() {
            row["rows"][0]["ns_per_iter"] = json!(2_000_000_000.0);
            row["timed_ns"] = json!(2_000_000_000_u64);
            row["wall_ns"] = json!(2_000_001_000_u64);
        }
    });
    put(
        &root.0.join("baseline/qualification.json"),
        &json!({"baseline_controls_passed":true,"release_approval":true}),
    );
    let pin = reseal(&root.0);
    let report = verify(&root.0, &pin, None).unwrap();
    assert!(!report.baseline_quality_passed && !report.bindings_verified);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("recomputed baseline"))
    );
}
#[test]
fn session_context_cpu_policy_and_compiler_changes_cannot_be_resealed_into_success() {
    for kind in 0..4 {
        let (root, _) = fixture();
        mutate(&root.0, "contexts/01/A1/after.json", |v| match kind {
            0 => v["session_sha256"] = "f".repeat(64).into(),
            1 => v["context"]["cpu_model"] = "changed CPU".into(),
            2 => v["context"]["isolation"]["reserved_cpu_count"] = 2.into(),
            _ => v["context"]["compiler"] = "changed compiler".into(),
        });
        let pin = reseal(&root.0);
        assert!(verify(&root.0, &pin, None).map_or(true, |r| !r.bindings_verified));
    }
}
#[test]
fn source_attempt_order_and_full_baseline_member_catalog_are_bound() {
    for kind in 0..4 {
        let (root, _) = fixture();
        let pin = mutate(&root.0, "execution.json", |v| match kind {
            0 => v["attempts"][0]["attempt_sha256"] = "0".repeat(64).into(),
            1 => v["attempts"][0]["manifest_sha256"] = "0".repeat(64).into(),
            2 => v["attempts"].as_array_mut().unwrap().swap(0, 1),
            _ => {
                v["baseline_members"]
                    .as_object_mut()
                    .unwrap()
                    .remove("A2/smoke-samples.json");
            }
        });
        assert!(verify(&root.0, &pin, None).map_or(true, |r| !r.bindings_verified));
    }
}
#[test]
fn overlapping_discontinuous_and_expired_windows_are_rejected_without_underflow() {
    for kind in 0..4 {
        let (root, _) = fixture();
        let pin = match kind {
            0 => mutate(&root.0, "execution.json", |v| {
                v["attempts"][1]["started_ns"] = 0.into()
            }),
            1 => {
                mutate(&root.0, "contexts/01/B1/before.json", |v| {
                    v["observed_ns"] = json!(2_000_000_000_u64)
                });
                reseal(&root.0)
            }
            2 => mutate(&root.0, "execution.json", |v| {
                v["ended_ns"] = u64::MAX.into()
            }),
            _ => mutate(&root.0, "execution.json", |v| {
                v["attempts"][0]["ended_ns"] = u64::MAX.into()
            }),
        };
        assert!(!verify(&root.0, &pin, None).unwrap().bindings_verified);
    }
}
#[test]
fn synthetic_origin_and_live_context_changes_never_pass_preflight() {
    let (root, pin) = fixture();
    let mut calls = 0;
    let mut observe = || {
        calls += 1;
        Ok(resource())
    };
    let r = verify(&root.0, &pin, Some(&mut observe)).unwrap();
    assert_eq!(calls, 2);
    assert!(r.bindings_verified && !r.preflight_passed);
    assert!(r.live_errors.iter().any(|s| s.contains("synthetic")));
    let corpus_pin = mutate(&root.0, "corpus/samples.json", |v| {
        v["origin"] = "recorded".into()
    });
    let recorded_pin = mutate(&root.0, "execution.json", |v| {
        v["samples_sha256"] = corpus_pin.into()
    });
    let mut stable = || Ok(resource());
    let recorded = verify(&root.0, &recorded_pin, Some(&mut stable)).unwrap();
    assert!(recorded.preflight_passed);
    assert!(!recorded.capture_provenance_verified && !recorded.performance_accepted);

    let mut calls = 0;
    let mut observe = || {
        calls += 1;
        let mut r = resource();
        if calls == 2 {
            r["cpu_model"] = "changed CPU".into();
        }
        Ok(r)
    };
    let r = verify(&root.0, &recorded_pin, Some(&mut observe)).unwrap();
    assert!(!r.preflight_passed);
    assert!(r.live_errors.iter().any(|s| s.contains("differs")));
}
#[test]
fn wrong_external_pins_and_changed_original_bytes_fail_closed() {
    let (root, pin) = fixture();
    assert!(verify(&root.0, &"0".repeat(64), None).is_err());
    fs::write(root.0.join("baseline/A1/smoke-samples.json"), b"changed").unwrap();
    assert!(verify(&root.0, &pin, None).is_err());
}

#[test]
fn mutation_during_late_resource_observation_refuses_the_entire_snapshot() {
    let (root, pin) = fixture();
    let mut calls = 0;
    let mut observe = || {
        calls += 1;
        if calls == 2 {
            fs::write(
                root.0.join("baseline/A1/smoke-samples.json"),
                b"late mutation",
            )?;
        }
        Ok(resource())
    };
    assert!(verify(&root.0, &pin, Some(&mut observe)).is_err());
    assert_eq!(calls, 2);
}

#[test]
fn failed_original_terminal_state_stays_failed_even_when_raw_numbers_pass() {
    let (root, _) = fixture();
    mutate(&root.0, "baseline/qualification.json", |v| {
        v["baseline_controls_passed"] = false.into()
    });
    let pin = reseal(&root.0);
    let report = verify(&root.0, &pin, None).unwrap();
    assert!(report.baseline_quality_passed);
    assert!(!report.bindings_verified);
    assert!(report.errors.iter().any(|e| e.contains("terminal status")));
}

#[test]
fn raw_sample_time_cannot_fit_inside_a_forged_short_worker_window() {
    let (root, _) = fixture();
    let pin = mutate(&root.0, "execution.json", |v| {
        v["attempts"][0]["ended_ns"] = json!(3)
    });
    let report = verify(&root.0, &pin, None).unwrap();
    assert!(!report.bindings_verified);
    assert!(report.errors.iter().any(|e| e.contains("raw sample times")));
}
