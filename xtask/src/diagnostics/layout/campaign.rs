//! Associate the paired slice replay with the frozen full Vec campaign.
use crate::{Result, artifact::python_json::equivalent, benchmark_contract};
use serde::Deserialize;
use serde_json::Value;

const CASES: [&str; 2] = ["verify_with_buffer:4:2:1024", "verify_with_buffer:4:2:4096"];

pub(super) fn iterations(counts: &Value) -> Result<u64> {
    CASES.into_iter().try_fold(0, |maximum, case| {
        counts[case]
            .as_u64()
            .filter(|n| (1..=1_000_000_000).contains(n))
            .map(|n| maximum.max(n))
            .ok_or_else(|| {
                "layout iterations require both completed buffered-verify calibration counts".into()
            })
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    sources: Value,
    context: Value,
    counts: Value,
    rows: Value,
}
impl Binding {
    pub(super) fn validate(&self, layout: &Value) -> Result<()> {
        if ["tree", "lock_sha256"]
            .into_iter()
            .any(|key| layout["tool"][key] != self.sources["candidate"][key])
        {
            return Err("paired-layout tool does not match the frozen candidate tree/lock".into());
        }
        if !equivalent(&layout["sources"], &self.sources)
            || !equivalent(&layout["context"], &self.context)
        {
            return Err(
                "paired-layout source or resource context differs from the full campaign".into(),
            );
        }
        if layout["iterations"].as_u64() != Some(iterations(&self.counts)?) {
            return Err(
                "paired-layout iterations differ from the frozen full-campaign calibration".into(),
            );
        }
        let native = &layout["phase_reports"]["A1"]["provenance"];
        let prefix = format!(
            "{}-{}-",
            native["target_arch"]
                .as_str()
                .ok_or("missing layout target")?,
            native["target_os"]
                .as_str()
                .ok_or("missing layout target")?
        );
        for case in CASES {
            let row = self.rows[case]
                .as_object()
                .ok_or("missing original buffered-verify case")?;
            let fields_match = ["backend", "backend_id", "backend_kind", "features"]
                .into_iter()
                .try_fold(true, |matches, field| -> Result<bool> {
                    Ok(matches
                        && benchmark_contract::provenance_value(row, field)?.as_deref()
                            == native[field].as_str())
                })?;
            let target = benchmark_contract::provenance_value(row, "target_triple")?;
            if !fields_match || !target.is_some_and(|target| target.starts_with(&prefix)) {
                return Err(format!("{case}: paired-layout backend/features/target differ from the original buffered-verify cases").into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
