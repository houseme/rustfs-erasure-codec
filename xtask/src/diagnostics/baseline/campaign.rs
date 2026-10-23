//! Complete campaign arithmetic and diagnostic classifications over validated rows.
use super::{PROFILES, REPEAT_LIMIT, SPREAD_LIMIT};
use crate::{
    Result, abba::STAGES, benchmark_contract, release_rows, sampling_contract, statistics,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type CampaignRows = BTreeMap<String, BTreeMap<String, Vec<(String, Value)>>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    data: CampaignRows,
    sample_spreads: Vec<Value>,
    diagnostic: bool,
}
impl Request {
    pub(super) fn execute(self) -> Result<Value> {
        analyze(&self.data, &self.sample_spreads, self.diagnostic)
    }
}
pub(crate) fn analyze(
    data: &CampaignRows,
    sample_spreads: &[Value],
    diagnostic: bool,
) -> Result<Value> {
    if data.len() != STAGES.len() || STAGES.iter().any(|p| !data.contains_key(*p)) {
        return Err("complete campaign phases are required".into());
    }
    let catalog = sampling_contract::case_catalog();
    let mut indexed = BTreeMap::new();
    for phase in STAGES {
        let profiles = &data[phase];
        if profiles.len() != PROFILES.len() {
            return Err("complete campaign profiles are required".into());
        }
        let mut sets = BTreeMap::new();
        for profile in PROFILES {
            let rows = profiles.get(profile).ok_or("missing campaign profile")?;
            let cases: BTreeMap<_, _> = rows.iter().map(|(c, r)| (c.as_str(), r)).collect();
            let expected: BTreeSet<_> = catalog[profile].iter().map(String::as_str).collect();
            if cases.len() != rows.len() || cases.keys().copied().ne(expected) {
                return Err("complete unique campaign cases are required".into());
            }
            let metric = release_rows::metric(profile)?;
            for row in cases.values() {
                if !row[metric]
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v > 0.0)
                {
                    return Err("positive finite campaign metrics are required".into());
                }
            }
            sets.insert(profile, cases);
        }
        indexed.insert(phase, sets);
    }
    let mut drifts = Vec::new();
    let mut comparisons = Vec::new();
    for profile in PROFILES {
        let metric = release_rows::metric(profile)?;
        for phase in &STAGES[1..] {
            for (case, row) in &data["A1"][profile] {
                benchmark_contract::compare_row(
                    profile,
                    row,
                    indexed[phase][profile][case.as_str()],
                )?;
            }
        }
        for (first, second) in [("A1", "A2"), ("B1", "B2")] {
            for (case, row) in &data[first][profile] {
                // The former repeat model explicitly converted both metrics to f64.
                let a = row[metric].as_f64().ok_or("missing campaign metric")?;
                let b = indexed[second][profile][case.as_str()][metric]
                    .as_f64()
                    .ok_or("missing campaign metric")?;
                let drift = (b - a).abs() / a;
                if !drift.is_finite() {
                    return Err(format!("nonfinite campaign ratio: {profile}/{case}").into());
                }
                drifts.push(json!({"profile":profile,"case":case,"pair":format!("{first}{second}"),"drift":drift}));
            }
        }
        if diagnostic {
            for a in ["A1", "A2"] {
                for b in ["B1", "B2"] {
                    for (case, row) in &data[a][profile] {
                        let old = &row[metric];
                        let new = &indexed[b][profile][case.as_str()][metric];
                        let ratio = statistics::ratio(new, old).map_err(|e| {
                            format!("nonfinite campaign ratio: {profile}/{case}: {e}")
                        })?;
                        comparisons.push(json!({"profile":profile,"pair":format!("{a}/{b}"),"case":case,
                                "metric":metric,"baseline":old,"candidate":new,"candidate_over_baseline":ratio}));
                    }
                }
            }
        }
    }
    if !diagnostic {
        return Ok(json!({"repeat_checks":drifts}));
    }
    let mut wide = Vec::new();
    let mut short = Vec::new();
    let mut spread_cases = BTreeSet::new();
    for row in sample_spreads {
        let label = |key| row[key].as_str().ok_or("missing campaign spread context");
        let (phase, profile, case) = (label("phase")?, label("profile")?, label("case")?);
        if !indexed
            .get(phase)
            .and_then(|p| p.get(profile))
            .is_some_and(|p| p.contains_key(case))
            || !spread_cases.insert((phase, profile, case))
        {
            return Err("complete unique campaign spreads are required".into());
        }
        let spread = row["central_spread"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("invalid campaign spread")?;
        if spread > SPREAD_LIMIT {
            wide.push(row);
        }
        for window in row["short_samples"]
            .as_array()
            .ok_or("missing diagnostic short samples")?
        {
            let mut fields = window
                .as_object()
                .ok_or("invalid diagnostic short sample")?
                .clone();
            for key in ["phase", "profile", "case"] {
                if fields.insert(key.into(), row[key].clone()).is_some() {
                    return Err("duplicate short sample context".into());
                }
            }
            short.push(Value::Object(fields));
        }
    }
    if spread_cases.len() != catalog.values().map(Vec::len).sum::<usize>() * STAGES.len() {
        return Err("complete campaign spreads are required".into());
    }
    let repeats: Vec<_> = drifts
        .iter()
        .filter(|r| r["drift"].as_f64().is_some_and(|v| v > REPEAT_LIMIT))
        .collect();
    Ok(
        json!({"status":"diagnostic_profiles_complete","diagnostic_only":true,"release_approval":false,
            "candidate_attribution":null,"sample_spreads":sample_spreads,"distribution_failures":wide,
            "short_samples":short,"repeat_checks":drifts,"repeat_failures":repeats,
            "unqualified_metric_comparisons":comparisons,
            "metric_direction":{"smoke":"higher","small_file":"lower","hotspot":"higher","stream":"lower"},
            "all_original_limits_passed":wide.is_empty() && short.is_empty() && repeats.is_empty(),
            "scope":"complete diagnostic; raw comparisons remain unqualified even if every check passes"}),
    )
}

#[cfg(test)]
mod tests;
