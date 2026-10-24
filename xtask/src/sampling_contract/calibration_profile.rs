//! Complete calibration profiles reuse the original row and rehearsal models.
use super::{calibration_worker, protocol, uint};
use crate::{Result, benchmark_contract, release_rows};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn analyze(
    records: &[Value],
    profile: &str,
    revision: &str,
    page: Option<u64>,
    cpus: Option<&[u64]>,
) -> Result<Value> {
    let counts = counts(records, profile, revision, page, cpus)?;
    let preparation = super::preparation::Preparation::new(records, true)?;
    Ok(
        json!({"counts":counts,"preparation":preparation.catalog(),"worker_starts":preparation.worker_starts}),
    )
}

pub(crate) fn counts<'a>(
    records: &'a [Value],
    profile: &str,
    revision: &str,
    page: Option<u64>,
    cpus: Option<&[u64]>,
) -> Result<Vec<(&'a str, u64)>> {
    let catalog = super::case_catalog();
    let expected = catalog.get(profile).ok_or("unknown calibration profile")?;
    let expected: BTreeSet<_> = expected.iter().map(String::as_str).collect();
    let mut groups: Vec<(&str, Vec<&Value>)> = Vec::new();
    let mut positions = BTreeMap::new();
    let mut contract = release_rows::Request {
        profile: profile.into(),
        rows: Value::Null,
        revision: revision.into(),
        iteration_counts: None,
        default_iterations: None,
        complete: false,
    };
    for record in records {
        let case = record["case"]
            .as_str()
            .ok_or("invalid calibration sample")?;
        let count = uint(&record["iterations"], "invalid calibration sample")?;
        if record["isolated_case"] != true
            || record["kind"] != "calibration" && record["kind"] != "calibration_warmup"
            || record["revision"] != revision
            || !expected.contains(case)
            || !(1..=1_000_000_000).contains(&count)
        {
            return Err("invalid calibration sample".into());
        }
        if !record["rows"].as_array().is_some_and(|r| r.len() == 1) {
            return Err("calibration must contain one raw row".into());
        }
        contract.default_iterations = Some(count);
        contract.validate_row(&record["rows"][0], case)?;
        let group = *positions.entry(case).or_insert_with(|| {
            groups.push((case, Vec::new()));
            groups.len() - 1
        });
        groups[group].1.push(record);
    }
    if positions.len() != expected.len() {
        return Err("calibration case catalog is incomplete".into());
    }
    let mut counts = Vec::with_capacity(groups.len());
    for (case, group) in groups {
        if group.is_empty()
            || group.len() > protocol::MAX_CALIBRATION_RECORDS
            || group.len() % protocol::WIDTH != 0
        {
            return Err("calibration rehearsal is incomplete or exceeds its budget".into());
        }
        let mut count = 1;
        let mut completed = false;
        for (batch, rows) in group
            .as_chunks::<{ protocol::WIDTH }>()
            .0
            .iter()
            .enumerate()
        {
            let following = calibration_worker(rows, profile, count, batch as u64 + 1, page, cpus)?;
            if following.is_null() {
                if (batch + 1) * protocol::WIDTH != group.len() {
                    return Err("extra calibration records after a complete rehearsal".into());
                }
                counts.push((case, count));
                completed = true;
            } else {
                count = following.as_u64().ok_or("invalid calibration proposal")?;
            }
        }
        if !completed {
            return Err("calibration rehearsal is incomplete".into());
        }
    }
    benchmark_contract::observations_ref(profile, records, None, false)?;
    Ok(counts)
}

#[cfg(test)]
pub(crate) mod tests;
