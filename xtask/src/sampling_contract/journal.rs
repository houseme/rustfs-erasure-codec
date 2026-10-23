//! Match the campaign journal against identities from already validated profiles.
use crate::{Result, statistics};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const PHASES: [&str; 6] = [
    "calibration-baseline",
    "calibration-candidate",
    "A1",
    "B1",
    "B2",
    "A2",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerStart {
    case: String,
    iterations: u64,
    calibration_batch: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    raw: String,
    workers: BTreeMap<String, BTreeMap<String, Vec<WorkerStart>>>,
}
// Identity fields used Python's numeric equality. Preserve that rule without
// relaxing the separate integer checks for timestamps and exit status.
fn identity_number(value: &Value, expected: u64) -> bool {
    match value {
        Value::Bool(v) => u64::from(*v) == expected,
        Value::Number(_) => statistics::compare(value, &json!(expected)).is_eq(),
        _ => false,
    }
}
impl Request {
    pub(crate) fn execute(&self) -> Result<Value> {
        if self.workers.len() != PHASES.len()
            || PHASES.iter().any(|p| !self.workers.contains_key(*p))
        {
            return Err("complete worker journal phase catalog is required".into());
        }
        let mut lines = self.raw.lines();
        let (mut previous, mut invocations) = (0, 0_u64);
        for phase in PHASES {
            let profiles = &self.workers[phase];
            if profiles.len() != super::PROFILES.len()
                || super::PROFILES.iter().any(|p| !profiles.contains_key(*p))
            {
                return Err("complete worker journal profile catalog is required".into());
            }
            let mode = if phase.starts_with("calibration-") {
                "calibrate"
            } else {
                "sample"
            };
            for profile in super::PROFILES {
                let jobs = &profiles[profile];
                if jobs.is_empty() {
                    return Err("complete worker journal identities are required".into());
                }
                for job in jobs {
                    for (position, event) in ["start", "end"].into_iter().enumerate() {
                        let line = lines
                            .next()
                            .ok_or("complete start/end worker journal is required")?;
                        // Preserve the previous original JSONL parser's last-key semantics.
                        let row: Value = serde_json::from_str(line)?;
                        let Some(fields) = row.as_object() else {
                            return Err("worker journal order or identity differs".into());
                        };
                        const KEYS: &[&str] = &[
                            "phase",
                            "profile",
                            "mode",
                            "case",
                            "iterations",
                            "calibration_batch",
                            "event",
                            "monotonic_ns",
                            "utc_ns",
                        ];
                        let expected = KEYS.len() + position;
                        let monotonic = statistics::uint(&row["monotonic_ns"])?;
                        statistics::uint(&row["utc_ns"])?;
                        if fields.len() != expected
                            || KEYS.iter().any(|k| !fields.contains_key(*k))
                            || row["phase"] != phase
                            || row["profile"] != profile
                            || row["mode"] != mode
                            || row["case"] != job.case
                            || !identity_number(&row["iterations"], job.iterations)
                            || !identity_number(&row["calibration_batch"], job.calibration_batch)
                            || row["event"] != event
                            || monotonic < previous
                            || position == 1 && row["exit_code"].as_u64() != Some(0)
                        {
                            return Err("worker journal order, identity or outcome differs from original records".into());
                        }
                        previous = monotonic;
                    }
                    invocations += 1;
                }
            }
        }
        if lines.next().is_some() {
            return Err("complete start/end worker journal is required".into());
        }
        Ok(json!(invocations))
    }
}

#[cfg(test)]
mod tests;
