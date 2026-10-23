//! Complete original sampling members share the existing borrowed validation models.
use super::{PROFILES, batch, calibration_profile, preparation::Preparation};
use crate::{
    Result,
    abba::STAGES,
    artifact::{
        python_json::equivalent,
        snapshot::{MEMBER_LIMIT, Snapshot},
    },
    release_rows,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    capture: PathBuf,
    document: Value,
    summarized: Option<Value>,
    diagnostic: bool,
}
fn member(snapshot: &mut Snapshot, entry: &Value, expected: &str) -> Result<Vec<Value>> {
    let sha = entry["sha256"]
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("invalid sampling evidence declaration")?;
    if !entry.is_object() || entry["path"] != expected {
        return Err("invalid sampling evidence declaration".into());
    }
    match snapshot.python_json_bound(expected, Some(sha))? {
        Value::Array(rows) => Ok(rows),
        _ => Err("original sample array is required".into()),
    }
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        let counts = &self.document["sampling"]["common_iterations"];
        if !counts.as_object().is_some_and(|v| {
            v.len() == PROFILES.len() && PROFILES.iter().all(|p| v.contains_key(*p))
        }) {
            return Err("complete common iteration counts are required".into());
        }
        // Supplied summaries retain the 24-member raw stage; directory replay
        // additionally binds 16 summaries, preserving each original file limit.
        let mut snapshot = Snapshot::with_budget(
            &self.capture,
            ((if self.summarized.is_some() { 6 } else { 10 }) * PROFILES.len() + 1) as u64
                * MEMBER_LIMIT,
        )?;
        let summaries = summaries::Summaries::new(&mut snapshot, &self.document, self.summarized)?;
        let context = &self.document["contexts"]["A1"];
        let page: Option<u64> = serde_json::from_value(
            context["runtime"]
                .get("page_size")
                .ok_or("missing recorded page size")?
                .clone(),
        )?;
        let cpus: Option<Vec<u64>> = serde_json::from_value(
            context["isolation"]
                .get("cpu_affinity")
                .ok_or("missing recorded CPU policy")?
                .clone(),
        )?;
        let mut spreads = Vec::new();
        let mut workers = json!({});
        let catalog = super::case_catalog();
        for profile in PROFILES {
            let mut pilots = Vec::with_capacity(2);
            for role in ["baseline", "candidate"] {
                let path = format!("calibration-{role}/{profile}-samples.json");
                let rows = member(
                    &mut snapshot,
                    &self.document["calibration"][role][profile],
                    &path,
                )?;
                let revision = self.document["sources"][role]["commit"]
                    .as_str()
                    .ok_or("missing measured source")?;
                let calibrated: BTreeMap<String, u64> =
                    calibration_profile::counts(&rows, profile, revision, page, cpus.as_deref())?
                        .into_iter()
                        .map(|(case, count)| (case.to_owned(), count))
                        .collect();
                pilots.push((rows, calibrated));
            }
            let prepared = [
                Preparation::new(&pilots[0].0, true)?,
                Preparation::new(&pilots[1].0, true)?,
            ];
            let prepared_catalog = [prepared[0].catalog(), prepared[1].catalog()];
            prepared[0].validate_observations(profile, &pilots[1].0, true)?;
            for (index, role) in ["baseline", "candidate"].into_iter().enumerate() {
                workers[format!("calibration-{role}")][profile] =
                    json!(prepared[index].worker_starts);
            }
            let expected: BTreeSet<String> = catalog[profile].iter().cloned().collect();
            let common: BTreeMap<String, u64> = expected
                .iter()
                .map(|case| (case.clone(), pilots[0].1[case].max(pilots[1].1[case])))
                .collect();
            if !equivalent(&counts[profile], &json!(common)) {
                return Err("common iterations differ from complete calibration".into());
            }
            let frozen: BTreeMap<String, u64> = serde_json::from_value(counts[profile].clone())?;
            let mut first: Option<Value> = None;
            for phase in STAGES {
                let role = if phase.starts_with('A') { 0 } else { 1 };
                let role_name = if role == 0 { "baseline" } else { "candidate" };
                let revision = self.document["sources"][role_name]["commit"]
                    .as_str()
                    .ok_or("missing measured source")?;
                let rows = member(
                    &mut snapshot,
                    &self.document["files"][phase][profile]["samples"],
                    &format!("{phase}/{profile}-samples.json"),
                )?;
                let contract = release_rows::Request {
                    profile: profile.into(),
                    rows: Value::Null,
                    revision: revision.into(),
                    iteration_counts: Some(frozen.clone()),
                    default_iterations: None,
                    complete: true,
                };
                let phase_context = &self.document["contexts"][phase];
                let phase_page: Option<u64> = serde_json::from_value(
                    phase_context["runtime"]
                        .get("page_size")
                        .ok_or("missing recorded page size")?
                        .clone(),
                )?;
                let phase_cpus: Option<Vec<u64>> = serde_json::from_value(
                    phase_context["isolation"]
                        .get("cpu_affinity")
                        .cloned()
                        .unwrap_or(Value::Null),
                )?;
                let groups = batch::validate_groups(
                    &rows,
                    &contract,
                    &expected,
                    phase_cpus.as_deref(),
                    phase_page,
                )?;
                let mut selected = Vec::with_capacity(groups.len());
                for (case, indices) in groups {
                    let observations: Vec<_> = indices.iter().map(|i| &rows[*i]).collect();
                    let (offset, mut spread) = batch::select(
                        &observations,
                        case,
                        release_rows::metric(profile)?,
                        self.diagnostic,
                    )?;
                    selected.push(rows[indices[offset]]["rows"][0].clone());
                    spread["phase"] = json!(phase);
                    spread["profile"] = json!(profile);
                    spreads.push(spread);
                }
                let current = Preparation::new(&rows, false)?;
                workers[phase][profile] = json!(current.worker_starts);
                prepared[role].validate_observations(profile, &rows, false)?;
                let current = current.catalog();
                let pilot = &prepared_catalog[role];
                if !equivalent(&current["workloads"], &pilot["workloads"]) {
                    return Err(format!("{profile}/{phase}: preparation or initial input state differs from final calibration").into());
                }
                for (case, trace) in current["traces"]
                    .as_object()
                    .ok_or("missing sample traces")?
                {
                    if pilots[role].1[case] == common[case]
                        && !equivalent(trace, &pilot["traces"][case])
                    {
                        return Err(format!("{profile}/{phase}: replayed random input differs from final calibration").into());
                    }
                }
                if !equivalent(&current["fixtures"], &pilot["fixtures"]) {
                    return Err(format!(
                        "{profile}/{phase}: buffer layout differs from final calibration"
                    )
                    .into());
                }
                let recomputed = release_rows::Request {
                    rows: json!(selected),
                    ..contract
                }
                .execute()?;
                let recomputed: BTreeMap<String, Value> =
                    serde_json::from_value::<Vec<(String, Value)>>(recomputed)?
                        .into_iter()
                        .collect();
                if !summaries.matches(phase, profile, &recomputed) {
                    return Err("summary is not the median of all fixed samples".into());
                }
                if let Some(first) = &first {
                    if !equivalent(&current["workloads"], &first["workloads"])
                        || !equivalent(&current["traces"], &first["traces"])
                    {
                        return Err(format!("{profile}: API/preparation or replayed random input differs between ABBA phases").into());
                    }
                    if !equivalent(&current["fixtures"], &first["fixtures"]) {
                        return Err(format!(
                            "{profile}: buffer layout differs between ABBA phases"
                        )
                        .into());
                    }
                } else {
                    first = Some(current);
                }
            }
        }
        let invocations = if self.diagnostic {
            let entry = &self.document["worker_journal"];
            if !entry.is_object() || entry["path"] != "worker-journal.jsonl" {
                return Err("complete digest-bound worker journal is required".into());
            }
            let sha = entry["sha256"]
                .as_str()
                .ok_or("original member digest mismatch")?;
            let raw = snapshot.text_bound("worker-journal.jsonl", Some(sha))?;
            serde_json::from_value::<super::journal::Request>(json!({"raw":raw,"workers":workers}))?
                .execute()?
        } else {
            Value::Null
        };
        let unstable = spreads.iter().any(|r| {
            r["central_spread"]
                .as_f64()
                .is_some_and(|n| n > super::CENTRAL_SPREAD_LIMIT)
        });
        let mut result =
            json!({"sample_spreads":spreads,"worker_invocations":invocations,"unstable":unstable});
        summaries.report(&mut result, &spreads, self.diagnostic, unstable)?;
        snapshot.finish()?;
        Ok(result)
    }
}

mod summaries;

#[cfg(test)]
mod tests;
