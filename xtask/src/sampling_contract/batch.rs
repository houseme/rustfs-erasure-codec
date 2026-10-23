//! One borrowed validation/selection pass for a complete profile or case subset.
use crate::{Result, diagnostics::baseline, statistics};
use serde::Deserialize;
use serde_json::{Value, json};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Output {
    Groups,
    Select { metric: String, diagnostic: bool },
    Describe { fixtures: BTreeMap<String, Value> },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    profile: String,
    revision: String,
    records: Vec<Value>,
    expected: BTreeSet<String>,
    counts: BTreeMap<String, u64>,
    allowed_cpus: Option<Vec<u64>>,
    expected_page_size: Option<u64>,
    output: Output,
}
impl Request {
    pub(super) fn execute(self) -> Result<Value> {
        let contract = crate::release_rows::Request {
            profile: self.profile,
            rows: Value::Null,
            revision: self.revision,
            iteration_counts: Some(self.counts),
            default_iterations: None,
            complete: false,
        };
        let groups = validate_groups(
            &self.records,
            &contract,
            &self.expected,
            self.allowed_cpus.as_deref(),
            self.expected_page_size,
        )?;
        if matches!(self.output, Output::Groups) {
            return Ok(
                json!({"groups":groups,"selected_indices":[],"spreads":[],"descriptions":[]}),
            );
        }
        let mut selected = Vec::new();
        let mut spreads = Vec::new();
        let mut descriptions = Vec::new();
        for (case, indices) in &groups {
            let rows: Vec<_> = indices.iter().map(|i| &self.records[*i]).collect();
            match &self.output {
                Output::Groups => {}
                Output::Select { metric, diagnostic } => {
                    let measured = &rows[3..];
                    let short: Vec<_> = measured.iter().filter(|r|
                        r["wall_ns"].as_f64().is_some_and(|n| n < super::MINIMUM_WALL_NS as f64)
                        || r["timed_ns"].as_f64().is_some_and(|n| n < super::MINIMUM_TIMED_NS as f64))
                        .map(|r|json!({"index":r["index"],"wall_ns":r["wall_ns"],"timed_ns":r["timed_ns"]})).collect();
                    if !short.is_empty() && !diagnostic {
                        return Err("sample measurement interval is insufficient".into());
                    }
                    let ordered =
                        statistics::ordered(measured.iter().map(|r| &r["rows"][0][metric]), false)?;
                    let median = ordered[4];
                    let offset = measured
                        .iter()
                        .position(|r| std::ptr::eq(&r["rows"][0][metric], median))
                        .ok_or("selected observation is absent")?;
                    selected.push(indices[offset + 3]);
                    let mut spread = json!({"case":case,"central_spread":statistics::ratio(&statistics::difference(ordered[7],ordered[1]),median)?});
                    if *diagnostic {
                        spread["short_samples"] = json!(short);
                    }
                    spreads.push(spread);
                }
                Output::Describe { fixtures } => {
                    descriptions.push(json!({"case":case,"detail":baseline::describe(&rows,crate::release_rows::metric(&contract.profile)?,fixtures.get(*case))?}));
                }
            }
        }
        descriptions.sort_by(|a, b| a["case"].as_str().cmp(&b["case"].as_str()));
        Ok(
            json!({"groups":groups,"selected_indices":selected,"spreads":spreads,"descriptions":descriptions}),
        )
    }
}

/// Preserve original indices while sharing full worker validation with inventories.
pub(crate) fn validate_groups<'a, T: Borrow<Value>>(
    records: &'a [T],
    contract: &crate::release_rows::Request,
    expected: &BTreeSet<String>,
    cpus: Option<&[u64]>,
    page: Option<u64>,
) -> Result<Vec<(&'a str, Vec<usize>)>> {
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    let mut positions = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let record = record.borrow();
        let case = record["case"]
            .as_str()
            .ok_or("sampling revision or case mismatch")?;
        if record["revision"] != contract.revision || !expected.contains(case) {
            return Err("sampling revision or case mismatch".into());
        }
        let group = *positions.entry(case).or_insert_with(|| {
            groups.push((case, Vec::with_capacity(12)));
            groups.len() - 1
        });
        groups[group].1.push(index);
    }
    if positions.len() != expected.len() {
        return Err("complete sampling catalog is required".into());
    }
    for (case, indices) in &groups {
        let rows: Vec<_> = indices.iter().map(|i| records[*i].borrow()).collect();
        let count = contract
            .iteration_counts
            .as_ref()
            .and_then(|counts| counts.get(*case))
            .copied()
            .or(contract.default_iterations)
            .ok_or("missing frozen sample count")?;
        super::worker(&rows, &contract.profile, count, cpus, page)?;
        for row in rows {
            contract.validate_row(&row["rows"][0], case)?;
        }
    }
    Ok(groups)
}

#[cfg(test)]
mod tests;
