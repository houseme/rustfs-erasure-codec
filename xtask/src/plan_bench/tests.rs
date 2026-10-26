use super::*;
use crate::artifact::snapshot::tests::Directory;
use model::{Manifest, Outcome, Receipt, SCHEMA, catalog, check_output};

fn keys() -> Vec<String> {
    vec![
        "reconstruct_plans/prepare_full/gf8/4x4/head/elements65".into(),
        "reconstruct_plans/prepare_none/gf8/4x4/head/elements65".into(),
    ]
}
fn logs() -> String {
    keys()
        .iter()
        .map(|k| format!("Testing {k}\nSuccess\n"))
        .collect()
}
fn fixture() -> (Directory, String, String) {
    let dir = Directory::new();
    let binary = b"frozen binary";
    let archive = b"frozen source";
    let catalog = keys();
    let contract = catalog.join("\n") + "\n";
    let manifest = Manifest {
        schema: SCHEMA.into(),
        source_commit: "a".repeat(40),
        source_tree: "b".repeat(40),
        source_sha256: "c".repeat(64),
        compiler: "stable compiler".into(),
        features: vec!["std".into(), "simd-accel".into()],
        binary_sha256: digest(binary),
        archive_sha256: digest(archive),
        catalog_sha256: digest(contract.as_bytes()),
        catalog,
    };
    fs::write(dir.0.join("binary"), binary).unwrap();
    fs::write(dir.0.join("source.tar.gz"), archive).unwrap();
    artifact::write_json(&dir.0.join("manifest.json"), &manifest).unwrap();
    let pin = crate::fingerprint::file(&dir.0.join("manifest.json")).unwrap();
    fs::write(dir.0.join("worker.stdout"), logs()).unwrap();
    fs::write(dir.0.join("worker.stderr"), b"").unwrap();
    let receipt = Receipt {
        schema: SCHEMA.into(),
        stage: "A1".into(),
        mode: "functional".into(),
        manifest_sha256: pin.clone(),
        stdout_sha256: digest(logs().as_bytes()),
        stderr_sha256: digest(b""),
        outcome: Outcome {
            exit_code: Some(0),
            reason: "exited".into(),
            reaped: true,
        },
        originals_unchanged: true,
    };
    artifact::write_json(&dir.0.join("receipt.json"), &receipt).unwrap();
    let receipt_pin = crate::fingerprint::file(&dir.0.join("receipt.json")).unwrap();
    (dir, pin, receipt_pin)
}
#[test]
fn replay_is_read_only_complete_and_never_qualifies_performance() {
    let (dir, pin, rpin) = fixture();
    let before = source_hash(&dir.0).unwrap();
    let check = verify(&dir.0, &pin, &rpin, "A1").unwrap();
    assert!(check.accepted);
    assert_eq!(check.observed, 2);
    assert!(!check.performance_accepted);
    assert_eq!(source_hash(&dir.0).unwrap(), before);
}
#[test]
fn output_contract_rejects_missing_duplicate_unknown_or_truncated_cases() {
    assert!(check_output(&logs(), &keys()).accepted);
    for text in [
        String::new(),
        logs().replace("Success\n", ""),
        format!("{}{}", logs(), logs()),
        format!("{}Testing unknown\nSuccess\n", logs()),
        format!("{}Success\n", logs()),
        format!("{}time: [1 ns]\n", logs()),
    ] {
        assert!(!check_output(&text, &keys()).accepted, "{text}");
    }
}
#[test]
fn manifest_receipt_binary_archive_and_logs_are_independently_bound() {
    for member in [
        "manifest.json",
        "receipt.json",
        "binary",
        "source.tar.gz",
        "worker.stdout",
        "worker.stderr",
    ] {
        let (dir, pin, rpin) = fixture();
        fs::write(dir.0.join(member), b"changed").unwrap();
        assert!(verify(&dir.0, &pin, &rpin, "A1").is_err(), "{member}");
    }
}
#[test]
fn stage_pin_and_mode_cannot_be_relabelled() {
    let (dir, pin, rpin) = fixture();
    assert!(verify(&dir.0, &pin, &rpin, "A2").is_err());
    assert!(verify(&dir.0, &"0".repeat(64), &rpin, "A1").is_err());
    let mut receipt: serde_json::Value = artifact::read_json(&dir.0.join("receipt.json")).unwrap();
    receipt["mode"] = "timing".into();
    artifact::write_json(&dir.0.join("receipt.json"), &receipt).unwrap();
    let changed = crate::fingerprint::file(&dir.0.join("receipt.json")).unwrap();
    assert!(verify(&dir.0, &pin, &changed, "A1").is_err());
}
#[test]
fn nonzero_exit_timeout_unreaped_or_changed_originals_remain_refusals() {
    for mutation in [0, 1, 2, 3] {
        let (dir, pin, _) = fixture();
        let mut receipt: serde_json::Value =
            artifact::read_json(&dir.0.join("receipt.json")).unwrap();
        match mutation {
            0 => receipt["outcome"]["exit_code"] = 1.into(),
            1 => receipt["outcome"]["reason"] = "timeout".into(),
            2 => receipt["outcome"]["reaped"] = false.into(),
            _ => receipt["originals_unchanged"] = false.into(),
        };
        artifact::write_json(&dir.0.join("receipt.json"), &receipt).unwrap();
        let rpin = crate::fingerprint::file(&dir.0.join("receipt.json")).unwrap();
        assert!(!verify(&dir.0, &pin, &rpin, "A1").unwrap().accepted);
    }
}
#[test]
fn catalog_requires_unique_canonical_keys_and_exact_binary_inventory() {
    let text = keys().join("\n") + "\n";
    assert_eq!(catalog(&text).unwrap(), keys());
    for bad in [
        text.replace('\n', "\r\n"),
        format!("{text}{text}"),
        text.trim_end().into(),
        "other/key\n".into(),
    ] {
        assert!(catalog(&bad).is_err());
    }
    let listed = keys()
        .iter()
        .map(|k| format!("{k}: benchmark\n"))
        .collect::<String>();
    assert!(validate_list(&listed, &keys()).is_ok());
    assert!(validate_list(&format!("{listed}{listed}"), &keys()).is_err());
}
#[cfg(unix)]
#[test]
fn symlinked_capsule_members_are_refused() {
    use std::os::unix::fs::symlink;
    let (dir, pin, rpin) = fixture();
    fs::rename(dir.0.join("worker.stderr"), dir.0.join("saved")).unwrap();
    symlink("saved", dir.0.join("worker.stderr")).unwrap();
    assert!(verify(&dir.0, &pin, &rpin, "A1").is_err());
}
#[cfg(unix)]
#[test]
fn owned_worker_timeout_cancellation_and_spawn_failure_preserve_logs() {
    use std::sync::atomic::AtomicBool;
    for cancelled in [false, true] {
        let dir = Directory::new();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf partial; sleep 20"]);
        let result = run::execute_with_cancel(
            cmd,
            &dir.0,
            "worker",
            Duration::from_millis(30),
            &AtomicBool::new(cancelled),
        )
        .unwrap();
        assert_eq!(
            result.reason,
            if cancelled { "cancelled" } else { "timeout" }
        );
        assert!(result.reaped);
        assert!(dir.0.join("worker.stdout").is_file());
    }
    let dir = Directory::new();
    let result = run::execute(
        Command::new("/no/such/plan-worker"),
        &dir.0,
        "worker",
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(result.reason, "spawn_failed");
    assert!(result.reaped);
}

#[test]
fn empty_failure_logs_remain_replayable_and_source_empty_members_stay_invalid() {
    let (dir, pin, _) = fixture();
    fs::write(dir.0.join("worker.stdout"), b"").unwrap();
    let mut receipt: serde_json::Value = artifact::read_json(&dir.0.join("receipt.json")).unwrap();
    receipt["stdout_sha256"] = digest(b"").into();
    artifact::write_json(&dir.0.join("receipt.json"), &receipt).unwrap();
    let rpin = crate::fingerprint::file(&dir.0.join("receipt.json")).unwrap();
    let check = verify(&dir.0, &pin, &rpin, "A1").unwrap();
    assert!(!check.accepted);
    assert_eq!(check.observed, 0);
    let mut originals = Snapshot::new(&dir.0).unwrap();
    assert!(
        originals
            .binary_bound("worker.stdout", &digest(b""))
            .is_err()
    );
    assert!(
        originals
            .raw_bound("worker.stderr", &digest(b""))
            .unwrap()
            .is_empty()
    );
    fs::write(dir.0.join("worker.stderr"), b"late").unwrap();
    assert!(originals.finish().is_err());
}
#[test]
fn duplicate_json_keys_are_refused_even_with_new_trusted_byte_pins() {
    for name in ["manifest.json", "receipt.json"] {
        let (dir, pin, _) = fixture();
        let original = fs::read_to_string(dir.0.join(name)).unwrap();
        let changed = format!(
            "{{\"schema\":\"{SCHEMA}\",{}",
            original.trim_start_matches('{')
        );
        fs::write(dir.0.join(name), changed).unwrap();
        let changed_pin = crate::fingerprint::file(&dir.0.join(name)).unwrap();
        if name == "manifest.json" {
            assert!(load(&dir.0, &changed_pin).is_err());
        } else {
            assert!(verify(&dir.0, &pin, &changed_pin, "A1").is_err());
        }
    }
}
#[test]
fn build_artifacts_require_one_successful_benchmark_from_the_owned_source_and_target() {
    let dir = Directory::new();
    let source = dir.0.join("source");
    let target = dir.0.join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(source.join("Cargo.toml"), "manifest").unwrap();
    fs::create_dir(source.join("benches")).unwrap();
    fs::write(source.join("benches/reconstruct_plans.rs"), "source").unwrap();
    let binary = target.join("bench");
    fs::write(&binary, "binary").unwrap();
    let row = serde_json::json!({"reason":"compiler-artifact", "manifest_path":source.join("Cargo.toml"),"target":{"name":"reconstruct_plans","kind":["bench"],"src_path":source.join("benches/reconstruct_plans.rs")},"executable":binary});
    let finish = "{\"reason\":\"build-finished\",\"success\":true}";
    let valid = format!("{row}\n{finish}\n");
    assert_eq!(
        build_executable(&valid, &source, &target).unwrap(),
        fs::canonicalize(&binary).unwrap()
    );
    for invalid in [
        format!("{row}\n"),
        format!("{row}\n{row}\n{finish}\n"),
        valid.replace("true", "false"),
    ] {
        assert!(build_executable(&invalid, &source, &target).is_err());
    }
    fs::write(dir.0.join("Cargo.toml"), "foreign manifest").unwrap();
    let mut wrong = row.clone();
    wrong["manifest_path"] = dir.0.join("Cargo.toml").to_str().unwrap().into();
    assert!(build_executable(&format!("{wrong}\n{finish}\n"), &source, &target).is_err());
    fs::write(dir.0.join("outside"), "binary").unwrap();
    let mut wrong = row;
    wrong["executable"] = dir.0.join("outside").to_str().unwrap().into();
    assert!(build_executable(&format!("{wrong}\n{finish}\n"), &source, &target).is_err());
}
#[cfg(unix)]
#[test]
fn actual_capture_retains_success_failure_and_empty_logs_without_overwriting() {
    for success in [true, false] {
        let (frozen, _, _) = fixture();
        let output = Directory::new();
        let stage = output.0.join("A1");
        let script = if success {
            format!("#!/bin/sh\nprintf '%s' '{}'\n", logs())
        } else {
            "#!/bin/sh\nexit 7\n".into()
        };
        fs::write(frozen.0.join("binary"), script.as_bytes()).unwrap();
        let mut manifest: serde_json::Value =
            artifact::read_json(&frozen.0.join("manifest.json")).unwrap();
        manifest["binary_sha256"] = digest(script.as_bytes()).into();
        artifact::write_json(&frozen.0.join("manifest.json"), &manifest).unwrap();
        let pin = crate::fingerprint::file(&frozen.0.join("manifest.json")).unwrap();
        let args = Args(
            [
                ("--frozen".into(), frozen.0.display().to_string()),
                ("--manifest-sha256".into(), pin.clone()),
                ("--stage".into(), "A1".into()),
                ("--output".into(), stage.display().to_string()),
            ]
            .into_iter()
            .collect(),
            Default::default(),
        );
        assert_eq!(capture(&args).unwrap(), success);
        let rpin = crate::fingerprint::file(&stage.join("receipt.json")).unwrap();
        assert_eq!(verify(&stage, &pin, &rpin, "A1").unwrap().accepted, success);
        let before = source_hash(&stage).unwrap();
        assert!(capture(&args).is_err());
        assert_eq!(source_hash(&stage).unwrap(), before);
    }
}
