//! Read-only statistical inputs. Integrity is separate from timing qualification.
use super::*;
use crate::abba::STAGES;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
mod criterion;
pub(super) fn validate_raw(
    sample: serde_json::Value,
    estimates: serde_json::Value,
    minimum: usize,
    confidence: f64,
    max_width: f64,
) -> Result<f64> {
    let distribution = criterion::analyze(sample, estimates, minimum, confidence)?;
    if distribution.relative_ci_width > max_width {
        return Err("original CI width exceeds the predeclared limit".into());
    }
    Ok(distribution.mean_ns)
}
pub(super) fn validate_configuration(value: serde_json::Value) -> Result<()> {
    let doc: Document = serde_json::from_value(value)?;
    doc.validate()
}
#[cfg(test)]
pub(crate) mod tests;
const SCHEMA: &str = "plan-bench-statistical-inputs-v1";

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Origin {
    Synthetic,
    Recorded,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    pub(super) origin: Origin,
    baseline_sha256: String,
    candidate_sha256: String,
    repetitions: u8,
    minimum_samples: usize,
    confidence_level: f64,
    max_ci_width: f64,
    max_drift: f64,
    attempts: Vec<Attempt>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    repetition: u8,
    stage: String,
    manifest_sha256: String,
    outcome: Outcome,
    originals_unchanged: bool,
    cases: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    case_index: usize,
    benchmark_sha256: Option<String>,
    sample_sha256: Option<String>,
    estimates_sha256: Option<String>,
}
#[derive(Serialize)]
pub(super) struct AttemptSummary {
    repetition: u8,
    stage: String,
    pub(super) outcome: Outcome,
    originals_unchanged: bool,
}
#[derive(Serialize)]
struct Observation {
    repetition: u8,
    stage: String,
    case: String,
    distribution: criterion::Distribution,
}
#[derive(Serialize)]
struct Drift {
    repetition: u8,
    case: String,
    baseline: Option<f64>,
    candidate: Option<f64>,
    baseline_repeat: Option<f64>,
    candidate_repeat: Option<f64>,
}
#[derive(Serialize)]
pub(super) struct Report {
    pub(super) schema: &'static str,
    pub(super) input_sha256: String,
    pub(super) origin: Origin,
    pub(super) catalog_cases: usize,
    pub(super) repetitions: u8,
    pub(super) minimum_samples: usize,
    pub(super) confidence_level: f64,
    pub(super) max_ci_width: f64,
    pub(super) max_drift: f64,
    pub(super) integrity_accepted: bool,
    pub(super) diagnostics_passed: bool,
    pub(super) performance_accepted: bool,
    pub(super) capture_provenance_verified: bool,
    pub(super) attempts: Vec<AttemptSummary>,
    observations: Vec<Observation>,
    drift: Vec<Drift>,
    pub(super) errors: Vec<String>,
    pub(super) diagnostic_failures: Vec<String>,
}
impl Document {
    fn validate(&self) -> Result<()> {
        valid_digest(&self.baseline_sha256)?;
        valid_digest(&self.candidate_sha256)?;
        if self.schema != SCHEMA
            || !(1..=9).contains(&self.repetitions)
            || !(10..=100_000).contains(&self.minimum_samples)
            || !self.confidence_level.is_finite()
            || !(0.0..1.0).contains(&self.confidence_level)
            || self.confidence_level == 0.0
            || [self.max_ci_width, self.max_drift]
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("unsupported statistical input protocol or diagnostic policy".into());
        }
        Ok(())
    }
    fn pin(&self, index: usize) -> &str {
        if index.is_multiple_of(4) || index % 4 == 3 {
            &self.baseline_sha256
        } else {
            &self.candidate_sha256
        }
    }
}
fn member(
    snapshot: &mut Snapshot,
    prefix: &str,
    name: &str,
    pin: &Option<String>,
) -> Result<serde_json::Value> {
    let pin = pin
        .as_deref()
        .ok_or("missing original statistical member")?;
    valid_digest(pin)?;
    snapshot.json_bound(&format!("{prefix}/{name}.json"), Some(pin))
}
// Hash/encoding/path/budget failures stop immediately. Numeric diagnostic
// refusals are collected below; they never bypass the original-read budget.
fn read_case(
    snapshot: &mut Snapshot,
    prefix: &str,
    entry: &Entry,
) -> Result<[serde_json::Value; 3]> {
    Ok([
        member(snapshot, prefix, "benchmark", &entry.benchmark_sha256)?,
        member(snapshot, prefix, "sample", &entry.sample_sha256)?,
        member(snapshot, prefix, "estimates", &entry.estimates_sha256)?,
    ])
}

fn relative(
    later: Option<f64>,
    earlier: Option<f64>,
) -> Option<std::result::Result<f64, &'static str>> {
    later.zip(earlier).map(|(a, b)| {
        let value = a / b - 1.0;
        if value.is_finite() {
            Ok(value)
        } else {
            Err("nonfinite drift")
        }
    })
}
fn center(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    a.zip(b).map(|(a, b)| a + (b - a) / 2.0)
}
pub(super) fn drift_values(
    current: [Option<f64>; 4],
    first: [Option<f64>; 4],
) -> [Option<std::result::Result<f64, &'static str>>; 4] {
    let [a1, b1, b2, a2] = current;
    [
        relative(a2, a1),
        relative(b2, b1),
        relative(center(a1, a2), center(first[0], first[3])),
        relative(center(b1, b2), center(first[1], first[2])),
    ]
}
fn verify(root: &Path, pin: &str) -> Result<Report> {
    let mut snapshot = Snapshot::new(root)?;
    let (mut report, _) = bound(&mut snapshot, "", pin)?;
    if let Err(error) = snapshot.finish() {
        report
            .errors
            .push(format!("original snapshot refused: {error}"));
    }
    report.integrity_accepted = report.errors.is_empty();
    report.diagnostics_passed = report.integrity_accepted && report.diagnostic_failures.is_empty();
    Ok(report)
}
pub(super) struct Binding {
    pub baseline_sha256: String,
    pub candidate_sha256: String,
    pub baseline: Manifest,
    pub candidate: Manifest,
    pub attempts: Vec<String>,
    pub attempt_times: Vec<Option<f64>>,
    pub repetitions: u8,
}
pub(super) fn bound(snapshot: &mut Snapshot, prefix: &str, pin: &str) -> Result<(Report, Binding)> {
    valid_digest(pin)?;
    let wire = snapshot.json_bound(&format!("{prefix}samples.json"), Some(pin))?;
    // Hash the already-decoded original Value directly: no cloned typed catalog
    // or extra Value allocation per attempt, and sorted object keys are stable.
    let attempts = wire["attempts"]
        .as_array()
        .ok_or("missing statistical attempts")?
        .iter()
        .map(|attempt| Ok(digest(&serde_json::to_vec(attempt)?)))
        .collect::<Result<Vec<_>>>()?;
    let mut document: Document = serde_json::from_value(wire)?;
    document.validate()?;
    let a = frozen_snapshot(
        snapshot,
        &format!("{prefix}sources/A/"),
        &document.baseline_sha256,
    )?;
    let b = frozen_snapshot(
        snapshot,
        &format!("{prefix}sources/B/"),
        &document.candidate_sha256,
    )?;
    compatible(&a, &b)?;
    let mut report = Report {
        schema: "plan-bench-statistical-audit-v1",
        input_sha256: pin.into(),
        origin: document.origin,
        catalog_cases: a.catalog.len(),
        repetitions: document.repetitions,
        minimum_samples: document.minimum_samples,
        confidence_level: document.confidence_level,
        max_ci_width: document.max_ci_width,
        max_drift: document.max_drift,
        integrity_accepted: false,
        diagnostics_passed: false,
        performance_accepted: false,
        capture_provenance_verified: false,
        attempts: Vec::new(),
        observations: Vec::new(),
        drift: Vec::new(),
        errors: Vec::new(),
        diagnostic_failures: Vec::new(),
    };
    let expected = usize::from(document.repetitions) * 4;
    let mut attempt_times = Vec::with_capacity(expected);
    if document.attempts.len() != expected {
        report
            .errors
            .push("incomplete repetition/stage matrix".into());
    }
    let mut means = vec![vec![[None; 4]; a.catalog.len()]; usize::from(document.repetitions)];
    for (index, attempt) in std::mem::take(&mut document.attempts)
        .into_iter()
        .enumerate()
    {
        let repetition = index / 4 + 1;
        let stage = STAGES[index % 4];
        let worker = attempt.outcome.exit_code == Some(0)
            && attempt.outcome.reason == "exited"
            && attempt.outcome.reaped
            && attempt.originals_unchanged;
        let identity = repetition <= usize::from(document.repetitions)
            && usize::from(attempt.repetition) == repetition
            && attempt.stage == stage
            && attempt.manifest_sha256 == document.pin(index);
        if !worker {
            report.errors.push(format!(
                "{repetition}/{stage}: worker failed, was not reaped or original inputs changed"
            ));
        }
        if !identity {
            report.errors.push(format!(
                "{repetition}/{stage}: repetition order or frozen pair mismatch"
            ));
        }
        if attempt.cases.len() != a.catalog.len() {
            report
                .errors
                .push(format!("{repetition}/{stage}: incomplete case catalog"));
        }
        let mut total_ns: Option<f64> = Some(0.0);
        let mut seen = BTreeSet::new();
        if repetition <= usize::from(document.repetitions) {
            for (position, entry) in attempt.cases.iter().enumerate() {
                if entry.case_index >= a.catalog.len() || !seen.insert(entry.case_index) {
                    report.errors.push(format!(
                        "{repetition}/{stage}: unknown or duplicate case index"
                    ));
                    continue;
                }
                if entry.case_index != position {
                    report
                        .errors
                        .push(format!("{repetition}/{stage}: noncanonical case order"));
                }
                let key = &a.catalog[entry.case_index];
                let member_prefix = format!(
                    "{prefix}records/{repetition:02}/{stage}/{:04}",
                    entry.case_index
                );
                if entry.benchmark_sha256.is_none()
                    || entry.sample_sha256.is_none()
                    || entry.estimates_sha256.is_none()
                {
                    report.errors.push(format!(
                        "{repetition}/{stage}/{key}: missing original statistical member"
                    ));
                    continue;
                }
                let [benchmark, sample, estimates] = read_case(snapshot, &member_prefix, entry)?;
                let checked = if benchmark["full_id"] != key.as_str() {
                    Err("benchmark identity differs from the full frozen catalog".into())
                } else {
                    criterion::analyze(
                        sample,
                        estimates,
                        document.minimum_samples,
                        document.confidence_level,
                    )
                };
                match checked {
                    Ok(distribution) => {
                        total_ns = total_ns
                            .zip(distribution.total_ns)
                            .map(|(a, b)| a + b)
                            .filter(|v| v.is_finite());
                        if distribution.relative_ci_width > document.max_ci_width {
                            report.diagnostic_failures.push(format!(
                                "{repetition}/{stage}/{key}: CI width exceeds declared policy"
                            ));
                        }
                        if worker && identity {
                            means[repetition - 1][entry.case_index][index % 4] =
                                Some(distribution.mean_ns);
                        }
                        report.observations.push(Observation {
                            repetition: attempt.repetition,
                            stage: attempt.stage.clone(),
                            case: key.clone(),
                            distribution,
                        });
                    }
                    Err(error) => report
                        .errors
                        .push(format!("{repetition}/{stage}/{key}: {error}")),
                }
            }
        }
        attempt_times.push(total_ns);
        report.attempts.push(AttemptSummary {
            repetition: attempt.repetition,
            stage: attempt.stage,
            outcome: attempt.outcome,
            originals_unchanged: attempt.originals_unchanged,
        });
    }
    for (repeat, rows) in means.iter().enumerate() {
        for (index, &values) in rows.iter().enumerate() {
            let [baseline, candidate, baseline_repeat, candidate_repeat] =
                drift_values(values, means[0][index]).map(|value| value.and_then(|r| r.ok()));
            for (name, value) in [
                ("A2/A1", baseline),
                ("B2/B1", candidate),
                ("baseline repeat", baseline_repeat),
                ("candidate repeat", candidate_repeat),
            ] {
                match value {
                    Some(v) if v.abs() > document.max_drift => {
                        report.diagnostic_failures.push(format!(
                            "{}/{}/{name}: drift exceeds declared policy",
                            repeat + 1,
                            a.catalog[index]
                        ))
                    }
                    None => report.errors.push(format!(
                        "{}/{}/{name}: incomplete or nonfinite drift inputs",
                        repeat + 1,
                        a.catalog[index]
                    )),
                    _ => {}
                }
            }
            report.drift.push(Drift {
                repetition: (repeat + 1) as u8,
                case: a.catalog[index].clone(),
                baseline,
                candidate,
                baseline_repeat,
                candidate_repeat,
            });
        }
    }
    report.integrity_accepted = report.errors.is_empty();
    report.diagnostics_passed = report.integrity_accepted && report.diagnostic_failures.is_empty();
    Ok((
        report,
        Binding {
            baseline_sha256: document.baseline_sha256,
            candidate_sha256: document.candidate_sha256,
            baseline: a,
            candidate: b,
            attempts,
            attempt_times,
            repetitions: document.repetitions,
        },
    ))
}
pub(crate) fn replay(args: &Args) -> Result<bool> {
    let report = verify(
        Path::new(args.required("--input")?),
        args.required("--manifest-sha256")?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report.integrity_accepted && report.diagnostics_passed)
}

impl Report {
    pub(super) fn matches_configuration(&self, value: &serde_json::Value) -> bool {
        value["origin"] == serde_json::to_value(self.origin).unwrap_or_default()
            && value["repetitions"].as_u64() == Some(u64::from(self.repetitions))
            && value["minimum_samples"].as_u64() == Some(self.minimum_samples as u64)
            && value["confidence_level"].as_f64() == Some(self.confidence_level)
            && value["max_ci_width"].as_f64() == Some(self.max_ci_width)
            && value["max_drift"].as_f64() == Some(self.max_drift)
            && self
                .observations
                .iter()
                .all(|v| v.distribution.samples == self.minimum_samples)
    }
}
