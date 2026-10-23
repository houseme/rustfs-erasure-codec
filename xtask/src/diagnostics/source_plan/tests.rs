use super::*;
use std::path::Path;

fn original(root: &Path, variant: &str) -> BTreeMap<String, String> {
    let names = match variant {
        "verify-scratch" => vec!["src/lib.rs", "src/core/verify.rs"],
        "stream-core" => vec!["src/lib.rs", "src/core/stream.rs", "src/core/verify.rs"],
        _ => [
            "src/lib.rs",
            "src/matrix.rs",
            "src/core/reconstruct.rs",
            "src/galois_8/policy.rs",
        ]
        .into_iter()
        .chain(super::super::leopard::FILES)
        .collect(),
    };
    names
        .into_iter()
        .map(|n| (n.into(), super::super::source::read(root.join(n)).unwrap()))
        .collect()
}

#[test]
fn complete_plans_keep_original_generated_bytes_and_bind_every_written_member() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for (variant, expected) in [
        (
            "verify-scratch",
            "6f857299445f92d9c09fe2b3802746f75036620c08b970f68c53d5bdfb8fd4a5",
        ),
        (
            "stream-core",
            "3f3d3ce8443cf418f327a0b8385b41063a726ff52ae9a76dff93ca9aaec330e8",
        ),
        (
            "dynamic",
            "c029b229a21829425aca8786f2341994e7154cd37d98426ed57cf76ab4095e35",
        ),
    ] {
        let original = original(root, variant);
        let request = json!({"variant":variant,"original":original,"tool":root});
        let plan = serde_json::from_value::<Request>(request.clone())
            .unwrap()
            .execute()
            .unwrap();
        let files: BTreeMap<String, String> =
            serde_json::from_value(plan["files"].clone()).unwrap();
        let bytes: Vec<_> = files
            .iter()
            .flat_map(|(n, s)| n.bytes().chain([0]).chain(s.bytes()))
            .collect();
        assert_eq!(digest(&bytes), expected, "{variant}");
        assert_eq!(plan["bindings"].as_object().unwrap().len(), files.len());
        for (name, text) in &files {
            assert_eq!(
                plan["bindings"][name]["diagnostic_sha256"],
                digest(text.as_bytes())
            );
            assert_eq!(
                plan["bindings"][name]["original_sha256"],
                json!(original.get(name).map(|s| digest(s.as_bytes())))
            );
        }
        for output in ["files", "bindings"] {
            let mut request = request.clone();
            request["output"] = json!(output);
            assert_eq!(
                serde_json::from_value::<Request>(request)
                    .unwrap()
                    .execute()
                    .unwrap(),
                plan[output]
            );
        }
    }
}

#[test]
fn missing_inputs_changed_anchors_and_unknown_modes_refuse_before_mutation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut request =
        json!({"variant":"verify-scratch","original":original(root,"verify-scratch"),"tool":root});
    request["original"]["src/core/verify.rs"] = json!("changed source");
    assert!(
        serde_json::from_value::<Request>(request.clone())
            .unwrap()
            .execute()
            .is_err()
    );
    request["original"]
        .as_object_mut()
        .unwrap()
        .remove("src/lib.rs");
    assert!(
        serde_json::from_value::<Request>(request.clone())
            .unwrap()
            .execute()
            .is_err()
    );
    request["output"] = json!("unknown");
    assert!(serde_json::from_value::<Request>(request.clone()).is_err());
    request.as_object_mut().unwrap().remove("output");
    request["variant"] = json!("unknown");
    assert!(serde_json::from_value::<Request>(request).is_err());
}

#[test]
fn sampler_adapters_preserve_operation_statements_and_reject_unknown_layouts() {
    use super::super::{sampler, source};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for (file, name, count) in [
        ("benchmark_small_files.rs", "fn run_operation", 11),
        ("benchmark_stream_paths.rs", "fn run_stream_operation", 3),
    ] {
        let original = source::read(root.join("tests").join(file)).unwrap();
        let adapted = sampler::exclude_setup(&original, name, count).unwrap();
        assert_eq!(
            adapted.matches("measured += start.elapsed();").count(),
            count
        );
        assert!(sampler::exclude_setup(&original, name, count + 1).is_err());
    }
    let original = source::read(root.join("tests/benchmark_stream_paths.rs")).unwrap();
    let adapted = sampler::reserve_outputs(&original).unwrap();
    assert_ne!(adapted, original);
    assert_eq!(
        adapted.replace(
            "let mut writers = reserved_stream_outputs(case.parity_shards, case.shard_size);",
            "let mut writers = vec![Vec::with_capacity(case.shard_size); case.parity_shards];"
        ),
        original
    );
    assert!(sampler::reserve_outputs(&adapted).is_err());
}

#[test]
fn observer_source_adapters_leave_other_apis_initialization_and_publication_unchanged() {
    use super::super::source;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let original = source::read(root.join("src/core/verify.rs")).unwrap();
    let changed = stream::verify(&original).unwrap();
    let marker = "    /// Verify using a pre-allocated";
    assert_eq!(
        original.split_once(marker).unwrap().1,
        changed.split_once(marker).unwrap().1
    );
    assert!(changed.contains("rse_probe.verified(valid)"));
    assert!(changed.contains("Guard::new(scratch_len)"));
    assert!(
        stream::verify(
            &original.replace("scratch.resize(scratch_len, F::zero());", "unexpected();")
        )
        .is_err()
    );
    let original = source::read(root.join("src/core/stream.rs")).unwrap();
    let changed = stream::patch(&original).unwrap();
    assert!(!original.contains("rse_probe"));
    assert!(changed.contains("rse_probe") && changed.contains("StreamReconstructPlan::new"));
    assert!(stream::patch(&original.replace("pub fn encode_stream(", "pub fn renamed(")).is_err());
    assert!(stream::patch(&changed).is_err());
    let original = source::read(root.join("src/matrix.rs")).unwrap();
    assert!(
        dynamic::matrix(
            &original.replace("let mut work = self.augment", "let mut work = self.other")
        )
        .is_err()
    );
    assert_eq!(original, source::read(root.join("src/matrix.rs")).unwrap());
    let original = source::read(root.join("src/core/reconstruct.rs")).unwrap();
    let changed = dynamic::decode(&original).unwrap();
    for marker in ["destination.copy_from_slice(&bytes);", "get_or_initialize("] {
        assert_eq!(
            original.matches(marker).count(),
            changed.matches(marker).count()
        );
    }
    assert!(changed.contains("Region::new(\"decode\")"));
    assert!(!original.contains(dynamic::MODULE));
}

#[test]
fn native_observers_share_the_existing_bounded_procfs_helpers() {
    use super::super::source;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let engine = source::read(root.join("scripts/release-sampling/engine.rs")).unwrap();
    let native = templates::native("verify", root).unwrap();
    for name in ["fn read_bounded", "fn process_faults"] {
        assert!(native.contains(source::block(&engine, name).unwrap()));
    }
    let faults = source::read(root.join("scripts/diagnostics/stream_core.rs")).unwrap();
    let native = templates::native("dynamic", root).unwrap();
    assert!(native.contains(source::block(&faults, "fn faults()").unwrap()));
    assert!(!native.contains("process_faults()"));
    assert!(native.contains("process_faults(read_bounded("));
}

#[test]
fn generated_workers_keep_setup_and_owned_metadata_outside_measured_work() {
    use super::super::{sampler, source};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let rendered = sampler::render(root, root).unwrap();
    for profile in ["small_file", "stream"] {
        let kernel = source::block(
            &rendered[&format!("{profile}.rs")],
            "fn execute_release_kernel",
        )
        .unwrap();
        for setup in [
            "ReedSolomon::new",
            "make_full_shards",
            "make_data_shards",
            "make_encoded_shards",
        ] {
            assert!(!kernel.contains(setup));
        }
        assert!(kernel.contains(".cloned()"));
    }
    assert!(rendered["small_file.rs"].contains("Vec<Option<Vec<u8>>>"));
    assert!(
        rendered["stream.rs"]
            .contains("reserved_stream_outputs(case.parity_shards, case.shard_size)")
    );
    let result = source::block(
        &rendered["hotspot.rs"],
        "struct ReconstructionHotspotBenchResult",
    )
    .unwrap();
    assert!(!result.contains(": String,"));
    assert_eq!(result.matches("super::pattern::ShardPattern").count(), 2);
    let pattern = source::block(&rendered["hotspot.rs"], "fn shard_index_pattern").unwrap();
    assert!(!pattern.contains("format!") && !pattern.contains("Vec::"));
    assert!(rendered["root.rs"].contains("mod pattern;"));
    assert_eq!(
        crate::sampling_contract::protocol_value()["result_metadata_policy"],
        "bounded-inline-hotspot-patterns-deferred-formatting"
    );
}
