use super::*;
use crate::artifact::snapshot::{digest, tests::Directory};
use std::fs;

fn context_value() -> Value {
    json!({"compiler":"rustc fixture","cpu_model":"fixture CPU","settings":sampling_contract::protocol::sampling_environment(),
        "runtime":{"page_size":4096,"libc":{"name":"glibc","version":"fixture"},"allocator_environment":{},
            "kernel_memory_policy":{"kernel_release":"fixture","process_thp_disable":0,"thp":{"enabled":"madvise","defrag":"madvise","shmem_enabled":null,"sizes_kib":{}}}},
        "isolation":{"scope":"exclusive-vm-cpu-partition","cpu_affinity":[2,3],"reserved_cpu_count":2,"physical_host_exclusivity_proven":false}})
}
fn campaign() -> (Directory, Value) {
    let directory = Directory::new();
    let baseline =
        json!({"commit":"a".repeat(40),"tree":"a".repeat(40),"lock_sha256":"a".repeat(64)});
    let candidate =
        json!({"commit":"b".repeat(40),"tree":"b".repeat(40),"lock_sha256":"b".repeat(64)});
    let contract = json!({"protocol":verify::protocol(),"template_sha256":"c".repeat(64),"controller_sha256":"d".repeat(64),"sampling_implementation_sha256":"e".repeat(64)});
    let mut document = json!({"schema_version":"verify-layout-paired-v2","phase_order":PHASES,"sources":{"baseline":baseline,"candidate":candidate},"tool":candidate,"contract":contract,"release_approval":false,"files":{}});
    let rows = super::super::tests::grid(Some(4096));
    for phase in PHASES {
        let root = directory.0.join(phase);
        fs::create_dir(&root).unwrap();
        let mut metadata = json!({"schema_version":"verify-layout-diagnostic-v3","source":if phase.starts_with('A') {&baseline} else {&candidate},"tool":candidate,
            "context":context_value(),"iterations":1000000,"measuring_cpu":2,"release_approval":false,"candidate_attribution":null});
        metadata
            .as_object_mut()
            .unwrap()
            .extend(contract.as_object().unwrap().clone());
        let values = [
            ("provenance.json", metadata.to_string()),
            ("samples.json", serde_json::to_string(&rows).unwrap()),
            (
                "measurement.log",
                rows.iter()
                    .map(|row| format!("RSE_LAYOUT_SAMPLE {row}\n"))
                    .collect(),
            ),
        ];
        for (name, text) in values {
            fs::write(root.join(name), &text).unwrap();
            document["files"][phase][name] =
                json!({"path":format!("{phase}/{name}"),"sha256":digest(text.as_bytes())});
        }
    }
    fs::write(directory.0.join("manifest.json"), document.to_string()).unwrap();
    let request = json!({"directory":directory.0,"diagnostic":false,"contract":contract,"baseline":baseline,"candidate":candidate,"tool":candidate});
    (directory, request)
}
fn execute(request: Value) -> Result<Value> {
    serde_json::from_value::<Request>(request)?.execute()
}
#[test]
fn complete_directory_replays_without_mutating_originals_or_approving_release() {
    let (directory, request) = campaign();
    let before = fs::read(directory.0.join("manifest.json")).unwrap();
    let report = execute(request).unwrap();
    assert_eq!(report["layout_gate_passed"], true);
    assert_eq!(report["release_approval"], false);
    assert_eq!(report["phase_reports"].as_object().unwrap().len(), 4);
    assert_eq!(fs::read(directory.0.join("manifest.json")).unwrap(), before);
}
#[test]
fn supplied_manifest_digest_and_current_tool_tree_are_always_bound() {
    let (directory, mut request) = campaign();
    let hash = digest(&fs::read(directory.0.join("manifest.json")).unwrap());
    request["manifest_sha256"] = json!(hash);
    execute(request.clone()).unwrap();
    request["manifest_sha256"] = Value::Null;
    assert!(execute(request.clone()).is_err());
    request["manifest_sha256"] = json!("0".repeat(64));
    assert!(execute(request.clone()).is_err());
    request.as_object_mut().unwrap().remove("manifest_sha256");
    request["candidate"]["commit"] = json!("e".repeat(40));
    request["tool"]["commit"] = json!("e".repeat(40));
    execute(request.clone()).unwrap();
    request["tool"]["tree"] = json!("f".repeat(40));
    assert!(
        execute(request)
            .unwrap_err()
            .to_string()
            .contains("tool source")
    );
}
#[test]
fn context_checks_remain_strict_and_keep_the_original_first_phase_iteration_rule() {
    let value = context_value();
    context(&value, &json!(2)).unwrap();
    for key in ["reserved_cpu_count", "physical_host_exclusivity_proven"] {
        let mut wrong = value.clone();
        wrong["isolation"][key] = json!(2.0);
        assert!(context(&wrong, &json!(2)).is_err());
    }
    assert!(context(&value, &json!(2.0)).is_err());
    let (directory, request) = campaign();
    let path = directory.0.join("A1/provenance.json");
    let mut metadata: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    metadata["iterations"] = json!(1000000.0);
    fs::write(&path, metadata.to_string()).unwrap();
    let manifest = directory.0.join("manifest.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    document["files"]["A1"]["provenance.json"]["sha256"] = json!(digest(&fs::read(path).unwrap()));
    fs::write(manifest, document.to_string()).unwrap();
    assert_eq!(execute(request).unwrap()["iterations"], 1000000);
}
