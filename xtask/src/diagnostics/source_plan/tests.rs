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
            "9730404ed20b11583b7e75209b8498d44f481811ce5e128047a0cf3159dc354f",
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
