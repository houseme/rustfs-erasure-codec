//! Structural execution bindings and read-only live resource preflight.
use super::*;
use crate::{abba::STAGES, diagnostics::baseline::replay as baseline, sampling_contract};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
pub(crate) mod tests;
const SCHEMA: &str = "plan-bench-execution-inputs-v1";
const SECOND: u64 = 1_000_000_000;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Execution {
    pub(super) schema: String,
    pub(super) session_sha256: String,
    pub(super) samples_sha256: String,
    pub(super) baseline_members: BTreeMap<String, String>,
    pub(super) renderer_sha256: String,
    pub(super) analyzer_sha256: String,
    pub(super) stage_seconds: u64,
    pub(super) window_seconds: u64,
    pub(super) gap_seconds: u64,
    pub(super) ended_ns: u64,
    pub(super) attempts: Vec<Attempt>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attempt {
    pub(super) repetition: u8,
    pub(super) stage: String,
    pub(super) manifest_sha256: String,
    pub(super) attempt_sha256: String,
    pub(super) before_sha256: String,
    pub(super) after_sha256: String,
    pub(super) started_ns: u64,
    pub(super) ended_ns: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    session_sha256: String,
    observed_ns: u64,
    context: Value,
}
#[derive(Serialize)]
pub(super) struct Report {
    pub(super) schema: &'static str,
    pub(super) execution_sha256: String,
    pub(super) bindings_verified: bool,
    pub(super) baseline_quality_passed: bool,
    pub(super) preflight_passed: bool,
    pub(super) capture_provenance_verified: bool,
    pub(super) performance_accepted: bool,
    pub(super) baseline: Value,
    pub(super) statistical: samples::Report,
    pub(super) errors: Vec<String>,
    pub(super) live_errors: Vec<String>,
}
pub(super) fn baseline_catalog() -> BTreeSet<String> {
    ["provenance.json", "qualification.json"]
        .into_iter()
        .map(str::to_owned)
        .chain(["calibration", "A1", "A2"].into_iter().flat_map(|phase| {
            sampling_contract::PROFILES
                .into_iter()
                .map(move |profile| format!("{phase}/{profile}-samples.json"))
        }))
        .collect()
}
impl Execution {
    fn validate(&self) -> Result<()> {
        for pin in [
            &self.session_sha256,
            &self.samples_sha256,
            &self.renderer_sha256,
            &self.analyzer_sha256,
        ] {
            valid_digest(pin)?;
        }
        if self.schema != SCHEMA
            || self.session_sha256.bytes().all(|b| b == b'0')
            || !(1..=86400).contains(&self.stage_seconds)
            || !(1..=604800).contains(&self.window_seconds)
            || !(1..=60).contains(&self.gap_seconds)
            || self.stage_seconds > self.window_seconds
        {
            return Err("unsupported execution identity or bounded window".into());
        }
        if self
            .baseline_members
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != baseline_catalog()
        {
            return Err("complete 14-member baseline catalog is required".into());
        }
        for pin in self.baseline_members.values() {
            valid_digest(pin)?;
        }
        Ok(())
    }
}
pub(super) fn context(value: &Value, compiler: &str) -> Result<()> {
    sampling_contract::campaign::validate_context(value)?;
    let cpus: Vec<u64> = serde_json::from_value(value["isolation"]["cpu_affinity"].clone())?;
    if cpus.len() < 2
        || cpus.windows(2).any(|w| w[0] >= w[1])
        || cpus.last().is_some_and(|v| *v > 1_000_000)
        || value["isolation"]["reserved_cpu_count"].as_u64() != Some(cpus.len() as u64)
        || value["compiler"].as_str() != compiler.lines().next()
        || !value["cpu_model"].is_string()
    {
        return Err("resource CPU policy or frozen compiler mismatch".into());
    }
    Ok(())
}
fn quality(report: &Value) -> bool {
    let expected: usize = sampling_contract::case_catalog()
        .values()
        .map(Vec::len)
        .sum();
    report["phase_order"] == json!(["A1", "A2"])
        && report["case_count"].as_u64() == Some(expected as u64)
        && report["phase_quality"]
            .as_object()
            .is_some_and(|p| p.len() == 2)
        && ["A1", "A2"].iter().all(|phase| {
            let p = &report["phase_quality"][phase];
            ["wide_cases", "layout_mismatches"]
                .iter()
                .all(|key| p[key].as_array().is_some_and(Vec::is_empty))
                && p["short_samples"].as_u64() == Some(0)
        })
        && report["cases"].as_array().is_some_and(|rows| {
            rows.iter()
                .all(|r| r["repeat"]["exceeds_repeat_limit"] == false)
        })
}
fn terminal_matches(
    terminal: &Value,
    provenance: &Value,
    counts: &Value,
    report: &Value,
    pins: &BTreeMap<String, String>,
) -> Result<bool> {
    if terminal["baseline_controls_passed"] != true
        || terminal["release_approval"] != false
        || !terminal["candidate_attribution"].is_null()
        || [
            "schema_version",
            "source",
            "tool",
            "context",
            "harness_sha256",
        ]
        .iter()
        .any(|k| terminal[k] != provenance[k])
        || terminal["sampling"]["common_iterations"] != *counts
    {
        return Ok(false);
    }
    valid_digest(
        provenance["harness_sha256"]
            .as_str()
            .ok_or("missing baseline harness identity")?,
    )?;
    if provenance["sampling"]
        .as_object()
        .ok_or("missing baseline sampling protocol")?
        .iter()
        .any(|(k, v)| terminal["sampling"][k] != *v)
    {
        return Ok(false);
    }
    for profile in sampling_contract::PROFILES {
        for phase in ["calibration", "A1", "A2"] {
            let name = format!("{phase}/{profile}-samples.json");
            let entry = if phase == "calibration" {
                &terminal["calibration"][profile]
            } else {
                &terminal["phases"][phase][profile]["samples"]
            };
            if entry["path"] != name
                || entry["sha256"].as_str() != pins.get(&name).map(String::as_str)
            {
                return Ok(false);
            }
        }
    }
    let rows = terminal["repeat_checks"]
        .as_array()
        .ok_or("missing terminal baseline repeat checks")?;
    let mut repeats = BTreeMap::new();
    for row in rows {
        let key = (
            row["profile"].as_str().ok_or("missing repeat profile")?,
            row["case"].as_str().ok_or("missing repeat case")?,
        );
        if repeats.insert(key, row).is_some() {
            return Ok(false);
        }
    }
    let original = report["cases"]
        .as_array()
        .ok_or("missing recomputed baseline cases")?;
    if repeats.len() != original.len() {
        return Ok(false);
    }
    for case in original {
        let key = (
            case["profile"].as_str().ok_or("missing baseline profile")?,
            case["case"].as_str().ok_or("missing baseline case")?,
        );
        let Some(row) = repeats.get(&key) else {
            return Ok(false);
        };
        let Some(drift) = row["drift"]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0 && *n <= sampling_contract::REPEAT_DRIFT_LIMIT)
        else {
            return Ok(false);
        };
        let expected = case["repeat"]["absolute_metric_drift"]
            .as_f64()
            .ok_or("missing recomputed drift")?;
        if row["metric"] != case["metric"]
            || row["first"].as_f64() != case["phases"]["A1"]["median"].as_f64()
            || row["second"].as_f64() != case["phases"]["A2"]["median"].as_f64()
            || (drift - expected).abs() > 1e-12
        {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) struct BaselineGate {
    pub report: Value,
    pub context: Value,
    pub passed: bool,
    pub errors: Vec<String>,
}
pub(super) fn baseline_gate(
    snapshot: &mut Snapshot,
    pins: &BTreeMap<String, String>,
    renderer: &str,
    analyzer: &str,
    compiler: &str,
) -> Result<BaselineGate> {
    if pins.keys().cloned().collect::<BTreeSet<_>>() != baseline_catalog() {
        return Err("complete baseline members required".into());
    }
    let (baseline_report, provenance, counts) =
        baseline::bound(snapshot, "baseline/", Some(pins), renderer, analyzer, true)?;
    let passed = quality(&baseline_report);
    let mut errors = Vec::new();
    if provenance["schema_version"] != "baseline-qualification-v2" {
        errors.push("complete v2 baseline runtime provenance is required".into());
    }
    for role in ["source", "tool"] {
        for field in ["commit", "tree"] {
            valid_revision(
                provenance[role][field]
                    .as_str()
                    .ok_or("missing baseline source identity")?,
            )?;
        }
        valid_digest(
            provenance[role]["lock_sha256"]
                .as_str()
                .ok_or("missing baseline lockfile identity")?,
        )?;
    }
    let terminal = snapshot.json_bound(
        "baseline/qualification.json",
        Some(&pins["qualification.json"]),
    )?;
    if !terminal_matches(&terminal, &provenance, &counts, &baseline_report, pins)? {
        errors.push("baseline terminal status, identity or raw member bindings failed".into());
    }
    let reference = &provenance["context"];
    context(reference, compiler)?;
    if !passed {
        errors.push("recomputed baseline width, duration, layout or repeat gate failed".into());
    }
    Ok(BaselineGate {
        report: baseline_report,
        context: provenance["context"].clone(),
        passed,
        errors,
    })
}
fn verify(
    root: &Path,
    pin: &str,
    observer: Option<&mut dyn FnMut() -> Result<Value>>,
) -> Result<Report> {
    let mut snapshot = Snapshot::new(root)?;
    let (report, _, _) = bound(&mut snapshot, pin, observer)?;
    snapshot.finish()?;
    Ok(report)
}
pub(super) fn bound(
    snapshot: &mut Snapshot,
    pin: &str,
    mut observer: Option<&mut dyn FnMut() -> Result<Value>>,
) -> Result<(Report, Execution, samples::Binding)> {
    valid_digest(pin)?;
    let before = observer.as_mut().map(|f| f());
    let execution: Execution =
        serde_json::from_value(snapshot.json_bound("execution.json", Some(pin))?)?;
    execution.validate()?;
    let (statistical, binding) = samples::bound(snapshot, "corpus/", &execution.samples_sha256)?;
    let gate = baseline_gate(
        snapshot,
        &execution.baseline_members,
        &execution.renderer_sha256,
        &execution.analyzer_sha256,
        &binding.baseline.compiler,
    )?;
    let reference = &gate.context;
    let mut report = Report {
        schema: "plan-bench-execution-audit-v1",
        execution_sha256: pin.into(),
        bindings_verified: false,
        baseline_quality_passed: gate.passed,
        preflight_passed: false,
        capture_provenance_verified: false,
        performance_accepted: false,
        baseline: gate.report,
        statistical,
        errors: gate.errors,
        live_errors: Vec::new(),
    };
    if !report.statistical.integrity_accepted || !report.statistical.diagnostics_passed {
        report
            .errors
            .push("statistical originals failed integrity or diagnostic gates".into());
    }
    let expected = usize::from(binding.repetitions) * 4;
    if execution.attempts.len() != expected || binding.attempts.len() != expected {
        report
            .errors
            .push("complete repeated execution matrix is required".into());
    }
    let mut previous = 0;
    for (index, attempt) in execution.attempts.iter().enumerate() {
        let repetition = index / 4 + 1;
        let stage = STAGES[index % 4];
        let manifest = if index % 4 == 0 || index % 4 == 3 {
            &binding.baseline
        } else {
            &binding.candidate
        };
        // The statistical input pin owns the source manifest bytes. The stage
        // must name that same frozen identity, not a newly declared source.
        let source_pin = if index % 4 == 0 || index % 4 == 3 {
            &binding.baseline_sha256
        } else {
            &binding.candidate_sha256
        };
        if usize::from(attempt.repetition) != repetition
            || attempt.stage != stage
            || binding.attempts.get(index) != Some(&attempt.attempt_sha256)
            || &attempt.manifest_sha256 != source_pin
        {
            report.errors.push(format!(
                "{repetition}/{stage}: execution order, attempt or frozen source mismatch"
            ));
        }
        let prefix = format!("contexts/{repetition:02}/{stage}");
        let before: Observation = serde_json::from_value(snapshot.json_bound(
            &format!("{prefix}/before.json"),
            Some(&attempt.before_sha256),
        )?)?;
        let after: Observation = serde_json::from_value(
            snapshot.json_bound(&format!("{prefix}/after.json"), Some(&attempt.after_sha256))?,
        )?;
        context(&before.context, &manifest.compiler)?;
        context(&after.context, &manifest.compiler)?;
        if before.session_sha256 != execution.session_sha256
            || after.session_sha256 != execution.session_sha256
            || before.context != *reference
            || after.context != *reference
        {
            report.errors.push(format!(
                "{repetition}/{stage}: session or resource context changed"
            ));
        }
        if !(previous <= before.observed_ns
            && before.observed_ns <= attempt.started_ns
            && attempt.started_ns <= attempt.ended_ns
            && attempt.ended_ns <= after.observed_ns
            && after.observed_ns <= execution.window_seconds * SECOND)
            || before.observed_ns.saturating_sub(previous) > execution.gap_seconds * SECOND
            || after.observed_ns.saturating_sub(before.observed_ns)
                > execution.stage_seconds * SECOND
        {
            report.errors.push(format!(
                "{repetition}/{stage}: overlapping, discontinuous or expired execution window"
            ));
        }
        let worker_ns = attempt.ended_ns.saturating_sub(attempt.started_ns) as f64;
        if !binding
            .attempt_times
            .get(index)
            .copied()
            .flatten()
            .is_some_and(|total| total <= worker_ns + 1.0_f64.max(total * 1e-12))
        {
            report.errors.push(format!(
                "{repetition}/{stage}: raw sample times exceed the worker window"
            ));
        }
        previous = after.observed_ns;
    }
    if execution.ended_ns < previous
        || execution.ended_ns > execution.window_seconds * SECOND
        || execution.ended_ns.saturating_sub(previous) > execution.gap_seconds * SECOND
    {
        report
            .errors
            .push("invalid terminal execution window".into());
    }
    if let Some(before) = before {
        for observed in [before, observer.as_mut().ok_or("missing live observer")?()] {
            match observed {
                Ok(v) if v == *reference => {}
                Ok(_) => report
                    .live_errors
                    .push("live resource context differs from recorded execution".into()),
                Err(e) => report.live_errors.push(e.to_string()),
            }
        }
        if report.statistical.origin != samples::Origin::Recorded {
            report
                .live_errors
                .push("synthetic inputs cannot pass live capture preflight".into());
        }
    }
    report.bindings_verified = report.errors.is_empty();
    report.preflight_passed =
        observer.is_some() && report.bindings_verified && report.live_errors.is_empty();
    Ok((report, execution, binding))
}
pub(crate) fn run(args: &Args, live: bool) -> Result<bool> {
    let root = Path::new(args.required("--input")?);
    let pin = args.required("--execution-sha256")?;
    let mut observe = || {
        crate::control::plan_resource_context(
            &args.root(),
            Path::new(args.required("--isolation-cgroup")?),
        )
    };
    let report = verify(root, pin, if live { Some(&mut observe) } else { None })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(if live {
        report.preflight_passed
    } else {
        report.bindings_verified
    })
}
