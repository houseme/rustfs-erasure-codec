use super::*;
use crate::artifact::snapshot::{digest, tests::Directory};
fn write(path: &Path, value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap();
    fs::write(path, &bytes).unwrap();
    digest(&bytes)
}
fn context() -> Value {
    json!({"compiler":"rustc fixture","cpu_model":"fixture CPU","settings":sampling_contract::protocol::sampling_environment(),"runtime":{"page_size":4096,"libc":{"name":"fixture","version":"1"},"allocator_environment":{},"kernel_memory_policy":{"kernel_release":"fixture","process_thp_disable":0,"thp":{"enabled":"madvise","defrag":"madvise","shmem_enabled":null,"sizes_kib":{}}}},"isolation":{"scope":"exclusive-vm-cpu-partition","reserved_cpu_count":8,"cpu_affinity":[0,1,2,3,4,5,6,7],"physical_host_exclusivity_proven":false}})
}
struct Fixture {
    directory: Directory,
    document: Value,
    observed: Observed,
}
impl Fixture {
    fn new(diagnostic: bool) -> Self {
        let (directory, mut request) = crate::sampling_contract::campaign::tests::fixture();
        let baseline =
            json!({"commit":BASELINE,"tree":"a".repeat(40),"lock_sha256":"a".repeat(64)});
        let candidate =
            json!({"commit":"b".repeat(40),"tree":"c".repeat(40),"lock_sha256":"b".repeat(64)});
        request["document"]["sources"] = json!({"baseline":baseline,"candidate":candidate});
        request["document"]["baseline_release"] = "9.0.0".into();
        request["document"]["schema_version"] = 4.into();
        request["document"]["phase_order"] = json!(crate::abba::STAGES);
        request["document"]["harness_sha256"] = "d".repeat(64).into();
        let resource = context();
        request["document"]["contexts"] = serde_json::to_value(
            crate::abba::STAGES
                .into_iter()
                .chain(["after"])
                .map(|p| (p, resource.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
        .unwrap();
        let mut expected = sampling_contract::protocol_value();
        expected["implementation_sha256"] = "e".repeat(64).into();
        expected["overlay_sha256"] = "f".repeat(64).into();
        let counts = request["document"]["sampling"]["common_iterations"].clone();
        request["document"]["sampling"] = expected.clone();
        request["document"]["sampling"]["common_iterations"] = counts;
        for phase in [
            "calibration-baseline",
            "calibration-candidate",
            "A1",
            "B1",
            "B2",
            "A2",
        ] {
            let revision = if phase == "calibration-baseline" || phase.starts_with('A') {
                BASELINE
            } else {
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            };
            for (profile, _) in FILES {
                let relative = format!("{phase}/{profile}-samples.json");
                let path = directory.0.join(&relative);
                let mut records: Vec<Value> =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                for record in &mut records {
                    record["revision"] = revision.into();
                    record["rows"][0]["git_revision"] = revision[..7].into();
                    record["rows"][0]["target_triple"] = "fixture-linux-gnu".into();
                }
                let sha = write(&path, &json!(records));
                if let Some(role) = phase.strip_prefix("calibration-") {
                    request["document"]["calibration"][role][profile]["sha256"] = sha.into();
                } else {
                    request["document"]["files"][phase][profile]["samples"]["sha256"] = sha.into();
                }
            }
        }
        crate::sampling_contract::campaign::tests::originals(&directory, &mut request);
        let paired = directory.0.join("layout-pair");
        fs::create_dir(&paired).unwrap();
        let contract = json!({"protocol":model("diagnostic-layout",json!({"operation":"protocol"})).unwrap(),"template_sha256":"1".repeat(64),"controller_sha256":"2".repeat(64),"sampling_implementation_sha256":"3".repeat(64)});
        let sources = request["document"]["sources"].clone();
        let mut layout = json!({"schema_version":"verify-layout-paired-v2","phase_order":crate::abba::STAGES,"sources":sources,"tool":candidate,"contract":contract,"release_approval":false,"files":{}});
        if diagnostic {
            layout["diagnostic_only"] = true.into();
        }
        let mut rows = crate::diagnostics::layout::tests::grid(Some(4096));
        for row in &mut rows {
            row["iterations"] = 1.into();
            row["ns_per_iter"] = 2_000_000_000_f64.into();
            row["cpu"] = 0.into();
            row["backend_id"] = "scalar".into();
            row["backend_kind"] = "scalar".into();
        }
        for phase in crate::abba::STAGES {
            fs::create_dir(paired.join(phase)).unwrap();
            let mut metadata = json!({"schema_version":"verify-layout-diagnostic-v3","source":sources[if phase.starts_with('A'){"baseline"}else{"candidate"}],"tool":candidate,"context":resource,"iterations":1,"measuring_cpu":0,"release_approval":false,"candidate_attribution":null});
            metadata
                .as_object_mut()
                .unwrap()
                .extend(contract.as_object().unwrap().clone());
            let metadata_sha = write(&paired.join(phase).join("provenance.json"), &metadata);
            let samples_sha = write(&paired.join(phase).join("samples.json"), &json!(rows));
            let log = rows
                .iter()
                .map(|r| format!("RSE_LAYOUT_SAMPLE {r}\n"))
                .collect::<String>();
            fs::write(paired.join(phase).join("measurement.log"), &log).unwrap();
            for (name, sha) in [
                ("provenance.json", metadata_sha),
                ("samples.json", samples_sha),
                ("measurement.log", digest(log.as_bytes())),
            ] {
                layout["files"][phase][name] =
                    json!({"path":format!("{phase}/{name}"),"sha256":sha});
            }
        }
        let filename = if diagnostic {
            "diagnostic-manifest.json"
        } else {
            "manifest.json"
        };
        let sha = write(&paired.join(filename), &layout);
        request["document"]["layout_supplement"] =
            json!({"path":format!("layout-pair/{filename}"),"sha256":sha});
        if diagnostic {
            request["document"]["schema_version"] = DIAGNOSTIC.into();
            request["document"]["diagnostic_only"] = true.into();
            request["document"]["release_approval"] = false.into();
        }
        let observed = Observed {
            baseline,
            candidate: candidate.clone(),
            tool: candidate.clone(),
            compiler: Some("rustc fixture".into()),
            harness: "d".repeat(64),
            sampling: expected,
            layout: json!({"baseline":sources["baseline"],"candidate":candidate,"tool":candidate,"contract":contract}),
        };
        Self {
            directory,
            document: request["document"].take(),
            observed,
        }
    }
    fn manifest(&self) -> PathBuf {
        self.directory
            .0
            .join(if self.document["schema_version"] == DIAGNOSTIC {
                "diagnostic-manifest.json"
            } else {
                "manifest.json"
            })
    }
    fn replay(&self, diagnostic: bool) -> Result<Value> {
        let path = self.manifest();
        write(&path, &self.document);
        verify_observed(&path, diagnostic, || Ok(self.observed.clone())).map(|(_, report)| report)
    }
    fn raw(&mut self, phase: &str, profile: &str, change: impl FnOnce(&mut Vec<Value>)) {
        let entry = &self.document["files"][phase][profile]["samples"];
        let path = self.directory.0.join(entry["path"].as_str().unwrap());
        let mut rows: Vec<Value> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut rows);
        self.document["files"][phase][profile]["samples"]["sha256"] =
            write(&path, &json!(rows)).into();
        let selected = crate::control::sampling::select(
            rows,
            profile,
            self.document["sources"][if phase.starts_with('A') {
                "baseline"
            } else {
                "candidate"
            }]["commit"]
                .as_str()
                .unwrap(),
            &self.document["sampling"]["common_iterations"][profile],
            &self.document["contexts"]["A1"]["isolation"]["cpu_affinity"],
            &json!(4096),
            true,
        )
        .unwrap();
        let path = self.directory.0.join(
            self.document["files"][phase][profile]["path"]
                .as_str()
                .unwrap(),
        );
        self.document["files"][phase][profile]["sha256"] =
            write(&path, &selected["selected"]).into();
    }
}
#[test]
fn complete_formal_and_diagnostic_controllers_preserve_original_members_and_approval_boundaries() {
    for diagnostic in [false, true] {
        let fixture = Fixture::new(diagnostic);
        let before = fs::read(fixture.directory.0.join("A1/small_file-samples.json")).unwrap();
        let report = fixture.replay(diagnostic).unwrap();
        if diagnostic {
            assert_eq!(report["status"], "diagnostic_complete");
            assert_eq!(report["release_approval"], false);
            assert!(report["candidate_attribution"].is_null());
            assert_eq!(report["worker_invocations"], 756);
            assert_eq!(report["regression_checks"].as_array().unwrap().len(), 16);
        } else {
            assert_eq!(report["status"], "passed");
            assert_eq!(
                report["case_counts"],
                json!({"smoke":15,"small_file":88,"hotspot":8,"stream":15})
            );
        }
        assert_eq!(
            fs::read(fixture.directory.0.join("A1/small_file-samples.json")).unwrap(),
            before
        );
    }
}
#[test]
fn source_flags_hashes_and_protocol_mismatches_are_refused() {
    let mut fixture = Fixture::new(false);
    let original = fixture.document.clone();
    for (pointer, value) in [
        ("/baseline_release", json!("8.0.0")),
        ("/sources/baseline/tree", json!("f".repeat(40))),
        ("/sources/candidate/lock_sha256", json!("f".repeat(64))),
        ("/harness_sha256", json!("f".repeat(64))),
        ("/contexts/after/cpu_model", json!("changed")),
        (
            "/contexts/A1/isolation/physical_host_exclusivity_proven",
            json!(0),
        ),
        ("/sampling/version", json!(14)),
        ("/files/A1/smoke/sha256", json!("0".repeat(64))),
        ("/calibration/baseline/smoke/path", json!("../outside.json")),
        ("/layout_supplement/sha256", json!("0".repeat(64))),
    ] {
        fixture.document = original.clone();
        *fixture.document.pointer_mut(pointer).unwrap() = value;
        assert!(fixture.replay(false).is_err(), "{pointer}");
    }
}
#[test]
fn copied_context_numeric_equality_and_same_tree_rebase_remain_supported() {
    let mut fixture = Fixture::new(false);
    fixture.document["contexts"]["B1"]["isolation"]["physical_host_exclusivity_proven"] = 0.into();
    fixture.observed.candidate["commit"] = "e".repeat(40).into();
    fixture.observed.layout["candidate"]["commit"] = "e".repeat(40).into();
    let report = fixture.replay(false).unwrap();
    assert_eq!(report["candidate_commit"], "e".repeat(40));
    assert_eq!(report["measured_candidate_commit"], "b".repeat(40));
}
#[test]
fn diagnostic_schema_is_never_promoted_or_materialized_as_formal_evidence() {
    let fixture = Fixture::new(true);
    assert!(fixture.replay(false).is_err());
    assert!(
        !fixture
            .directory
            .0
            .join("target/release-preflight/inputs")
            .exists()
    );
    let mut fixture = Fixture::new(false);
    fixture.document["schema_version"] = 3.into();
    assert!(
        fixture
            .replay(false)
            .unwrap_err()
            .to_string()
            .contains("schema 4")
    );
}
#[test]
fn source_changes_during_replay_prevent_a_successful_return() {
    let fixture = Fixture::new(false);
    let path = fixture.manifest();
    write(&path, &fixture.document);
    let mut observations = 0;
    let result = verify_observed(&path, false, || {
        observations += 1;
        let mut live = fixture.observed.clone();
        if observations > 1 {
            live.candidate["tree"] = "f".repeat(40).into();
        }
        Ok(live)
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("changed during verification")
    );
    assert_eq!(observations, 2);
}
#[test]
fn short_wide_and_drift_failures_are_collected_across_the_complete_diagnostic() {
    let mut fixture = Fixture::new(true);
    fixture.raw("A1", "small_file", |rows| {
        for r in rows {
            if r["case"] == "encode:4:2:1024" && r["index"].as_u64().unwrap() >= 9 {
                r["rows"][0]["ns_per_iter"] =
                    json!(r["rows"][0]["ns_per_iter"].as_f64().unwrap() * 2.0);
                r["timed_ns"] = json!(r["timed_ns"].as_f64().unwrap() * 2.0);
                r["wall_ns"] = json!(r["wall_ns"].as_f64().unwrap() * 2.0);
            }
        }
    });
    fixture.raw("B1", "stream", |rows| {
        let row = rows.last_mut().unwrap();
        row["wall_ns"] = 500_000_000.into();
        row["complete"] = false.into();
        row["target_met"] = false.into();
    });
    let report = fixture.replay(true).unwrap();
    assert_eq!(report["regression_checks"].as_array().unwrap().len(), 16);
    assert!(report["all_checks_executed"].as_bool().unwrap());
    assert_eq!(report["all_original_limits_passed"], false);
    assert!(
        !report["distribution_failures"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!report["short_samples"].as_array().unwrap().is_empty());
}
#[test]
fn missing_layout_is_refused_before_source_observation() {
    let fixture = Fixture::new(false);
    let path = fixture.manifest();
    let mut doc = fixture.document;
    doc.as_object_mut().unwrap().remove("layout_supplement");
    write(&path, &doc);
    let result = verify_observed(&path, false, || {
        panic!("must not observe/build before refusing the missing supplement")
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("paired-layout supplement")
    );
}

#[test]
fn changed_originals_after_verification_cannot_be_exported_with_a_passing_report() {
    let fixture = Fixture::new(false);
    let report = fixture.replay(false).unwrap();
    let repository = Directory::new();
    let env_file = repository.0.join("release.env");
    let original = fixture.directory.0.join(
        fixture.document["files"]["B2"]["stream"]["path"]
            .as_str()
            .unwrap(),
    );
    fs::write(original, b"changed after verification").unwrap();
    assert!(
        materialize_verified(
            &fixture.manifest(),
            &repository.0,
            &env_file,
            &fixture.document,
            &report
        )
        .is_err()
    );
    assert!(!env_file.exists());
    assert!(
        !repository
            .0
            .join("target/release-preflight/inputs")
            .exists()
    );
}

#[test]
fn materialized_inputs_preserve_all_verified_bytes_and_release_variable_paths() {
    let fixture = Fixture::new(false);
    let report = fixture.replay(false).unwrap();
    let repository = Directory::new();
    let env_file = repository.0.join("release.env");
    materialize_verified(
        &fixture.manifest(),
        &repository.0,
        &env_file,
        &fixture.document,
        &report,
    )
    .unwrap();
    let mut expected_env = String::new();
    for (index, (profile, name)) in FILES.into_iter().enumerate() {
        let baseline = repository
            .0
            .join("target/release-preflight/inputs")
            .join(name);
        let candidate = repository.0.join("target/benchmark-smoke").join(name);
        for (phase, actual) in [("A1", &baseline), ("B2", &candidate)] {
            let original = fixture.directory.0.join(
                fixture.document["files"][phase][profile]["path"]
                    .as_str()
                    .unwrap(),
            );
            assert_eq!(fs::read(actual).unwrap(), fs::read(original).unwrap());
        }
        expected_env.push_str(&format!("{}={}\n", VARIABLES[index], baseline.display()));
    }
    assert_eq!(fs::read_to_string(env_file).unwrap(), expected_env);
    assert_eq!(
        io::json(
            &repository
                .0
                .join("target/release-preflight/evidence-validation.json"),
            None
        )
        .unwrap(),
        report
    );
}
