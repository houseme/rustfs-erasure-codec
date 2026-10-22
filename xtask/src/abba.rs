//! One campaign model for extraction provenance, repeat drift and attribution.
use crate::{Args, Result};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) const STAGES: [&str; 4] = ["A1", "B1", "B2", "A2"];
type Key = (String, String, u64);

pub(crate) fn operation(group: &str) -> Option<(&'static str, &str)> {
    group
        .strip_prefix("galois_mul_slice_xor_")
        .map(|backend| ("mul_slice_xor", backend))
        .or_else(|| {
            group
                .strip_prefix("galois_mul_slice_")
                .map(|backend| ("mul_slice", backend))
        })
}

fn known_token(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
        && ![
            "unknown",
            "none",
            "null",
            "n/a",
            "na",
            "unspecified",
            "unavailable",
        ]
        .contains(&value.to_ascii_lowercase().as_str())
}

fn revision(value: &str) -> bool {
    ((4..=40).contains(&value.len()) || value.len() == 64)
        && value.bytes().all(|c| c.is_ascii_hexdigit())
        && value.bytes().any(|c| c != b'0')
}

#[derive(Debug)]
struct Estimate {
    key: Key,
    stage: usize,
    commit: String,
    backend_override: Option<String>,
    mean_ns: f64,
}

fn read_csv(reader: impl std::io::Read) -> Result<(Vec<Estimate>, Vec<String>)> {
    let mut csv = csv::ReaderBuilder::new().from_reader(reader);
    let fields = csv.headers()?.clone();
    if fields.is_empty() {
        return Err("CSV must have a header and a nonempty campaign".into());
    }
    if fields.iter().collect::<BTreeSet<_>>().len() != fields.len() {
        return Err("CSV has duplicate column names".into());
    }
    let required = [
        "stage",
        "commit",
        "criterion_group",
        "operation",
        "length_bytes",
        "mean_ns",
    ];
    let mut columns = BTreeMap::new();
    for field in required {
        columns.insert(
            field,
            fields
                .iter()
                .position(|v| v == field)
                .ok_or_else(|| format!("CSV missing required columns: {field}"))?,
        );
    }
    let override_column = fields.iter().position(|v| v == "backend_override");
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut count = 0;
    for row in csv.records() {
        count += 1;
        let row = match row {
            Ok(row) => row,
            Err(error) => {
                errors.push(format!("cannot read CSV: {error}"));
                continue;
            }
        };
        let get = |field| row.get(columns[field]).unwrap_or("").trim();
        let mut problems = Vec::new();
        let stage = STAGES.iter().position(|value| *value == get("stage"));
        if stage.is_none() {
            problems.push("unknown stage; expected A1/B1/B2/A2");
        }
        let commit = get("commit").to_ascii_lowercase();
        if !revision(&commit) {
            problems.push(
                "commit must be a resolved nonzero hexadecimal revision (4..40 or 64 digits)",
            );
        }
        let group = get("criterion_group");
        let (expected_operation, backend) = operation(group).unwrap_or(("", ""));
        if !known_token(backend) {
            problems
                .push("criterion_group must contain a valid galois operation and backend label");
        } else if expected_operation != get("operation") {
            problems.push("operation does not match criterion_group provenance");
        }
        let mut backend_override =
            override_column.map(|index| row.get(index).unwrap_or("").trim().to_owned());
        if backend_override
            .as_ref()
            .is_some_and(|value| !known_token(value))
        {
            problems.push("backend_override column must be nonempty and known on every row");
        }
        if let Some((_, embedded)) = backend.rsplit_once("_override_") {
            if backend.matches("_override_").count() != 1 || !known_token(embedded) {
                problems.push("criterion_group has an invalid embedded backend override");
            } else if backend_override
                .as_ref()
                .is_some_and(|value| value != embedded)
            {
                problems.push("backend_override conflicts with criterion_group provenance");
            } else if backend_override.is_none() {
                backend_override = Some(embedded.to_owned());
            }
        }
        let length = get("length_bytes").parse::<u64>().ok().filter(|v| *v > 0);
        if length.is_none() {
            problems.push("length_bytes must be a positive integer");
        }
        let mean = get("mean_ns")
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0);
        if mean.is_none() {
            problems.push("mean_ns must be finite and positive");
        }
        if problems.is_empty() {
            rows.push(Estimate {
                key: (
                    group.to_owned(),
                    get("operation").to_owned(),
                    length.unwrap(),
                ),
                stage: stage.unwrap(),
                commit,
                backend_override,
                mean_ns: mean.unwrap(),
            });
        } else {
            errors.extend(
                problems
                    .into_iter()
                    .map(|problem| format!("CSV row {}: {problem}", count + 1)),
            );
        }
    }
    if count == 0 {
        errors.push("CSV campaign must be nonempty".into());
    }
    Ok((rows, errors))
}

#[derive(Serialize)]
struct Case {
    criterion_group: String,
    operation: String,
    length_bytes: u64,
    a1_mean_ns: f64,
    a2_mean_ns: f64,
    a_mean_ns: f64,
    b1_mean_ns: f64,
    b2_mean_ns: f64,
    b_mean_ns: f64,
    baseline_drift: Option<f64>,
    candidate_drift: Option<f64>,
    candidate_delta_vs_a_mean: Option<f64>,
    drift_failed: bool,
}

#[derive(Serialize)]
struct Report {
    drift_threshold: Option<f64>,
    drift_failed: bool,
    drift_failures: usize,
    validation_errors: Vec<String>,
    attribution_allowed: bool,
    commits: BTreeMap<String, String>,
    criterion_groups: BTreeSet<String>,
    cases: Vec<Case>,
}

fn ratio(later: f64, earlier: f64) -> Option<f64> {
    let ratio = (later - earlier) / earlier;
    ratio.is_finite().then_some(ratio)
}

fn analyze(input: Result<(Vec<Estimate>, Vec<String>)>, threshold: &str) -> Report {
    let (rows, mut errors) = input.unwrap_or_else(|error| (Vec::new(), vec![error.to_string()]));
    let threshold = threshold
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0);
    if threshold.is_none() {
        errors.push("drift threshold must be finite and nonnegative".into());
    }
    let mut grouped: BTreeMap<Key, [Option<f64>; 4]> = BTreeMap::new();
    let mut revisions: [BTreeSet<String>; 4] = Default::default();
    let mut overrides: BTreeMap<&str, BTreeSet<Option<&str>>> = BTreeMap::new();
    for row in &rows {
        if grouped.entry(row.key.clone()).or_default()[row.stage]
            .replace(row.mean_ns)
            .is_some()
        {
            errors.push(format!(
                "duplicate estimate for {:?}, stage {}",
                row.key, STAGES[row.stage]
            ));
        }
        revisions[row.stage].insert(row.commit.clone());
        overrides
            .entry(&row.key.0)
            .or_default()
            .insert(row.backend_override.as_deref());
    }
    for (key, stages) in &grouped {
        let missing: Vec<_> = STAGES
            .iter()
            .zip(stages)
            .filter(|(_, v)| v.is_none())
            .map(|(s, _)| *s)
            .collect();
        if !missing.is_empty() {
            errors.push(format!(
                "incomplete ABBA case {key:?}: missing {}",
                missing.join(", ")
            ));
        }
    }
    let mut commits = BTreeMap::new();
    for (stage, revisions) in STAGES.iter().zip(revisions) {
        if revisions.len() > 1 {
            errors.push(format!("stage {stage} contains inconsistent revisions"));
        } else if let Some(commit) = revisions.into_iter().next() {
            commits.insert((*stage).into(), commit);
        }
    }
    for (a, b) in [("A1", "A2"), ("B1", "B2")] {
        if commits.contains_key(a) && commits.contains_key(b) && commits[a] != commits[b] {
            errors.push(format!("revision mismatch: {a} must equal {b}"));
        }
    }
    for (group, values) in overrides {
        if values.len() > 1 {
            errors.push(format!(
                "inconsistent backend_override provenance for {group}"
            ));
        }
    }
    let mut cases = Vec::new();
    if errors.is_empty() {
        for ((criterion_group, operation, length_bytes), values) in grouped {
            let [a1, b1, b2, a2] = values.map(Option::unwrap);
            // Divide first: means of finite positive samples cannot overflow.
            let (a, b) = (a1 / 2.0 + a2 / 2.0, b1 / 2.0 + b2 / 2.0);
            let (baseline_drift, candidate_drift, delta) =
                (ratio(a2, a1), ratio(b2, b1), ratio(b, a));
            let invalid = [baseline_drift, candidate_drift, delta]
                .iter()
                .any(Option::is_none);
            if invalid {
                errors.push(format!(
                    "{criterion_group}:{operation}:{length_bytes}: derived ratio is not finite"
                ));
            }
            let drift_failed = invalid
                || baseline_drift.unwrap().abs() > threshold.unwrap()
                || candidate_drift.unwrap().abs() > threshold.unwrap();
            cases.push(Case {
                criterion_group,
                operation,
                length_bytes,
                a1_mean_ns: a1,
                a2_mean_ns: a2,
                a_mean_ns: a,
                b1_mean_ns: b1,
                b2_mean_ns: b2,
                b_mean_ns: b,
                baseline_drift,
                candidate_drift,
                candidate_delta_vs_a_mean: delta,
                drift_failed,
            });
        }
    }
    let drift_failures = cases.iter().filter(|case| case.drift_failed).count();
    let attribution_allowed = !cases.is_empty() && errors.is_empty() && drift_failures == 0;
    if !attribution_allowed {
        for case in &mut cases {
            case.candidate_delta_vs_a_mean = None;
        }
    }
    Report {
        drift_threshold: threshold,
        drift_failed: !attribution_allowed,
        drift_failures,
        validation_errors: errors,
        attribution_allowed,
        commits,
        criterion_groups: rows.into_iter().map(|row| row.key.0).collect(),
        cases,
    }
}

fn percentage(value: Option<f64>) -> String {
    value.map_or_else(
        || "not attributable".into(),
        |v| format!("{:.2}%", v * 100.0),
    )
}

fn markdown(report: &Report) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "# Galois Backend ABBA Summary\n\n- Drift threshold: {}\n- Input gate: {}\n- Drift gate: {}\n- Candidate attribution: {}\n- Commits: A={} / B={}\n\n",
        report
            .drift_threshold
            .map_or_else(|| "invalid".into(), |v| percentage(Some(v))),
        if report.validation_errors.is_empty() {
            "PASS"
        } else {
            "FAIL"
        },
        if report.drift_failed { "FAIL" } else { "PASS" },
        if report.attribution_allowed {
            "PERMITTED"
        } else {
            "WITHHELD for the entire campaign"
        },
        report.commits.get("A1").map_or("unknown", String::as_str),
        report.commits.get("B1").map_or("unknown", String::as_str)
    );
    if !report.validation_errors.is_empty() {
        text.push_str("Validation errors:\n\n");
        for error in &report.validation_errors {
            writeln!(text, "- {error}").unwrap();
        }
        text.push('\n');
    }
    text.push_str("| Criterion group | Operation | Length | A1 mean ns | A2 mean ns | A drift | B1 mean ns | B2 mean ns | B drift | B vs A mean | Gate |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|\n");
    for c in &report.cases {
        writeln!(
            text,
            "| {} | {} | {} | {:.2} | {:.2} | {} | {:.2} | {:.2} | {} | {} | {} |",
            c.criterion_group,
            c.operation,
            c.length_bytes,
            c.a1_mean_ns,
            c.a2_mean_ns,
            percentage(c.baseline_drift),
            c.b1_mean_ns,
            c.b2_mean_ns,
            percentage(c.candidate_drift),
            percentage(c.candidate_delta_vs_a_mean),
            if c.drift_failed { "FAIL" } else { "PASS" }
        )
        .unwrap();
    }
    text
}

pub(crate) fn run(args: &Args) -> Result<bool> {
    // Always replace stale PASS reports, even for unreadable/malformed input.
    let input = std::fs::File::open(args.required("--csv")?)
        .map_err(Into::into)
        .and_then(read_csv);
    let report = analyze(input, args.get("--drift-threshold", "0.05"));
    let markdown = markdown(&report);
    std::fs::write(
        Path::new(args.required("--summary-json")?),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    std::fs::write(Path::new(args.required("--summary-md")?), &markdown)?;
    print!("{markdown}");
    if !report.attribution_allowed {
        eprintln!(
            "ABBA campaign failed validation or drift checks; candidate attribution withheld"
        );
    }
    Ok(report.attribution_allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn campaign(values: [f64; 4]) -> String {
        let mut text = String::from(
            "stage,commit,backend_override,criterion_group,operation,length_bytes,mean_ns\n",
        );
        for (stage, value) in STAGES.iter().zip(values) {
            let commit = if stage.starts_with('A') {
                "aaaa111"
            } else {
                "bbbb222"
            };
            text += &format!(
                "{stage},{commit},auto,galois_mul_slice_neon_override_auto,mul_slice,65536,{value}\n"
            );
        }
        text
    }
    fn report(csv: &str, threshold: &str) -> Report {
        analyze(read_csv(csv.as_bytes()), threshold)
    }

    #[test]
    fn exact_boundary_and_no_candidate_regression_limit() {
        for values in [[100., 100., 95., 105.], [100., 150., 150., 100.]] {
            assert!(report(&campaign(values), "0.05").attribution_allowed);
        }
        assert!(report(&campaign([100., 100., 110., 110.]), "0.1").attribution_allowed);
    }

    #[test]
    fn either_repeat_failure_withholds_every_delta() {
        for values in [
            [100., 80., 80., 106.],
            [100., 80., 85., 100.],
            [100., 80., 74., 100.],
        ] {
            let r = report(&campaign(values), "0.05");
            assert!(!r.attribution_allowed);
            assert_eq!(r.drift_failures, 1);
            assert!(
                r.cases
                    .iter()
                    .all(|c| c.candidate_delta_vs_a_mean.is_none())
            );
            assert!(markdown(&r).contains("WITHHELD for the entire campaign"));
        }
    }

    #[test]
    fn malformed_incomplete_duplicate_and_conflicting_campaigns_fail() {
        let good = campaign([100., 90., 90., 100.]);
        for bad in [
            String::new(),
            good.lines().take(4).collect::<Vec<_>>().join("\n"),
            good.clone() + good.lines().nth(1).unwrap(),
            good.replace("A2,aaaa111", "A2,cccc111"),
            good.replace("B1,bbbb222,auto", "B1,bbbb222,scalar"),
            good.replace("B2,bbbb222", "XX,bbbb222"),
            good.replace("65536,90", "65536,NaN"),
            good.replace("mul_slice,65536", "mul_slice_xor,65536"),
        ] {
            let r = report(&bad, "0.05");
            assert!(!r.attribution_allowed, "{bad}");
            assert!(!r.validation_errors.is_empty());
        }
    }

    #[test]
    fn legacy_override_and_extreme_numbers() {
        let legacy = campaign([100., 90., 90., 100.])
            .replace(",backend_override", "")
            .replace(",auto,", ",");
        assert!(report(&legacy, "0.05").attribution_allowed);
        for threshold in ["nan", "inf", "-inf", "-0.01", "bad"] {
            assert!(!report(&legacy, threshold).attribution_allowed);
        }
        for values in [[1e-308, 1e308, 1e308, 1e-308], [1e-308, 10., 10., 1e308]] {
            let r = report(&campaign(values), "0.05");
            assert!(!r.attribution_allowed);
            assert!(
                r.validation_errors
                    .iter()
                    .any(|e| e.contains("derived ratio is not finite"))
            );
            serde_json::to_string(&r).unwrap();
        }
    }
}
