//! Validate recorded campaign metadata against fresh caller-owned Git/compiler observations.
use crate::{
    Result, abba::STAGES, artifact::python_json::equivalent,
    diagnostics::baseline::replay::BASELINE, runtime, sampling_contract::protocol,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Bindings {
    baseline: Value,
    candidate: Value,
    harness_sha256: String,
    compiler: Option<String>,
}
fn hex(value: &Value, len: usize) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
pub(crate) fn validate_context(reference: &Value) -> Result<()> {
    let isolation = &reference["isolation"];
    if !reference.is_object()
        || !isolation.is_object()
        || isolation["scope"] != "exclusive-vm-cpu-partition"
    {
        return Err("controlled resource provenance is required".into());
    }
    if !equivalent(&reference["settings"], &protocol::sampling_environment())
        || !truthy(&reference["cpu_model"])
        || !truthy(&reference["compiler"])
        || !isolation["reserved_cpu_count"]
            .as_u64()
            .is_some_and(|n| n >= 2)
        || isolation["physical_host_exclusivity_proven"] != false
    {
        return Err("incomplete or unexpected controlled capture settings".into());
    }
    runtime::validate_runtime(&reference["runtime"])?;
    Ok(())
}

impl Bindings {
    pub(super) fn validate(&self, document: &Value) -> Result<()> {
        if document["baseline_release"] != "9.0.0" {
            return Err("release baseline must be the published 9.0.0".into());
        }
        let sources = &document["sources"];
        for role in ["baseline", "candidate"] {
            let source = &sources[role];
            if !source.is_object() || !["commit", "tree"].iter().all(|key| hex(&source[key], 40)) {
                return Err("sources must bind full commit and tree identifiers".into());
            }
            if !hex(&source["lock_sha256"], 64) {
                return Err("sources must bind lockfile digests".into());
            }
        }
        let baseline = &sources["baseline"];
        let candidate = &sources["candidate"];
        if baseline["commit"] != BASELINE {
            return Err("baseline commit is not published 9.0.0".into());
        }
        if !equivalent(baseline, &self.baseline) {
            return Err("baseline source metadata differs from published 9.0.0".into());
        }
        if baseline["commit"] == candidate["commit"] || baseline["tree"] == candidate["tree"] {
            return Err("candidate/self comparison is not release evidence".into());
        }
        if ["tree", "lock_sha256"]
            .iter()
            .any(|key| candidate[key] != self.candidate[key])
        {
            return Err("evidence does not match the current candidate tree/lock".into());
        }
        if document["harness_sha256"] != self.harness_sha256 {
            return Err("benchmark harness differs from measured evidence".into());
        }
        let contexts = &document["contexts"];
        let reference = &contexts["A1"];
        validate_context(reference)?;
        if STAGES
            .into_iter()
            .chain(["after"])
            .any(|phase| !equivalent(&contexts[phase], reference))
        {
            return Err("measurement environment changed during the campaign".into());
        }
        if self
            .compiler
            .as_ref()
            .is_some_and(|compiler| reference["compiler"] != compiler.as_str())
        {
            return Err("release compiler differs from measured evidence".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (Bindings, Value) {
        let baseline =
            json!({"commit":BASELINE,"tree":"a".repeat(40),"lock_sha256":"a".repeat(64)});
        let candidate =
            json!({"commit":"b".repeat(40),"tree":"c".repeat(40),"lock_sha256":"b".repeat(64)});
        let context = json!({"compiler":"rustc fixture","cpu_model":"fixture CPU","settings":protocol::sampling_environment(),
            "runtime":{"page_size":4096,"libc":{"name":"fixture","version":"1"},"allocator_environment":{},"kernel_memory_policy":{"kernel_release":"fixture-kernel","process_thp_disable":0,"thp":{"enabled":"madvise","defrag":"madvise","shmem_enabled":null,"sizes_kib":{"16":{"anonymous":"never","shmem":null},"2048":{"anonymous":"inherit","shmem":null}}}}},
            "isolation":{"scope":"exclusive-vm-cpu-partition","reserved_cpu_count":8,"cpu_affinity":[0,1,2,3,4,5,6,7],"physical_host_exclusivity_proven":false}});
        let document = json!({"baseline_release":"9.0.0","sources":{"baseline":baseline,"candidate":candidate},"harness_sha256":"d".repeat(64),"contexts":STAGES.into_iter().chain(["after"]).map(|phase|(phase,context.clone())).collect::<std::collections::BTreeMap<_,_>>()});
        let bindings=serde_json::from_value(json!({"baseline":baseline,"candidate":candidate,"harness_sha256":"d".repeat(64),"compiler":"rustc fixture"})).unwrap();
        (bindings, document)
    }
    #[test]
    fn fresh_git_and_compiler_bindings_keep_same_tree_rebase_and_recorded_inputs() {
        let (mut bindings, document) = fixture();
        let before = document.clone();
        bindings.candidate["commit"] = json!("e".repeat(40));
        bindings.validate(&document).unwrap();
        assert_eq!(document, before);
        bindings.compiler = Some("different compiler".into());
        assert!(
            bindings
                .validate(&document)
                .unwrap_err()
                .to_string()
                .contains("compiler")
        );
        bindings.compiler = None;
        bindings.validate(&document).unwrap();
        bindings.candidate["tree"] = json!("f".repeat(40));
        assert!(
            bindings
                .validate(&document)
                .unwrap_err()
                .to_string()
                .contains("tree/lock")
        );
    }
    #[test]
    fn original_source_harness_environment_and_runtime_refusals_are_retained() {
        let (bindings, document) = fixture();
        let changes = [
            ("baseline_release", json!("8.0.0")),
            ("harness_sha256", json!("e".repeat(64))),
        ];
        for (key, value) in changes {
            let mut wrong = document.clone();
            wrong[key] = value;
            assert!(bindings.validate(&wrong).is_err());
        }
        for key in ["commit", "tree", "lock_sha256"] {
            let mut wrong = document.clone();
            wrong["sources"]["baseline"][key] = json!(true);
            assert!(bindings.validate(&wrong).is_err());
        }
        for (key, value) in [
            ("reserved_cpu_count", json!(true)),
            ("physical_host_exclusivity_proven", json!(0)),
        ] {
            let mut wrong = document.clone();
            wrong["contexts"]["A1"]["isolation"][key] = value;
            assert!(bindings.validate(&wrong).is_err());
        }
        let mut wrong = document.clone();
        wrong["contexts"]["after"]["cpu_model"] = json!("changed");
        assert!(
            bindings
                .validate(&wrong)
                .unwrap_err()
                .to_string()
                .contains("environment changed")
        );
        let mut wrong = document;
        wrong["contexts"]["A1"]["runtime"]["page_size"] = json!(true);
        assert!(bindings.validate(&wrong).is_err());
    }
}
