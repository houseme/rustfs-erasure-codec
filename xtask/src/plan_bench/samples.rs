//! Read-only statistical inputs. Integrity is separate from timing qualification.
use super::*;
use crate::abba::STAGES;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
mod criterion;
#[cfg(test)]
mod tests;
const SCHEMA: &str = "plan-bench-statistical-inputs-v1";

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Origin {
    Synthetic,
    Recorded,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    origin: Origin,
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
struct AttemptSummary {
    repetition: u8,
    stage: String,
    outcome: Outcome,
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
struct Report {
    schema: &'static str,
    input_sha256: String,
    origin: Origin,
    catalog_cases: usize,
    repetitions: u8,
    minimum_samples: usize,
    confidence_level: f64,
    max_ci_width: f64,
    max_drift: f64,
    integrity_accepted: bool,
    diagnostics_passed: bool,
    performance_accepted: bool,
    capture_provenance_verified: bool,
    attempts: Vec<AttemptSummary>,
    observations: Vec<Observation>,
    drift: Vec<Drift>,
    errors: Vec<String>,
    diagnostic_failures: Vec<String>,
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

fn relative(later: Option<f64>, earlier: Option<f64>) -> Option<f64> {
    later
        .zip(earlier)
        .map(|(a, b)| a / b - 1.0)
        .filter(|v| v.is_finite())
}
fn center(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    a.zip(b).map(|(a, b)| a + (b - a) / 2.0)
}
fn verify(root: &Path, pin: &str) -> Result<Report> {
    valid_digest(pin)?;
    let mut snapshot = Snapshot::new(root)?;
    let mut document: Document =
        serde_json::from_value(snapshot.json_bound("samples.json", Some(pin))?)?;
    document.validate()?;
    let a = frozen_snapshot(&mut snapshot, "sources/A/", &document.baseline_sha256)?;
    let b = frozen_snapshot(&mut snapshot, "sources/B/", &document.candidate_sha256)?;
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
                let prefix = format!("records/{repetition:02}/{stage}/{:04}", entry.case_index);
                if entry.benchmark_sha256.is_none()
                    || entry.sample_sha256.is_none()
                    || entry.estimates_sha256.is_none()
                {
                    report.errors.push(format!(
                        "{repetition}/{stage}/{key}: missing original statistical member"
                    ));
                    continue;
                }
                let [benchmark, sample, estimates] = read_case(&mut snapshot, &prefix, entry)?;
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
        report.attempts.push(AttemptSummary {
            repetition: attempt.repetition,
            stage: attempt.stage,
            outcome: attempt.outcome,
            originals_unchanged: attempt.originals_unchanged,
        });
    }
    for (repeat, rows) in means.iter().enumerate() {
        for (index, &[a1, b1, b2, a2]) in rows.iter().enumerate() {
            let baseline = relative(a2, a1);
            let candidate = relative(b2, b1);
            let baseline_repeat = relative(
                center(a1, a2),
                center(means[0][index][0], means[0][index][3]),
            );
            let candidate_repeat = relative(
                center(b1, b2),
                center(means[0][index][1], means[0][index][2]),
            );
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
    if let Err(error) = snapshot.finish() {
        report
            .errors
            .push(format!("original snapshot refused: {error}"));
    }
    report.integrity_accepted = report.errors.is_empty();
    report.diagnostics_passed = report.integrity_accepted && report.diagnostic_failures.is_empty();
    Ok(report)
}
pub(crate) fn replay(args: &Args) -> Result<bool> {
    let report = verify(
        Path::new(args.required("--input")?),
        args.required("--manifest-sha256")?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report.integrity_accepted && report.diagnostics_passed)
}
