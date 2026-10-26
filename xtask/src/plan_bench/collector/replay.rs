use super::*;
pub(super) fn verify(root: &Path, plan_pin: &str, receipt_pin: &str) -> Result<Check> {
    valid_digest(plan_pin)?;
    valid_digest(receipt_pin)?;
    let mut snapshot = Snapshot::with_budget(root, 2 * 1024 * 1024 * 1024)?;
    let plan: Plan = serde_json::from_value(snapshot.json_bound("plan.json", Some(plan_pin))?)?;
    plan.validate()?;
    let receipt: Receipt =
        serde_json::from_value(snapshot.json_bound("collection.receipt.json", Some(receipt_pin))?)?;
    valid_digest(&receipt.controller_sha256)?;
    if receipt.schema != SCHEMA || receipt.plan_sha256 != plan_pin {
        return Err("collection source identity mismatch".into());
    }
    let mut check = Check {
        accepted: false,
        observed_steps: receipt.steps.len(),
        execution: None,
        errors: Vec::new(),
        capture_provenance_verified: false,
        performance_accepted: false,
    };
    if !receipt.completed || receipt.error.is_some() {
        check.errors.push(
            "collection failed or was refused; retained partial artifacts cannot qualify".into(),
        );
    }
    let expected = usize::from(plan.repetitions) * 4;
    if receipt.steps.len() != expected {
        check
            .errors
            .push("collection stopped before all planned workers".into());
    }
    let analyzed = match receipt.execution_sha256.as_deref() {
        Some(pin) => match snapshot.scoped(256 * 1024 * 1024, |snapshot| {
            execution::bound(snapshot, pin, None)
        }) {
            Ok(value) => Some(value),
            Err(e) => {
                check.errors.push(e.to_string());
                None
            }
        },
        None => {
            check
                .errors
                .push("collection has no complete execution envelope".into());
            None
        }
    };
    if let Some((report, execution, binding)) = &analyzed
        && (!report.bindings_verified
            || !report
                .statistical
                .matches_configuration(&plan.document(Vec::new()))
            || binding.baseline_sha256 != plan.baseline_sha256
            || binding.candidate_sha256 != plan.candidate_sha256
            || execution.session_sha256 != plan.session_sha256
            || execution.baseline_members != plan.baseline_members
            || execution.renderer_sha256 != plan.renderer_sha256
            || execution.analyzer_sha256 != plan.analyzer_sha256
            || execution.stage_seconds != plan.stage_seconds
            || execution.window_seconds != plan.window_seconds
            || execution.gap_seconds != plan.gap_seconds
            || receipt.samples_sha256.as_deref() != Some(execution.samples_sha256.as_str()))
    {
        check
            .errors
            .push("collected execution or statistics differ from the predeclared plan".into());
    }
    for (index, step) in receipt.steps.iter().enumerate() {
        let repetition = index / 4 + 1;
        let stage = STAGES[index % 4];
        let prefix = format!("workers/{repetition:02}/{stage}");
        if usize::from(step.repetition) != repetition
            || step.stage != stage
            || step.arguments != plan.args()
        {
            check
                .errors
                .push("worker order or fixed arguments mismatch".into());
        }
        snapshot.binary_bound(&format!("{prefix}/binary"), &step.binary_sha256)?;
        let start =
            snapshot.json_bound(&format!("{prefix}/start.json"), Some(&step.start_sha256))?;
        if start["plan_sha256"] != plan_pin
            || start["repetition"] != repetition
            || start["stage"] != stage
            || start["binary_sha256"] != step.binary_sha256
            || start["arguments"] != serde_json::to_value(plan.args())?
        {
            check
                .errors
                .push("worker start journal differs from the plan".into());
        }
        let outcome = match step.outcome_sha256.as_deref() {
            Some(pin) => Some(serde_json::from_value::<Outcome>(
                snapshot.json_bound(&format!("{prefix}/outcome.json"), Some(pin))?,
            )?),
            None => {
                check.errors.push("missing owned worker outcome".into());
                None
            }
        };
        if !outcome
            .as_ref()
            .is_some_and(|v| v.exit_code == Some(0) && v.reason == "exited" && v.reaped)
        {
            check
                .errors
                .push("worker failed, cancelled, exceeded a bound or was not reaped".into());
        }
        for (name, pin) in [
            ("stdout", &step.stdout_sha256),
            ("stderr", &step.stderr_sha256),
        ] {
            match pin {
                Some(pin) => {
                    snapshot.raw_bound(&format!("{prefix}/worker.{name}"), pin)?;
                }
                None => check
                    .errors
                    .push("worker logs exceed original-member acceptance or are incomplete".into()),
            }
        }
        if let Some((report, execution, binding)) = &analyzed {
            let source = if stage.starts_with('A') {
                &binding.baseline
            } else {
                &binding.candidate
            };
            if step.binary_sha256 != source.binary_sha256 {
                check
                    .errors
                    .push("worker binary differs from the frozen source".into());
            }
            if let Some(actual) = execution.attempts.get(index) {
                let expected = json!({"repetition":actual.repetition,"stage":actual.stage,"manifest_sha256":actual.manifest_sha256,"attempt_sha256":actual.attempt_sha256,"before_sha256":actual.before_sha256,"after_sha256":actual.after_sha256,"started_ns":actual.started_ns,"ended_ns":actual.ended_ns});
                if step.execution.as_ref() != Some(&expected)
                    || start["before_sha256"] != actual.before_sha256
                    || start["started_ns"] != actual.started_ns
                {
                    check.errors.push(
                        "owned execution journal differs from the statistical attempt".into(),
                    );
                }
                if let (Some(outcome), Some(attempt)) =
                    (&outcome, report.statistical.attempts.get(index))
                    && serde_json::to_value(outcome)? != serde_json::to_value(&attempt.outcome)?
                {
                    check
                        .errors
                        .push("statistical outcome differs from the owned process outcome".into());
                }
            } else {
                check.errors.push("missing collected execution step".into());
            }
            let canonical_prefix = format!("corpus/records/{repetition:02}/{stage}/");
            let mut seen = BTreeSet::new();
            for copy in &step.copies {
                let tail = copy
                    .canonical
                    .strip_prefix(&canonical_prefix)
                    .ok_or("artifact escaped its canonical stage")?;
                let (index, name) = tail.split_once('/').ok_or("invalid canonical artifact")?;
                let ordinal: usize = index.parse()?;
                if index != format!("{ordinal:04}")
                    || ordinal >= source.catalog.len()
                    || !["benchmark.json", "sample.json", "estimates.json"].contains(&name)
                    || !copy.original.starts_with(&format!("{prefix}/raw/"))
                    || !seen.insert(copy.canonical.as_str())
                {
                    return Err("invalid or duplicate original artifact binding".into());
                }
                if snapshot.member_digest(&copy.canonical) != Some(copy.sha256.as_str()) {
                    check
                        .errors
                        .push("canonical artifact differs from its worker original".into());
                }
                snapshot.binary_bound(&copy.original, &copy.sha256)?;
            }
            if seen.len() != source.catalog.len() * 3 {
                check
                    .errors
                    .push("complete raw-to-canonical artifact bindings required".into());
            }
        }
    }
    snapshot.finish()?;
    if let Some((report, _, _)) = analyzed {
        check.execution = Some(report);
    }
    check.accepted = check.errors.is_empty();
    Ok(check)
}
pub(crate) fn replay(args: &Args) -> Result<bool> {
    let check = verify(
        Path::new(args.required("--input")?),
        args.required("--plan-sha256")?,
        args.required("--receipt-sha256")?,
    )?;
    println!("{}", serde_json::to_string_pretty(&check)?);
    Ok(check.accepted)
}
