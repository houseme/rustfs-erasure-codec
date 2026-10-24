//! Original summaries retain input order while statistical analysis borrows their rows.
use super::{PROFILES, Result, STAGES, Snapshot, Value, equivalent, json, member, release_rows};
use std::collections::BTreeMap;

use crate::diagnostics::baseline::CampaignRows;
pub(super) enum Summaries {
    Supplied(Value),
    Originals(CampaignRows),
}
impl Summaries {
    pub(super) fn new(
        snapshot: &mut Snapshot,
        document: &Value,
        supplied: Option<Value>,
    ) -> Result<Self> {
        if let Some(value) = supplied {
            return Ok(Self::Supplied(value));
        }
        let files = document["files"]
            .as_object()
            .ok_or("unexpected or missing measurement phases")?;
        if files.len() != STAGES.len() || STAGES.iter().any(|phase| !files.contains_key(*phase)) {
            return Err("unexpected or missing measurement phases".into());
        }
        let mut output = BTreeMap::new();
        for phase in STAGES {
            let entries = files[phase]
                .as_object()
                .ok_or("every phase needs all four release profiles")?;
            if entries.len() != PROFILES.len() || PROFILES.iter().any(|p| !entries.contains_key(*p))
            {
                return Err("every phase needs all four release profiles".into());
            }
            let role = if phase.starts_with('A') {
                "baseline"
            } else {
                "candidate"
            };
            let revision = document["sources"][role]["commit"]
                .as_str()
                .ok_or("missing measured source")?;
            let mut profiles = BTreeMap::new();
            for profile in PROFILES {
                let filename = format!("{}.json", release_rows::artifact_kind(profile)?);
                let rows = member(snapshot, &entries[profile], &format!("{phase}/{filename}"))?;
                let counts = serde_json::from_value(
                    document["sampling"]["common_iterations"][profile].clone(),
                )?;
                let normalized = release_rows::Request {
                    profile: profile.into(),
                    rows: Value::Array(rows),
                    revision: revision.into(),
                    iteration_counts: Some(counts),
                    default_iterations: None,
                    complete: true,
                }
                .indexed()?;
                profiles.insert(profile.to_owned(), normalized);
            }
            output.insert(phase.to_owned(), profiles);
        }
        Ok(Self::Originals(output))
    }
    pub(super) fn matches(
        &self,
        phase: &str,
        profile: &str,
        actual: &BTreeMap<String, Value>,
    ) -> bool {
        match self {
            Self::Supplied(value) => equivalent(&json!(actual), &value[phase][profile]),
            Self::Originals(data) => {
                let expected = &data[phase][profile];
                actual.len() == expected.len()
                    && expected
                        .iter()
                        .all(|(case, row)| actual.get(case).is_some_and(|v| equivalent(v, row)))
            }
        }
    }
    pub(super) fn report(
        &self,
        result: &mut Value,
        spreads: &[Value],
        diagnostic: bool,
        unstable: bool,
    ) -> Result<()> {
        if let Self::Originals(data) = self {
            // Preserve formal distribution refusal precedence over ratio analysis.
            if diagnostic || !unstable {
                result["analysis"] = crate::diagnostics::baseline::analyze_campaign(
                    data,
                    if diagnostic { spreads } else { &[] },
                    diagnostic,
                )?;
            }
            result["case_counts"] = json!(
                data["A1"]
                    .iter()
                    .map(|(profile, rows)| (profile, rows.len()))
                    .collect::<BTreeMap<_, _>>()
            );
            result["layout_rows"] = json!({"A1":{"small_file":data["A1"]["small_file"].iter().map(|(case,row)|(case,row)).collect::<BTreeMap<_,_>>()}});
        }
        Ok(())
    }
}
