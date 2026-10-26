use super::*;
use crate::artifact::snapshot::tests::Directory;

fn keys() -> Vec<String> {
    vec!["reconstruct_plans/prepare_full/gf8/4x4/head/elements65".into()]
}
fn logs() -> String {
    format!("Testing {}\nSuccess\n", keys()[0])
}
fn frozen(label: &str, script: &str) -> (Directory, String) {
    let directory = Directory::new();
    let manifest = Manifest {
        schema: model::SCHEMA.into(),
        source_commit: label.repeat(40),
        source_tree: "c".repeat(40),
        source_sha256: "d".repeat(64),
        compiler: "test compiler".into(),
        features: vec!["std".into(), "simd-accel".into()],
        binary_sha256: digest(script.as_bytes()),
        archive_sha256: digest(b"test archive"),
        catalog_sha256: digest((keys().join("\n") + "\n").as_bytes()),
        catalog: keys(),
    };
    fs::write(directory.0.join("binary"), script).unwrap();
    fs::write(directory.0.join("source.tar.gz"), b"test archive").unwrap();
    let pin = save(&directory.0.join("manifest.json"), &manifest).unwrap();
    (directory, pin)
}
fn plan(a: String, b: String) -> Plan {
    Plan {
        schema: SCHEMA.into(),
        mode: "functional".into(),
        nonce: "f".repeat(64),
        baseline_sha256: a,
        candidate_sha256: b,
        stage_seconds: 2,
        window_seconds: 10,
        gap_seconds: 1,
    }
}
fn fixture() -> (Directory, String, String) {
    let root = Directory::new();
    let (a, apin) = frozen("a", "test binary a");
    let (b, bpin) = frozen("b", "test binary b");
    let plan = plan(apin, bpin);
    let ppin = save(&root.0.join("campaign.json"), &plan).unwrap();
    let mut previous = ppin.clone();
    let mut links = Vec::new();
    for (i, stage) in STAGES.iter().enumerate() {
        let source = if i == 0 || i == 3 { &a } else { &b };
        let frozen = load(&source.0, plan.pin(i)).unwrap();
        let dir = root.0.join(stage);
        install(&frozen, &dir).unwrap();
        frozen.finish().unwrap();
        fs::write(dir.join("worker.stdout"), logs()).unwrap();
        fs::write(dir.join("worker.stderr"), b"").unwrap();
        let rpin = save(
            &dir.join("receipt.json"),
            &Receipt {
                schema: model::SCHEMA.into(),
                stage: (*stage).into(),
                mode: "functional".into(),
                manifest_sha256: plan.pin(i).into(),
                stdout_sha256: digest(logs().as_bytes()),
                stderr_sha256: digest(b""),
                outcome: Outcome {
                    exit_code: Some(0),
                    reason: "exited".into(),
                    reaped: true,
                },
                originals_unchanged: true,
            },
        )
        .unwrap();
        let start_pin = save(
            &dir.join("start.json"),
            &Start {
                plan_sha256: ppin.clone(),
                stage: (*stage).into(),
                previous_sha256: previous,
                started_ns: i as u64 * 100 + 10,
            },
        )
        .unwrap();
        let end_pin = save(
            &dir.join("end.json"),
            &End {
                start_sha256: start_pin.clone(),
                ended_ns: i as u64 * 100 + 50,
                receipt_sha256: Some(rpin),
                error: None,
            },
        )
        .unwrap();
        links.push(Link {
            start_sha256: start_pin,
            end_sha256: end_pin.clone(),
        });
        previous = end_pin;
    }
    let rpin = save(
        &root.0.join("campaign.receipt.json"),
        &CampaignReceipt {
            schema: SCHEMA.into(),
            plan_sha256: ppin.clone(),
            stages: links,
            ended_ns: 400,
            failure: None,
        },
    )
    .unwrap();
    (root, ppin, rpin)
}
fn change(root: &Path, name: &str, f: impl FnOnce(&mut serde_json::Value)) {
    let path = root.join(name);
    let mut value = artifact::read_json(&path).unwrap();
    f(&mut value);
    artifact::write_json(&path, &value).unwrap();
}
// Re-seal mutated fixtures to exercise semantic rejection beyond byte mismatches.
fn reseal(root: &Path) -> String {
    let mut receipt: CampaignReceipt =
        artifact::read_json(&root.join("campaign.receipt.json")).unwrap();
    let mut previous = receipt.plan_sha256.clone();
    for (index, link) in receipt.stages.iter_mut().enumerate() {
        let stage = STAGES[index];
        change(root, &format!("{stage}/start.json"), |v| {
            v["previous_sha256"] = previous.clone().into()
        });
        link.start_sha256 = crate::fingerprint::file(&root.join(stage).join("start.json")).unwrap();
        change(root, &format!("{stage}/end.json"), |v| {
            v["start_sha256"] = link.start_sha256.clone().into()
        });
        link.end_sha256 = crate::fingerprint::file(&root.join(stage).join("end.json")).unwrap();
        previous = link.end_sha256.clone();
    }
    artifact::write_json(&root.join("campaign.receipt.json"), &receipt).unwrap();
    crate::fingerprint::file(&root.join("campaign.receipt.json")).unwrap()
}
#[test]
fn replay_binds_distinct_ab_pairs_and_is_read_only_without_performance_acceptance() {
    let (root, ppin, rpin) = fixture();
    let before = source_hash(&root.0).unwrap();
    let check = verify(&root.0, &ppin, &rpin).unwrap();
    assert!(check.accepted);
    assert_eq!((check.stages, check.observed), (4, 4));
    assert!(!check.performance_accepted);
    assert_eq!(source_hash(&root.0).unwrap(), before);
    assert!(verify(&root.0, &"0".repeat(64), &rpin).is_err());
    assert!(verify(&root.0, &ppin, &"0".repeat(64)).is_err());
}
#[test]
fn fresh_pins_do_not_make_renamed_reordered_or_cross_campaign_stages_valid() {
    for mutation in 0..3 {
        let (root, ppin, _) = fixture();
        change(&root.0, "B1/start.json", |v| match mutation {
            0 => v["stage"] = "B2".into(),
            1 => v["plan_sha256"] = "0".repeat(64).into(),
            _ => v["previous_sha256"] = "0".repeat(64).into(),
        });
        let rpin = if mutation == 2 {
            let changed = crate::fingerprint::file(&root.0.join("B1/start.json")).unwrap();
            change(&root.0, "campaign.receipt.json", |v| {
                v["stages"][1]["start_sha256"] = changed.into()
            });
            crate::fingerprint::file(&root.0.join("campaign.receipt.json")).unwrap()
        } else {
            reseal(&root.0)
        };
        assert!(verify(&root.0, &ppin, &rpin).is_err());
    }
    let (root, ppin, _) = fixture();
    change(&root.0, "campaign.receipt.json", |v| {
        v["stages"].as_array_mut().unwrap().swap(1, 2)
    });
    let rpin = crate::fingerprint::file(&root.0.join("campaign.receipt.json")).unwrap();
    assert!(verify(&root.0, &ppin, &rpin).is_err());
}
#[test]
fn overlap_gap_stage_and_total_deadlines_remain_refusals_with_self_consistent_pins() {
    for mutation in 0..5 {
        let (root, ppin, _) = fixture();
        match mutation {
            0 => change(&root.0, "B1/start.json", |v| v["started_ns"] = 1.into()),
            1 => change(&root.0, "B1/start.json", |v| {
                v["started_ns"] = (2 * SECOND).into()
            }),
            2 => change(&root.0, "A2/end.json", |v| {
                v["ended_ns"] = (3 * SECOND).into()
            }),
            3 => change(&root.0, "A2/end.json", |v| {
                v["ended_ns"] = (11 * SECOND).into()
            }),
            _ => change(&root.0, "campaign.receipt.json", |v| {
                v["ended_ns"] = 1.into()
            }),
        }
        let rpin = reseal(&root.0);
        assert!(!verify(&root.0, &ppin, &rpin).unwrap().accepted);
    }
}
#[test]
fn changed_pair_partial_failure_and_forged_derived_success_cannot_pass() {
    let (root, ppin, _) = fixture();
    let b_manifest = fs::read(root.0.join("B1/manifest.json")).unwrap();
    fs::write(root.0.join("A2/manifest.json"), b_manifest).unwrap();
    let rpin = reseal(&root.0);
    assert!(verify(&root.0, &ppin, &rpin).is_err());
    let (root, ppin, _) = fixture();
    change(&root.0, "campaign.receipt.json", |v| {
        v["stages"].as_array_mut().unwrap().truncate(1);
        v["failure"] = "cancelled".into();
    });
    fs::write(
        root.0.join("campaign.replay.json"),
        br#"{"accepted":true,"performance_accepted":true}"#,
    )
    .unwrap();
    let rpin = reseal(&root.0);
    assert!(!verify(&root.0, &ppin, &rpin).unwrap().accepted);
}
#[test]
fn changed_original_members_and_duplicate_json_fields_are_refused() {
    for member in [
        "campaign.json",
        "campaign.receipt.json",
        "A1/start.json",
        "A2/end.json",
        "B2/worker.stdout",
    ] {
        let (root, ppin, rpin) = fixture();
        fs::write(root.0.join(member), b"changed").unwrap();
        assert!(verify(&root.0, &ppin, &rpin).is_err(), "{member}");
    }
    let (root, ppin, _) = fixture();
    let receipt = fs::read_to_string(root.0.join("campaign.receipt.json")).unwrap();
    fs::write(
        root.0.join("campaign.receipt.json"),
        format!(
            "{{\"schema\":\"{SCHEMA}\",{}",
            receipt.trim_start_matches('{')
        ),
    )
    .unwrap();
    let rpin = crate::fingerprint::file(&root.0.join("campaign.receipt.json")).unwrap();
    assert!(verify(&root.0, &ppin, &rpin).is_err());
}
#[test]
fn pair_compatibility_and_limits_are_explicit() {
    let (a, apin) = frozen("a", "a");
    let (b, bpin) = frozen("b", "b");
    let a = load(&a.0, &apin).unwrap();
    let mut b = load(&b.0, &bpin).unwrap();
    assert!(compatible(&a.manifest, &b.manifest).is_ok());
    b.manifest.compiler.push_str(" changed");
    assert!(compatible(&a.manifest, &b.manifest).is_err());
    b.manifest.compiler = a.manifest.compiler.clone();
    b.manifest.catalog.push("reconstruct_plans/extra".into());
    assert!(compatible(&a.manifest, &b.manifest).is_err());
    for (stage, window, gap) in [
        (0, 10, 1),
        (1201, 4800, 1),
        (2, 1, 1),
        (1, 4801, 1),
        (1, 10, 0),
        (1, 10, 61),
    ] {
        let mut p = plan(apin.clone(), bpin.clone());
        p.stage_seconds = stage;
        p.window_seconds = window;
        p.gap_seconds = gap;
        assert!(p.validate().is_err());
    }
}
#[cfg(unix)]
#[test]
fn controller_runs_exact_abba_stops_on_failure_and_never_overwrites() {
    for failure_kind in ["success", "exit", "timeout"] {
        let failure = failure_kind != "success";
        let script = format!("#!/bin/sh\nprintf '%s' '{}'\n", logs());
        let (a, apin) = frozen("a", &script);
        let (b, bpin) = frozen(
            "b",
            match failure_kind {
                "exit" => "#!/bin/sh\nexit 7\n",
                "timeout" => "#!/bin/sh\nsleep 20\n",
                _ => &script,
            },
        );
        let root = Directory::new();
        let out = root.0.join("campaign");
        let args = Args(
            [
                ("--stage-seconds".into(), "1".into()),
                ("--baseline".into(), a.0.display().to_string()),
                ("--baseline-sha256".into(), apin),
                ("--candidate".into(), b.0.display().to_string()),
                ("--candidate-sha256".into(), bpin),
                ("--output".into(), out.display().to_string()),
            ]
            .into_iter()
            .collect(),
            Default::default(),
        );
        assert_eq!(capture(&args).unwrap(), !failure);
        let ppin = crate::fingerprint::file(&out.join("campaign.json")).unwrap();
        let rpin = crate::fingerprint::file(&out.join("campaign.receipt.json")).unwrap();
        let check = verify(&out, &ppin, &rpin).unwrap();
        assert_eq!(
            (check.stages, check.observed),
            if failure { (2, 1) } else { (4, 4) }
        );
        assert_eq!(out.join("B2/worker.stdout").exists(), !failure);
        if failure {
            let outcome: serde_json::Value =
                artifact::read_json(&out.join("B1/worker.outcome.json")).unwrap();
            assert_eq!(outcome["reaped"], true);
            assert_eq!(
                outcome["reason"],
                if failure_kind == "timeout" {
                    "timeout"
                } else {
                    "exited"
                }
            );
        }
        let before = source_hash(&out).unwrap();
        assert!(capture(&args).is_err());
        assert_eq!(source_hash(&out).unwrap(), before);
    }
}
