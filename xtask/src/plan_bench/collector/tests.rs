use super::*;
use crate::artifact::snapshot::tests::Directory;
fn put(path: &Path, value: &Value) -> String {
    artifact::write_json(path, value).unwrap();
    crate::fingerprint::file(path).unwrap()
}
fn script(mode: &str) -> String {
    let key = "reconstruct_plans/prepare_full/gf8/4x4/head/elements65";
    let estimate = |point: f64| json!({"point_estimate":point,"confidence_interval":{"confidence_level":0.95,"lower_bound":point*0.99,"upper_bound":point*1.01},"standard_error":0.0});
    let sample = json!({"sampling_mode":"Linear","iters":[1,2,3,4,5,6,7,8,9,10],"times":[100,200,300,400,500,600,700,800,900,1000]});
    let mut estimates = json!({"mean":estimate(100.0),"median":estimate(100.0),"median_abs_dev":estimate(0.0),"std_dev":estimate(0.0),"slope":estimate(100.0)});
    if mode == "wide" {
        estimates["mean"]["confidence_interval"]["lower_bound"] = 50.into();
        estimates["mean"]["confidence_interval"]["upper_bound"] = 150.into();
    }
    let end = match mode {
        "exit" => "exit 7",
        "hang" => "printf ready > ready; sleep 20 & wait",
        "mutate" => "printf changed > ../../../plan.json",
        "binary" => "rm binary; printf changed > binary",
        "huge" => {
            "dd if=/dev/zero of=\"$CRITERION_HOME/huge\" bs=1 count=0 seek=33554433 2>/dev/null"
        }
        "link" => "ln -s /tmp \"$CRITERION_HOME/link\"",
        _ => ":",
    };
    let normal = format!(
        "#!/bin/sh\n[ \"$1\" = --test ] || exit 42\nmkdir -p \"$CRITERION_HOME/case/new\"\ncat > \"$CRITERION_HOME/case/new/benchmark.json\" <<'JSON'\n{{\"full_id\":\"{key}\"}}\nJSON\ncat > \"$CRITERION_HOME/case/new/sample.json\" <<'JSON'\n{sample}\nJSON\n{}\n{end}\n",
        if mode == "partial" {
            String::new()
        } else {
            format!("cat > \"$CRITERION_HOME/case/new/estimates.json\" <<'JSON'\n{estimates}\nJSON")
        }
    );
    if ["drift", "repeat"].contains(&mode) {
        let sample = json!({"sampling_mode":"Linear","iters":[1,2,3,4,5,6,7,8,9,10],"times":[200,400,600,800,1000,1200,1400,1600,1800,2000]});
        let estimates = json!({"mean":estimate(200.0),"median":estimate(200.0),"median_abs_dev":estimate(0.0),"std_dev":estimate(0.0),"slope":estimate(200.0)});
        let pattern = if mode == "drift" {
            "*/B2"
        } else {
            "*/02/B1|*/02/B2"
        };
        return format!(
            r#"{normal}
case "$PWD" in {pattern})
cat > "$CRITERION_HOME/case/new/sample.json" <<'JSON'
{sample}
JSON
cat > "$CRITERION_HOME/case/new/estimates.json" <<'JSON'
{estimates}
JSON
;; esac
"#
        );
    }
    normal
}

pub(crate) fn fixture(mode: &str) -> (Directory, String) {
    let (root, _) = execution::tests::fixture();
    let mut source_pins = Vec::new();
    for label in ["A", "B"] {
        let directory = root.0.join(format!("corpus/sources/{label}"));
        let bytes = script(if label == "B" { mode } else { "success" });
        fs::write(directory.join("binary"), bytes.as_bytes()).unwrap();
        let mut manifest: Value = artifact::read_json(&directory.join("manifest.json")).unwrap();
        manifest["binary_sha256"] = digest(bytes.as_bytes()).into();
        source_pins.push(put(&directory.join("manifest.json"), &manifest));
    }
    let execution: Value = artifact::read_json(&root.0.join("execution.json")).unwrap();
    let plan = json!({"schema":SCHEMA,"origin":"synthetic","session_sha256":"a".repeat(64),"baseline_sha256":source_pins[0],"candidate_sha256":source_pins[1],"baseline_members":execution["baseline_members"],"renderer_sha256":execution["renderer_sha256"],"analyzer_sha256":execution["analyzer_sha256"],"repetitions":1,"minimum_samples":10,"confidence_level":0.95,"max_ci_width":0.1,"max_drift":0.05,"warm_up_seconds":1,"measurement_seconds":1,"stage_seconds":30,"window_seconds":300,"gap_seconds":30});
    let pin = put(&root.0.join("plan.json"), &plan);
    (root, pin)
}
fn capture_fixture(source: &Path, out: &Path, pin: &str, cancelled: &AtomicBool) -> Result<bool> {
    capture_owned(source, out, pin, cancelled, &mut |_| {
        Err("synthetic worker must not request live resources".into())
    })
}
fn receipt(root: &Path) -> (String, Receipt) {
    let p = root.join("collection.receipt.json");
    (
        crate::fingerprint::file(&p).unwrap(),
        artifact::read_json(&p).unwrap(),
    )
}
#[test]
fn supervised_success_binds_fixed_arguments_original_artifacts_and_read_only_replay() {
    let (source, pin) = fixture("success");
    let parent = Directory::new();
    let out = parent.0.join("capture");
    assert!(capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap());
    let (rpin, r) = receipt(&out);
    assert_eq!(r.steps.len(), 4);
    assert!(r.completed);
    assert_eq!(r.steps.iter().map(|s| s.copies.len()).sum::<usize>(), 12);
    let before = source_hash(&out).unwrap();
    let check = replay::verify(&out, &pin, &rpin).unwrap();
    assert!(check.accepted);
    assert!(!check.capture_provenance_verified && !check.performance_accepted);
    assert_eq!(source_hash(&out).unwrap(), before);
    assert!(capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).is_err());
    assert_eq!(source_hash(&out).unwrap(), before);
}
#[test]
fn failed_partial_oversized_and_symlinked_worker_outputs_stop_later_stages() {
    for mode in ["exit", "partial", "huge", "link"] {
        let (source, pin) = fixture(mode);
        let parent = Directory::new();
        let out = parent.0.join("capture");
        assert!(
            !capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap(),
            "{mode}"
        );
        let (rpin, r) = receipt(&out);
        assert!(!r.completed && r.error.is_some());
        assert_eq!(r.steps.len(), 2);
        assert!(!out.join("workers/01/B2").exists());
        assert!(out.join("workers/01/B1/worker.stdout").is_file());
        assert!(out.join("workers/01/B1/outcome.json").is_file());
        assert!(!replay::verify(&out, &pin, &rpin).unwrap().accepted);
        let outcome: Outcome =
            artifact::read_json(&out.join("workers/01/B1/outcome.json")).unwrap();
        if ["huge", "link"].contains(&mode) {
            assert_eq!(outcome.reason, "guard_failed");
            assert!(outcome.reaped);
        }
    }
}
#[test]
fn cancellation_before_spawn_and_mid_worker_is_reaped_without_running_later_stages() {
    let (source, pin) = fixture("hang");
    let parent = Directory::new();
    let out = parent.0.join("early");
    assert!(!capture_fixture(&source.0, &out, &pin, &AtomicBool::new(true)).unwrap());
    assert!(receipt(&out).1.steps.is_empty());
    let out = parent.0.join("mid");
    let cancelled = AtomicBool::new(false);
    let (result, ready) = std::thread::scope(|scope| {
        let watch = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !out.join("workers/01/B1/ready").exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            let ready = out.join("workers/01/B1/ready").exists();
            cancelled.store(true, Ordering::Relaxed);
            ready
        });
        let result = capture_fixture(&source.0, &out, &pin, &cancelled);
        (result, watch.join().unwrap())
    });
    assert!(ready);
    assert!(!result.unwrap());
    let outcome: Outcome = artifact::read_json(&out.join("workers/01/B1/outcome.json")).unwrap();
    assert_eq!(outcome.reason, "cancelled");
    assert!(outcome.reaped);
    assert!(!out.join("workers/01/B2").exists());
}
#[test]
fn timed_out_worker_retains_outcome_and_complete_partial_artifacts() {
    let (source, _) = fixture("hang");
    let mut plan: Value = artifact::read_json(&source.0.join("plan.json")).unwrap();
    plan["stage_seconds"] = 1.into();
    let pin = put(&source.0.join("plan.json"), &plan);
    let parent = Directory::new();
    let out = parent.0.join("capture");
    assert!(!capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap());
    let outcome: Outcome = artifact::read_json(&out.join("workers/01/B1/outcome.json")).unwrap();
    assert_eq!(outcome.reason, "timeout");
    assert!(outcome.reaped);
    assert!(out.join("workers/01/B1/raw/case/new/sample.json").is_file());
    assert!(!out.join("workers/01/B2").exists());
}
#[test]
fn preflight_failure_prevents_any_worker_and_keeps_the_original_plan() {
    let (source, _) = fixture("success");
    let mut plan: Value = artifact::read_json(&source.0.join("plan.json")).unwrap();
    plan["origin"] = "recorded".into();
    let pin = put(&source.0.join("plan.json"), &plan);
    let parent = Directory::new();
    let out = parent.0.join("capture");
    assert!(
        !capture_owned(
            &source.0,
            &out,
            &pin,
            &AtomicBool::new(false),
            &mut |_| Err("Linux partition unavailable".into())
        )
        .unwrap()
    );
    let (_, r) = receipt(&out);
    assert!(r.steps.is_empty());
    assert!(r.error.as_deref().unwrap().contains("Linux partition"));
    assert!(!out.join("workers").exists());
    let (source, _) = fixture("success");
    let mut terminal: Value =
        artifact::read_json(&source.0.join("baseline/qualification.json")).unwrap();
    terminal["baseline_controls_passed"] = false.into();
    let terminal_pin = put(&source.0.join("baseline/qualification.json"), &terminal);
    let mut plan: Value = artifact::read_json(&source.0.join("plan.json")).unwrap();
    plan["baseline_members"]["qualification.json"] = terminal_pin.into();
    let pin = put(&source.0.join("plan.json"), &plan);
    let out = parent.0.join("baseline-refusal");
    assert!(!capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap());
    assert!(receipt(&out).1.steps.is_empty());
    assert!(!out.join("workers").exists());
}
#[test]
fn changed_plan_refuses_the_session_and_never_overwrites_original_inputs() {
    for mode in ["mutate", "binary"] {
        let (source, pin) = fixture(mode);
        let before = source_hash(&source.0).unwrap();
        let parent = Directory::new();
        let out = parent.0.join("capture");
        assert!(
            capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false))
                .map_or(true, |passed| !passed)
        );
        assert!(out.join("collection.receipt.json").is_file());
        assert!(!out.join("workers/01/B2").exists());
        assert_eq!(source_hash(&source.0).unwrap(), before);
    }
}
#[test]
fn failed_receipt_cannot_be_promoted_by_a_forged_derived_summary() {
    let (source, pin) = fixture("exit");
    let parent = Directory::new();
    let out = parent.0.join("capture");
    assert!(!capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap());
    let (rpin, _) = receipt(&out);
    put(
        &out.join("collection.replay.json"),
        &json!({"accepted":true,"capture_provenance_verified":true,"performance_accepted":true}),
    );
    assert!(!replay::verify(&out, &pin, &rpin).unwrap().accepted);
}
#[test]
fn raw_worker_artifact_mutation_is_rejected_even_when_canonical_corpus_is_unchanged() {
    let (source, pin) = fixture("success");
    let parent = Directory::new();
    let out = parent.0.join("capture");
    assert!(capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap());
    let (rpin, _) = receipt(&out);
    fs::write(
        out.join("workers/01/A1/raw/case/new/sample.json"),
        b"changed",
    )
    .unwrap();
    assert!(replay::verify(&out, &pin, &rpin).is_err());
    assert!(replay::verify(&out, &"0".repeat(64), &rpin).is_err());
}

#[test]
fn width_and_available_drift_failures_stop_before_the_next_worker() {
    for (mode, expected) in [("wide", 2), ("drift", 3), ("repeat", 7)] {
        let (source, _) = fixture(mode);
        let mut plan: Value = artifact::read_json(&source.0.join("plan.json")).unwrap();
        if mode == "repeat" {
            plan["repetitions"] = 2.into();
        }
        let pin = put(&source.0.join("plan.json"), &plan);
        let parent = Directory::new();
        let out = parent.0.join("capture");
        assert!(!capture_fixture(&source.0, &out, &pin, &AtomicBool::new(false)).unwrap());
        let (_, r) = receipt(&out);
        assert!(!r.completed);
        assert_eq!(r.steps.len(), expected, "{mode}");
    }
}

#[test]
fn post_worker_deadline_refusal_prevents_the_next_worker() {
    let (source, _) = fixture("success");
    let mut plan: Value = artifact::read_json(&source.0.join("plan.json")).unwrap();
    plan["stage_seconds"] = 1.into();
    let pin = put(&source.0.join("plan.json"), &plan);
    let parent = Directory::new();
    let out = parent.0.join("capture");
    let mut count = 0_u64;
    let mut clock = || {
        let time = match count {
            0 | 1 => 0,
            2 => 1_000_000,
            3 => 10_000_000,
            4 => 1_000_000_001,
            n => 1_000_000_001 + (n - 4) * 20_000_000,
        };
        count += 1;
        Ok(time)
    };
    assert!(
        !capture_timed(
            &source.0,
            &out,
            &pin,
            &AtomicBool::new(false),
            &mut |_| Err("synthetic context".into()),
            &mut clock
        )
        .unwrap()
    );
    let (_, r) = receipt(&out);
    assert!(!r.completed);
    assert_eq!(r.steps.len(), 1);
    assert!(!out.join("workers/01/B1").exists());
    assert!(r.error.as_deref().unwrap().contains("deadline"));
}

#[test]
fn compiler_mismatch_is_refused_before_any_recorded_worker() {
    let (source, _) = fixture("success");
    let mut plan: Value = artifact::read_json(&source.0.join("plan.json")).unwrap();
    let compiler = format!(
        "rustc 1.99.1 (fixture)\nbinary: rustc\ncommit-hash: {}\ncommit-date: 2026-10-13\nhost: x86_64-unknown-linux-gnu\nrelease: 1.99.1\nLLVM version: 23.1.1",
        "a".repeat(40)
    );
    for (label, field) in [("A", "baseline_sha256"), ("B", "candidate_sha256")] {
        let path = source
            .0
            .join(format!("corpus/sources/{label}/manifest.json"));
        let mut value: Value = artifact::read_json(&path).unwrap();
        value["compiler"] = compiler.clone().into();
        plan[field] = put(&path, &value).into();
    }
    for member in ["provenance.json", "qualification.json"] {
        let path = source.0.join("baseline").join(member);
        let mut value: Value = artifact::read_json(&path).unwrap();
        value["context"]["compiler"] = compiler.lines().next().unwrap().into();
        plan["baseline_members"][member] = put(&path, &value).into();
    }
    plan["origin"] = "recorded".into();
    let pin = put(&source.0.join("plan.json"), &plan);
    let parent = Directory::new();
    let out = parent.0.join("capture");
    let live = compiler.replace("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu");
    let mut observed = |frozen: &str| {
        require_compiler(frozen, &live, "aarch64-unknown-linux-gnu")?;
        Err("compiler gate should have refused".into())
    };
    assert!(
        !capture_owned(
            &source.0,
            &out,
            &pin,
            &AtomicBool::new(false),
            &mut observed
        )
        .unwrap()
    );
    let (_, r) = receipt(&out);
    assert!(r.steps.is_empty());
    assert!(r.error.as_deref().unwrap().contains("compiler identity"));
    assert!(!out.join("workers").exists());
}

#[test]
fn failed_clock_observation_still_preserves_the_owned_worker_outcome() {
    let (source, pin) = fixture("success");
    let parent = Directory::new();
    let out = parent.0.join("capture");
    let mut count = 0;
    let mut clock = || {
        count += 1;
        if count == 4 {
            Err("clock observation failed".into())
        } else {
            Ok(count * 1_000_000_u64)
        }
    };
    assert!(
        !capture_timed(
            &source.0,
            &out,
            &pin,
            &AtomicBool::new(false),
            &mut |_| Err("synthetic context".into()),
            &mut clock
        )
        .unwrap()
    );
    let (_, r) = receipt(&out);
    assert!(!r.completed);
    assert_eq!(r.steps.len(), 1);
    assert!(r.steps[0].outcome_sha256.is_some());
    let outcome: Outcome = artifact::read_json(&out.join("workers/01/A1/outcome.json")).unwrap();
    assert!(outcome.reaped);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(!out.join("workers/01/B1").exists());
}
