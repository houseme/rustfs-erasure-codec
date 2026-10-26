use super::*;
use crate::artifact::snapshot::tests::Directory;
use serde_json::{Value, json};

const KEY: &str = "reconstruct_plans/prepare_full/gf8/4x4/head/elements65";
fn put(path: &Path, value: &Value) -> String {
    artifact::write_json(path, value).unwrap();
    crate::fingerprint::file(path).unwrap()
}
fn estimate(point: f64) -> Value {
    json!({"confidence_interval":{"confidence_level":0.95,"lower_bound":point*0.99,"upper_bound":point*1.01},"point_estimate":point,"standard_error":0.1})
}
fn raw(factor: f64) -> (Value, Value) {
    let iters: Vec<_> = (1..=10).map(f64::from).collect();
    let times: Vec<_> = iters.iter().map(|i| (99.0 + i) * i * factor).collect();
    let slope = iters.iter().zip(&times).map(|(i, t)| i * t).sum::<f64>()
        / iters.iter().map(|i| i * i).sum::<f64>();
    (
        json!({"sampling_mode":"Linear","iters":iters,"times":times}),
        json!({"mean":estimate(104.5*factor),"median":estimate(104.5*factor),"median_abs_dev":estimate(2.5*factor),"std_dev":estimate(3.0276503540974917*factor),"slope":estimate(slope)}),
    )
}
pub(crate) fn fixture() -> (Directory, String) {
    let dir = Directory::new();
    let mut pins = Vec::new();
    for (label, revision) in [("A", "a"), ("B", "b")] {
        let path = dir.0.join("sources").join(label);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("binary"), b"bound test binary").unwrap();
        fs::write(path.join("source.tar.gz"), b"bound test archive").unwrap();
        pins.push(put(&path.join("manifest.json"), &json!({"schema":model::SCHEMA,"source_commit":revision.repeat(40),"source_tree":"c".repeat(40),"source_sha256":"d".repeat(64),"compiler":"test compiler","features":["std","simd-accel"],"binary_sha256":digest(b"bound test binary"),"archive_sha256":digest(b"bound test archive"),"catalog_sha256":digest(format!("{KEY}\n").as_bytes()),"catalog":[KEY]})));
    }
    let mut attempts = Vec::new();
    for repetition in 1..=2 {
        for stage in crate::abba::STAGES {
            let path = dir.0.join(format!("records/{repetition:02}/{stage}/0000"));
            let (sample, estimates) = raw(1.0);
            let b = put(
                &path.join("benchmark.json"),
                &json!({"full_id":KEY,"other_metadata":"retained in original bytes"}),
            );
            let s = put(&path.join("sample.json"), &sample);
            let e = put(&path.join("estimates.json"), &estimates);
            attempts.push(json!({"repetition":repetition,"stage":stage,"manifest_sha256":pins[usize::from(stage.starts_with('B'))],"outcome":{"exit_code":0,"reason":"exited","reaped":true},"originals_unchanged":true,"cases":[{"case_index":0,"benchmark_sha256":b,"sample_sha256":s,"estimates_sha256":e}]}));
        }
    }
    let pin = put(
        &dir.0.join("samples.json"),
        &json!({"schema":"plan-bench-statistical-inputs-v1","origin":"synthetic","baseline_sha256":pins[0],"candidate_sha256":pins[1],"repetitions":2,"minimum_samples":10,"confidence_level":0.95,"max_ci_width":0.1,"max_drift":0.05,"attempts":attempts}),
    );
    (dir, pin)
}
fn check(dir: &Path, pin: &str) -> Result<Report> {
    verify(dir, pin)
}
fn mutate(dir: &Path, member: &str, f: impl FnOnce(&mut Value)) -> String {
    let path = dir.join(member);
    let mut v: Value = artifact::read_json(&path).unwrap();
    f(&mut v);
    put(&path, &v)
}
fn reseal(dir: &Path) -> String {
    let mut document: Value = artifact::read_json(&dir.join("samples.json")).unwrap();
    for (i, attempt) in document["attempts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        for case in attempt["cases"].as_array_mut().unwrap() {
            let index = case["case_index"].as_u64().unwrap();
            for (field, name) in [
                ("benchmark_sha256", "benchmark.json"),
                ("sample_sha256", "sample.json"),
                ("estimates_sha256", "estimates.json"),
            ] {
                let path = dir.join(format!(
                    "records/{:02}/{}/{index:04}/{name}",
                    i / 4 + 1,
                    crate::abba::STAGES[i % 4]
                ));
                if case[field].is_string() && path.exists() {
                    case[field] = crate::fingerprint::file(&path).unwrap().into();
                }
            }
        }
    }
    put(&dir.join("samples.json"), &document)
}
#[test]
fn complete_repetitions_preserve_distributions_without_performance_qualification() {
    let (dir, pin) = fixture();
    let before = source_hash(&dir.0).unwrap();
    let report = check(&dir.0, &pin).unwrap();
    assert!(report.integrity_accepted && report.diagnostics_passed);
    assert!(!report.performance_accepted && !report.capture_provenance_verified);
    assert_eq!(report.observations.len(), 8);
    assert_eq!(report.observations[0].distribution.mean_ns, 104.5);
    assert_eq!(report.observations[0].distribution.samples, 10);
    assert_eq!(source_hash(&dir.0).unwrap(), before);
}
#[test]
fn self_consistent_width_and_drift_failures_remain_visible_across_all_repetitions() {
    let (dir, _) = fixture();
    mutate(&dir.0, "records/01/A1/0000/estimates.json", |v| {
        v["mean"]["confidence_interval"]["lower_bound"] = 50.into();
        v["mean"]["confidence_interval"]["upper_bound"] = 150.into();
    });
    let (sample, estimates) = raw(2.0);
    put(&dir.0.join("records/02/A2/0000/sample.json"), &sample);
    put(&dir.0.join("records/02/A2/0000/estimates.json"), &estimates);
    let pin = reseal(&dir.0);
    let report = check(&dir.0, &pin).unwrap();
    assert!(report.integrity_accepted && !report.diagnostics_passed);
    assert_eq!(report.observations.len(), 8);
    assert!(
        report
            .diagnostic_failures
            .iter()
            .any(|s| s.contains("CI width"))
    );
    assert!(
        report
            .diagnostic_failures
            .iter()
            .any(|s| s.contains("A2/A1"))
    );
    assert!(
        report
            .diagnostic_failures
            .iter()
            .any(|s| s.contains("baseline repeat"))
    );
}
#[test]
fn failed_and_incomplete_attempts_cannot_be_replaced_by_selected_successes() {
    let (dir, _) = fixture();
    let pin = mutate(&dir.0, "samples.json", |v| {
        v["attempts"][2]["outcome"]["reason"] = "cancelled".into();
        v["attempts"][2]["outcome"]["exit_code"] = Value::Null;
        v["attempts"][2]["cases"] = json!([]);
    });
    put(
        &dir.0.join("summary.json"),
        &json!({"integrity_accepted":true,"performance_accepted":true}),
    );
    let report = check(&dir.0, &pin).unwrap();
    assert!(!report.integrity_accepted);
    assert_eq!(report.attempts.len(), 8);
    assert_eq!(report.observations.len(), 7);
    assert!(report.errors.iter().any(|s| s.contains("worker")));
    assert!(report.errors.iter().any(|s| s.contains("catalog")));
}
#[test]
fn case_identity_repeat_order_pair_and_member_digests_are_bound() {
    for mutation in 0..5 {
        let (dir, pin) = fixture();
        let changed = match mutation {
            0 => mutate(&dir.0, "samples.json", |v| {
                v["attempts"][0]["manifest_sha256"] = "0".repeat(64).into()
            }),
            1 => mutate(&dir.0, "samples.json", |v| {
                v["attempts"].as_array_mut().unwrap().swap(0, 1)
            }),
            2 => mutate(&dir.0, "samples.json", |v| {
                v["attempts"][4]["repetition"] = 1.into()
            }),
            3 => {
                mutate(&dir.0, "records/01/A1/0000/benchmark.json", |v| {
                    v["full_id"] = "other/case".into()
                });
                reseal(&dir.0)
            }
            _ => {
                fs::write(dir.0.join("records/01/A1/0000/sample.json"), b"changed").unwrap();
                pin
            }
        };
        assert!(
            check(&dir.0, &changed).map_or(true, |r| !r.integrity_accepted),
            "mutation {mutation}"
        );
    }
}
#[test]
fn nonfinite_fractional_iterations_wrong_means_and_short_samples_are_refused() {
    for mutation in 0..5 {
        let (dir, _) = fixture();
        match mutation {
            0 => {
                mutate(&dir.0, "records/01/A1/0000/sample.json", |v| {
                    v["iters"][0] = 0.into()
                });
            }
            1 => {
                mutate(&dir.0, "records/01/A1/0000/sample.json", |v| {
                    v["iters"][0] = 1.5.into()
                });
            }
            2 => {
                mutate(&dir.0, "records/01/A1/0000/estimates.json", |v| {
                    v["mean"]["point_estimate"] = 105.into()
                });
            }
            3 => {
                mutate(&dir.0, "records/01/A1/0000/sample.json", |v| {
                    v["iters"].as_array_mut().unwrap().pop();
                    v["times"].as_array_mut().unwrap().pop();
                });
            }
            _ => {
                fs::write(
                    dir.0.join("records/01/A1/0000/sample.json"),
                    b"{\"sampling_mode\":\"Flat\",\"iters\":[1e999],\"times\":[1]}",
                )
                .unwrap();
            }
        }
        let pin = reseal(&dir.0);
        assert!(
            check(&dir.0, &pin).map_or(true, |r| !r.integrity_accepted),
            "mutation {mutation}"
        );
    }
}
#[test]
fn missing_duplicate_unknown_and_noncanonical_case_catalogs_are_rejected() {
    for mutation in 0..4 {
        let (dir, _) = fixture();
        let pin = mutate(&dir.0, "samples.json", |v| match mutation {
            0 => v["attempts"][0]["cases"] = json!([]),
            1 => {
                let case = v["attempts"][0]["cases"][0].clone();
                v["attempts"][0]["cases"].as_array_mut().unwrap().push(case);
            }
            2 => v["attempts"][0]["cases"][0]["case_index"] = 999.into(),
            _ => v["attempts"][0]["cases"][0]["sample_sha256"] = Value::Null,
        });
        assert!(!check(&dir.0, &pin).unwrap().integrity_accepted);
    }
}
#[test]
fn external_pins_schema_unknown_fields_and_duplicate_keys_are_strict() {
    let (dir, pin) = fixture();
    assert!(check(&dir.0, &"0".repeat(64)).is_err());
    for field in ["performance_accepted", "unsupported"] {
        let (dir, _) = fixture();
        let pin = mutate(&dir.0, "samples.json", |v| v[field] = true.into());
        assert!(check(&dir.0, &pin).is_err());
    }
    let path = dir.0.join("samples.json");
    let raw = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        format!("{{\"origin\":\"synthetic\",{}", raw.trim_start_matches('{')),
    )
    .unwrap();
    assert!(check(&dir.0, &crate::fingerprint::file(&path).unwrap()).is_err());
    assert!(check(&dir.0, &pin).is_err());
}
#[cfg(unix)]
#[test]
fn raw_symlinks_are_refused_without_touching_the_inputs() {
    use std::os::unix::fs::symlink;
    let (dir, pin) = fixture();
    let path = dir.0.join("records/01/A1/0000/sample.json");
    fs::rename(&path, dir.0.join("saved.json")).unwrap();
    symlink(dir.0.join("saved.json"), &path).unwrap();
    let before = fs::read_link(&path).unwrap();
    assert!(check(&dir.0, &pin).is_err());
    assert_eq!(fs::read_link(&path).unwrap(), before);
}

#[test]
fn flat_and_degenerate_samples_preserve_zero_variance_and_require_the_right_slope() {
    let (mut sample, mut estimates) = raw(1.0);
    sample["sampling_mode"] = "Flat".into();
    sample["iters"] = json!(vec![1; 10]);
    sample["times"] = json!((100..110).collect::<Vec<_>>());
    estimates["slope"] = Value::Null;
    let distribution = criterion::analyze(sample.clone(), estimates.clone(), 10, 0.95).unwrap();
    assert_eq!(distribution.mean_ns, 104.5);
    estimates["slope"] = estimate(104.5);
    assert!(criterion::analyze(sample.clone(), estimates.clone(), 10, 0.95).is_err());
    sample["times"] = json!(vec![100; 10]);
    estimates = json!({"mean":estimate(100.0),"median":estimate(100.0),"median_abs_dev":estimate(0.0),"std_dev":estimate(0.0),"slope":null});
    assert_eq!(
        criterion::analyze(sample, estimates, 10, 0.95)
            .unwrap()
            .mean_ns,
        100.0
    );
}
#[test]
fn exact_iteration_bounds_and_finite_extreme_values_do_not_silently_round_or_overflow() {
    let (mut sample, _) = raw(1.0);
    sample["sampling_mode"] = "Flat".into();
    sample["iters"] = json!(vec![1; 10]);
    sample["times"] = json!(vec![1e300; 10]);
    let estimates = json!({"mean":estimate(1e300),"median":estimate(1e300),"median_abs_dev":estimate(0.0),"std_dev":estimate(0.0),"slope":null});
    assert!(
        criterion::analyze(sample.clone(), estimates.clone(), 10, 0.95)
            .unwrap()
            .mean_ns
            .is_finite()
    );
    sample["iters"] = json!(vec![9_007_199_254_740_993_u64; 10]);
    assert!(criterion::analyze(sample, estimates, 10, 0.95).is_err());
}
#[test]
fn existing_criterion_originals_recompute_without_relabelling_their_history() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for stage in crate::abba::STAGES {
        let directory=root.join(format!("benchmarks/recovery-paths/2026-10-17-final-vs-c68-vm/{stage}/false-missing-first-data/new"));
        let sample: Value = artifact::read_json(&directory.join("sample.json")).unwrap();
        let estimates: Value = artifact::read_json(&directory.join("estimates.json")).unwrap();
        let confidence = estimates["mean"]["confidence_interval"]["confidence_level"]
            .as_f64()
            .unwrap();
        let distribution = criterion::analyze(sample, estimates, 10, confidence).unwrap();
        assert_eq!(distribution.samples, 10);
    }
}
