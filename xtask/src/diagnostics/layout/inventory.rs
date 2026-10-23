//! Structural inventories share the formal worker/provenance and typed view models.
use super::super::views;
use crate::{Result, benchmark_contract, release_rows, sampling_contract};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const PHASES: [&str; 4] = ["A1", "B1", "B2", "A2"];
const PAIRS: [(&str, &str); 6] = [
    ("A1", "A2"),
    ("B1", "B2"),
    ("A1", "B1"),
    ("A1", "B2"),
    ("A2", "B1"),
    ("A2", "B2"),
];
#[derive(Deserialize)]
pub(crate) struct Source {
    commit: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Sources {
    baseline: Source,
    candidate: Source,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Contract {
    row: Value,
    placement: Value,
    workload: Value,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Retained {
        phases: BTreeMap<String, Vec<Value>>,
        sources: Sources,
        cpus: Option<Vec<u64>>,
        page_size: Option<u64>,
    },
    StreamPhase {
        records: Vec<Value>,
        revision: String,
        iterations: u64,
        cpus: Option<Vec<u64>>,
        page_size: u64,
        reference: Option<BTreeMap<String, Contract>>,
    },
    StreamReport {
        phases: BTreeMap<String, Vec<views::Inventory>>,
    },
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::Retained {
                phases,
                sources,
                cpus,
                page_size,
            } => retained(&phases, &sources, cpus.as_deref(), page_size),
            Self::StreamPhase {
                mut records,
                revision,
                iterations,
                cpus,
                page_size,
                reference,
            } => {
                views::underlying_rows(&mut records)?;
                let rows: Vec<_> = records.iter().collect();
                let contract = profile(
                    &rows,
                    "stream",
                    &revision,
                    iterations,
                    cpus.as_deref(),
                    Some(page_size),
                    reference.as_ref(),
                )?;
                Ok(serde_json::to_value(contract)?)
            }
            Self::StreamReport { phases } => stream_report(&phases),
        }
    }
}
fn phases<T>(values: &BTreeMap<String, T>) -> Result<()> {
    if values.len() != PHASES.len() || PHASES.iter().any(|p| !values.contains_key(*p)) {
        return Err("all four layout inventory phases are required".into());
    }
    Ok(())
}
fn cases(profile: &str) -> BTreeSet<String> {
    sampling_contract::case_catalog()[profile]
        .iter()
        .cloned()
        .collect()
}
fn workload(record: &Value) -> Value {
    let mut value = record["workload"].clone();
    if let Some(fields) = value.as_object_mut() {
        fields.remove("rng");
        fields.insert(
            "initial_rng".into(),
            record["workload"]["rng"]["next_before"].clone(),
        );
    }
    value
}
fn profile(
    records: &[&Value],
    profile: &str,
    revision: &str,
    iterations: u64,
    cpus: Option<&[u64]>,
    page_size: Option<u64>,
    reference: Option<&BTreeMap<String, Contract>>,
) -> Result<BTreeMap<String, Contract>> {
    let expected = cases(profile);
    let contract = release_rows::Request {
        profile: profile.into(),
        revision: revision.into(),
        rows: Value::Null,
        iteration_counts: None,
        default_iterations: Some(iterations),
        complete: false,
    };
    let groups =
        sampling_contract::batch::validate_groups(records, &contract, &expected, cpus, page_size)?;
    benchmark_contract::observations_ref(profile, records, None, false)?;
    if reference.is_some_and(|r| r.keys().ne(expected.iter())) {
        return Err("complete inventory reference catalog is required".into());
    }
    let mut current = BTreeMap::new();
    for (case, indices) in groups {
        // The validated worker has an index-zero first observation and fixed layout.
        let record = records[indices[0]];
        let prepared = workload(record);
        if let Some(prior) = reference.and_then(|r| r.get(case)) {
            benchmark_contract::compare_row(profile, &prior.row, &record["rows"][0])?;
            if prior.placement != record["placement"] || prior.workload != prepared {
                return Err("inventory preparation/placement differs between phases".into());
            }
        }
        current.insert(
            case.into(),
            Contract {
                row: record["rows"][0].clone(),
                placement: record["placement"].clone(),
                workload: prepared,
            },
        );
    }
    Ok(current)
}
fn retained(
    data: &BTreeMap<String, Vec<Value>>,
    sources: &Sources,
    cpus: Option<&[u64]>,
    page_size: Option<u64>,
) -> Result<Value> {
    phases(data)?;
    let profiles = ["small_file", "stream"];
    let catalogs = profiles.map(cases);
    let all: BTreeSet<_> = catalogs.iter().flatten().map(String::as_str).collect();
    let mut references = BTreeMap::new();
    let mut layouts = BTreeMap::new();
    for phase in PHASES {
        let records = &data[phase];
        if records
            .iter()
            .any(|r| r["case"].as_str().is_none_or(|c| !all.contains(c)))
        {
            return Err("unexpected inventory case".into());
        }
        let revision = if phase.starts_with('A') {
            &sources.baseline.commit
        } else {
            &sources.candidate.commit
        };
        for (name, catalog) in profiles.iter().zip(&catalogs) {
            let rows: Vec<_> = records
                .iter()
                .filter(|r| r["case"].as_str().is_some_and(|c| catalog.contains(c)))
                .collect();
            let contract = profile(
                &rows,
                name,
                revision,
                1,
                cpus,
                page_size,
                references.get(name),
            )?;
            references.insert(name, contract);
        }
        let layout: BTreeMap<_, _> = records
            .iter()
            .filter(|r| r["kind"] == "sample")
            .filter_map(|r| r["case"].as_str().map(|c| (c, &r["fixture"])))
            .collect();
        layouts.insert(phase, layout);
    }
    let mut checks = Vec::new();
    for (left, right) in PAIRS {
        for case in &all {
            let before = layouts[left][case];
            let after = layouts[right][case];
            let mut changed = Vec::new();
            for role in ["source", "scratch"] {
                let a = before[role].as_array().ok_or("missing fixture positions")?;
                let b = after[role].as_array().ok_or("missing fixture positions")?;
                for (index, (a, b)) in a.iter().zip(b).enumerate() {
                    let fields: BTreeSet<_> = a
                        .as_object()
                        .ok_or("missing fixture position")?
                        .keys()
                        .filter(|k| a[*k] != b[*k])
                        .collect();
                    if !fields.is_empty() {
                        changed.push(json!({"role":role,"index":index,"fields":fields}));
                    }
                }
            }
            checks.push(json!({"case":case,"pair":format!("{left}/{right}"),"matches":before==after,"different_buffers":changed}));
        }
    }
    Ok(
        json!({"purpose":"one-iteration structural layout inventory; no timing inference",
        "release_approval":false,"candidate_attribution":null,"case_count":all.len(),"phase_count":data.len(),
        "dynamic_output_layouts_observed":false,"layout_mismatches":mismatches(&checks),"layout_checks":checks}),
    )
}
fn mismatches(checks: &[Value]) -> Vec<&Value> {
    checks.iter().filter(|c| c["matches"] == false).collect()
}
fn stream_report(data: &BTreeMap<String, Vec<views::Inventory>>) -> Result<Value> {
    phases(data)?;
    let expected = cases("stream");
    let mut signatures = BTreeMap::new();
    let mut lifecycle = BTreeMap::new();
    for phase in PHASES {
        let mut groups: BTreeMap<&str, (u64, BTreeSet<_>)> = BTreeMap::new();
        let mut windows = Vec::new();
        for observation in &data[phase] {
            if !expected.contains(&observation.case) {
                return Err("unexpected stream-view inventory case".into());
            }
            let (count, shapes) = groups.entry(&observation.case).or_default();
            if observation.observation != *count {
                return Err("incomplete stream-view inventory schedule".into());
            }
            *count += 1;
            shapes.extend(&observation.shapes);
            windows.push(json!({"case":observation.case,"observation":observation.observation,"calls":observation.calls}));
        }
        if groups.len() != expected.len() || groups.values().any(|(count, _)| *count != 12) {
            return Err("complete stream-view inventory catalog is required".into());
        }
        signatures.insert(phase, groups);
        lifecycle.insert(phase, windows);
    }
    let mut checks = Vec::new();
    for (left, right) in PAIRS {
        for case in &expected {
            let a = &signatures[left][case.as_str()].1;
            let b = &signatures[right][case.as_str()].1;
            checks.push(
                json!({"case":case,"pair":format!("{left}/{right}"),"matches":a==b,
                "different_view_shapes":a.symmetric_difference(b).count()}),
            );
        }
    }
    Ok(
        json!({"purpose":"instrumented real I/O view/lifecycle inventory; no timing inference",
        "release_approval":false,"candidate_attribution":null,"case_count":expected.len(),"phase_count":4,
        "observations":data.values().map(Vec::len).sum::<usize>(),
        "coverage":{"stream_case_lifecycles":15,"caller_owned_destination_cases":10,"internal_read_views":10,
            "internal_parity_write_views":5,"native_virtual_page_snapshots":true,"former_encode_scratch_ranges":true,
            "former_writer_ranges":true,"physical_page_identity":false,"first_fault_instruction":false,
            "internal_verify_scratch":false,"internal_recovery_scratch":false,"non_stream_dynamic_buffers":false,
            "formal_performance_geometry_equivalence":false},
        "lifecycle_windows":lifecycle,"layout_mismatches":mismatches(&checks),"layout_checks":checks}),
    )
}

#[cfg(test)]
mod tests;
