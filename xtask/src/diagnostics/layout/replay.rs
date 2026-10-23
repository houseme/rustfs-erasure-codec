//! Complete paired layouts: bind original members and reuse the shared grid model.
use super::verify::{self, PHASES};
use crate::{
    Result,
    artifact::{python_json::equivalent, snapshot::Snapshot},
    runtime, sampling_contract,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    directory: PathBuf,
    diagnostic: bool,
    contract: Value,
    baseline: Value,
    candidate: Value,
    tool: Value,
    #[serde(default, deserialize_with = "declared_digest")]
    manifest_sha256: Option<String>,
}
fn declared_digest<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn sha(value: &Value, len: usize) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn keys(value: &Value, names: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.len() == names.len() && names.iter().all(|k| v.contains_key(*k)))
}
fn same_tree(recorded: &Value, current: &Value) -> bool {
    keys(recorded, &["commit", "tree", "lock_sha256"])
        && sha(&recorded["commit"], 40)
        && ["tree", "lock_sha256"]
            .iter()
            .all(|k| recorded[k] == current[k])
}
fn context(value: &Value, cpu: &Value) -> Result<()> {
    if !value.is_object()
        || !equivalent(
            &value["settings"],
            &sampling_contract::protocol::sampling_environment(),
        )
        || ["compiler", "cpu_model"]
            .iter()
            .any(|k| !value[k].as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err("complete paired environment provenance is required".into());
    }
    runtime::validate_runtime(&value["runtime"])
        .map_err(|e| format!("complete paired runtime provenance is required: {e}"))?;
    let isolation = &value["isolation"];
    let cpus: Vec<u64> = serde_json::from_value(isolation["cpu_affinity"].clone())
        .map_err(|_| "complete paired exclusive CPU provenance is required")?;
    if isolation["scope"] != "exclusive-vm-cpu-partition"
        || isolation["physical_host_exclusivity_proven"].as_bool() != Some(false)
        || cpus.len() < 2
        || cpus.windows(2).any(|p| p[0] >= p[1])
        || isolation["reserved_cpu_count"].as_u64() != Some(cpus.len() as u64)
    {
        return Err("complete paired exclusive CPU provenance is required".into());
    }
    if cpu.as_u64() != cpus.first().copied() {
        return Err("paired measuring CPU differs from its reservation".into());
    }
    Ok(())
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        let mut snapshot = Snapshot::new(&self.directory)?;
        let document = snapshot.python_json_bound(
            if self.diagnostic {
                "diagnostic-manifest.json"
            } else {
                "manifest.json"
            },
            self.manifest_sha256.as_deref(),
        )?;
        if !document.is_object()
            || document
                .get("diagnostic_only")
                .unwrap_or(&Value::Bool(false))
                .as_bool()
                != Some(self.diagnostic)
        {
            return Err("layout diagnostic and acceptance modes must match".into());
        }
        if document["schema_version"] != "verify-layout-paired-v2"
            || document["phase_order"] != json!(PHASES)
            || !equivalent(&document["contract"], &self.contract)
            || document["release_approval"].as_bool() != Some(false)
        {
            return Err("a complete current paired-layout manifest is required".into());
        }
        let sources = &document["sources"];
        if !keys(sources, &["baseline", "candidate"]) {
            return Err("both paired source identities are required".into());
        }
        if sources["baseline"] != self.baseline {
            return Err("paired baseline must be the published 9.0.0".into());
        }
        if !same_tree(&sources["candidate"], &self.candidate)
            || sources["candidate"]["commit"] == self.baseline["commit"]
            || sources["candidate"]["tree"] == sources["baseline"]["tree"]
        {
            return Err("paired candidate does not match the current source tree/lock".into());
        }
        if !same_tree(&document["tool"], &self.tool) {
            return Err("paired tool source differs".into());
        }
        if !keys(&document["files"], &PHASES) {
            return Err("all paired phase members are required".into());
        }
        let (mut selected_context, mut cpu, mut iterations) =
            (Value::Null, Value::Null, Value::Null);
        let mut phases = BTreeMap::new();
        let contract = self
            .contract
            .as_object()
            .ok_or("invalid paired tool contract")?;
        for (index, phase) in PHASES.into_iter().enumerate() {
            let entries = &document["files"][phase];
            const MEMBERS: [&str; 3] = ["provenance.json", "samples.json", "measurement.log"];
            if !keys(entries, &MEMBERS) {
                return Err("each phase requires its provenance, raw samples and journal".into());
            }
            for name in MEMBERS {
                if !entries[name].is_object()
                    || entries[name]["path"] != format!("{phase}/{name}")
                    || !sha(&entries[name]["sha256"], 64)
                {
                    return Err("invalid paired member declaration".into());
                }
            }
            let path = |name: &str| format!("{phase}/{name}");
            let hash = |name: &str| entries[name]["sha256"].as_str();
            let metadata =
                snapshot.python_json_bound(&path("provenance.json"), hash("provenance.json"))?;
            let role = if phase.starts_with('A') {
                "baseline"
            } else {
                "candidate"
            };
            if !metadata.is_object()
                || metadata["schema_version"] != "verify-layout-diagnostic-v3"
                || metadata["source"] != sources[role]
                || metadata["tool"] != document["tool"]
                || contract
                    .iter()
                    .any(|(k, v)| !metadata.get(k).is_some_and(|a| equivalent(a, v)))
                || metadata["release_approval"].as_bool() != Some(false)
                || !metadata["candidate_attribution"].is_null()
            {
                return Err("paired phase source/tool/fixture contract differs".into());
            }
            context(&metadata["context"], &metadata["measuring_cpu"])?;
            if index > 0
                && (!equivalent(&metadata["context"], &selected_context)
                    || !equivalent(&metadata["measuring_cpu"], &cpu)
                    || metadata["iterations"].as_u64().is_none()
                    || !equivalent(&metadata["iterations"], &iterations))
            {
                return Err("paired phase context/CPU/iterations differ".into());
            }
            selected_context = metadata["context"].clone();
            cpu = metadata["measuring_cpu"].clone();
            iterations = metadata["iterations"].clone();
            let rows = snapshot.python_json_bound(&path("samples.json"), hash("samples.json"))?;
            let rows: Vec<Value> = serde_json::from_value(rows)?;
            let text = snapshot.text_bound(&path("measurement.log"), hash("measurement.log"))?;
            let mut count = 0usize;
            sampling_contract::output::visit(&text, &["RSE_LAYOUT_SAMPLE "], |_, actual| {
                if !rows
                    .get(count)
                    .is_some_and(|expected| equivalent(&actual, expected))
                {
                    return Err("paired samples differ from the completed numeric journal".into());
                }
                count += 1;
                Ok(())
            })?;
            if count != rows.len() {
                return Err("paired samples differ from the completed numeric journal".into());
            }
            phases.insert(phase.to_owned(), rows);
        }
        let mut result = verify::paired(
            &phases,
            iterations
                .as_u64()
                .ok_or("invalid paired iteration count")?,
            selected_context["runtime"]["page_size"]
                .as_u64()
                .ok_or("invalid paired page size")?,
            cpu.as_u64().ok_or("invalid paired measuring CPU")?,
            self.diagnostic,
        )?;
        snapshot.finish()?;
        let fields = result.as_object_mut().ok_or("invalid paired summary")?;
        let Value::Object(metadata) = json!({"sources":sources,"tool":document["tool"],"context":selected_context,"iterations":iterations,"measuring_cpu":cpu})
        else {
            return Err("invalid paired provenance".into());
        };
        fields.extend(metadata);
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
