//! Supervised, fixed-argument collection. Synthetic plans never run Criterion timing.
use super::*;
use crate::{abba::STAGES, sampling_contract};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
mod artifacts;
mod model;
mod replay;
#[cfg(all(test, unix))]
pub(crate) mod tests;
use model::{ArtifactCopy, Check, Plan, Receipt, SCHEMA, Step};
pub(crate) use replay::replay;
fn save(path: &Path, value: &impl Serialize) -> Result<String> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    write(path, &bytes)?;
    Ok(digest(&bytes))
}
fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T> {
    let artifact::StrictJson(v) = serde_json::from_slice(bytes)?;
    Ok(serde_json::from_value(v)?)
}
fn elapsed(origin: Instant) -> Result<u64> {
    Ok(origin.elapsed().as_nanos().try_into()?)
}
fn copied(source: &mut Snapshot, output: &Path, name: &str, pin: &str) -> Result<Vec<u8>> {
    let bytes = source.binary_bound(name, pin)?;
    if let Some(p) = output.join(name).parent() {
        fs::create_dir_all(p)?;
    }
    write(&output.join(name), &bytes)?;
    Ok(bytes)
}
fn prepare(source: &Path, output: &Path, pin: &str) -> Result<Plan> {
    let mut originals = Snapshot::new(source)?;
    let bytes = copied(&mut originals, output, "plan.json", pin)?;
    let plan: Plan = decode(&bytes)?;
    plan.validate()?;
    for (label, pin) in [("A", &plan.baseline_sha256), ("B", &plan.candidate_sha256)] {
        let prefix = format!("corpus/sources/{label}/");
        let manifest: Manifest = decode(&copied(
            &mut originals,
            output,
            &format!("{prefix}manifest.json"),
            pin,
        )?)?;
        manifest.validate()?;
        copied(
            &mut originals,
            output,
            &format!("{prefix}source.tar.gz"),
            &manifest.archive_sha256,
        )?;
        copied(
            &mut originals,
            output,
            &format!("{prefix}binary"),
            &manifest.binary_sha256,
        )?;
    }
    for name in execution::baseline_catalog() {
        copied(
            &mut originals,
            output,
            &format!("baseline/{name}"),
            &plan.baseline_members[&name],
        )?;
    }
    originals.finish()?;
    Ok(plan)
}
fn protected(root: &Path, plan: &Plan, pin: &str) -> Result<(Snapshot, Manifest, Manifest, Value)> {
    let mut snapshot = Snapshot::new(root)?;
    snapshot.binary_bound("plan.json", pin)?;
    let a = frozen_snapshot(&mut snapshot, "corpus/sources/A/", &plan.baseline_sha256)?;
    let b = frozen_snapshot(&mut snapshot, "corpus/sources/B/", &plan.candidate_sha256)?;
    compatible(&a, &b)?;
    let gate = execution::baseline_gate(
        &mut snapshot,
        &plan.baseline_members,
        &plan.renderer_sha256,
        &plan.analyzer_sha256,
        &a.compiler,
    )?;
    if !gate.errors.is_empty() || !gate.passed {
        return Err(format!("baseline preflight refused: {}", gate.errors.join("; ")).into());
    }
    Ok((snapshot, a, b, gate.context))
}
fn command(directory: &Path, plan: &Plan, reference: &Value) -> Command {
    let mut command = Command::new(directory.join("binary"));
    command
        .current_dir(directory)
        .args(plan.args())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NO_COLOR", "1")
        .env("CRITERION_HOME", directory.join("raw"))
        .stdin(std::process::Stdio::null());
    if let Some(settings) = sampling_contract::protocol::sampling_environment().as_object() {
        for (k, v) in settings {
            if let Some(v) = v.as_str() {
                command.env(k, v);
            }
        }
    }
    if let Some(settings) = reference["runtime"]["allocator_environment"].as_object() {
        for (k, v) in settings {
            if let Some(v) = v.as_str() {
                command.env(k, v);
            }
        }
    }
    command
}
fn observe(
    plan: &Plan,
    reference: &Value,
    observer: &mut dyn FnMut() -> Result<Value>,
) -> Result<Value> {
    let actual = if plan.origin == samples::Origin::Synthetic {
        reference.clone()
    } else {
        observer()?
    };
    if actual != *reference {
        return Err("live resource context changed; refuse collection".into());
    }
    Ok(actual)
}
fn collect(
    root: &Path,
    pin: &str,
    plan: &Plan,
    receipt: &mut Receipt,
    cancelled: &AtomicBool,
    observer: &mut dyn FnMut() -> Result<Value>,
) -> Result<()> {
    let (inputs, a, b, reference) = protected(root, plan, pin)?;
    observe(plan, &reference, observer)?;
    let origin = Instant::now();
    let mut attempts = Vec::new();
    let mut means = vec![vec![[None; 4]; a.catalog.len()]; usize::from(plan.repetitions)];
    let mut executed = Vec::new();
    let mut previous = 0;
    let result = (|| -> Result<()> {
        for index in 0..usize::from(plan.repetitions) * 4 {
            if cancelled.load(Ordering::Relaxed) {
                return Err("collection cancelled before next worker".into());
            }
            inputs.unchanged()?;
            let repetition = index / 4 + 1;
            let stage = STAGES[index % 4];
            let label = if stage.starts_with('A') { "A" } else { "B" };
            let manifest = if label == "A" { &a } else { &b };
            let directory = root.join(format!("workers/{repetition:02}/{stage}"));
            fs::create_dir_all(directory.parent().ok_or("missing worker parent")?)?;
            new_directory(&directory)?;
            new_directory(&directory.join("raw"))?;
            let frozen = load(
                &root.join(format!("corpus/sources/{label}")),
                plan.pin(index),
            )?;
            write(&directory.join("binary"), &frozen.binary)?;
            executable(&directory.join("binary"))?;
            frozen.snapshot.finish()?;
            let before_context = observe(plan, &reference, observer)?;
            let before_ns = elapsed(origin)?;
            if before_ns.saturating_sub(previous) > plan.gap_seconds * 1_000_000_000
                || before_ns >= plan.window_seconds * 1_000_000_000
            {
                return Err("collection gap or total window expired".into());
            }
            let context_prefix = root.join(format!("contexts/{repetition:02}/{stage}"));
            let before_pin = save(
                &context_prefix.join("before.json"),
                &json!({"session_sha256":plan.session_sha256,"observed_ns":before_ns,"context":before_context}),
            )?;
            let started_ns = elapsed(origin)?;
            let start_pin = save(
                &directory.join("start.json"),
                &json!({"plan_sha256":pin,"repetition":repetition,"stage":stage,"binary_sha256":manifest.binary_sha256,"arguments":plan.args(),"before_sha256":before_pin,"started_ns":started_ns}),
            )?;
            receipt.steps.push(Step {
                repetition: repetition as u8,
                stage: stage.into(),
                binary_sha256: manifest.binary_sha256.clone(),
                arguments: plan.args(),
                start_sha256: start_pin,
                outcome_sha256: None,
                stdout_sha256: None,
                stderr_sha256: None,
                execution: None,
                copies: Vec::new(),
            });
            let timeout = (plan.stage_seconds * 1_000_000_000)
                .min((plan.window_seconds * 1_000_000_000).saturating_sub(started_ns));
            if timeout == 0 {
                return Err("collection total deadline expired before spawn".into());
            }
            let mut last_check = Instant::now();
            let mut guard_error = None;
            let outcome = run::execute_guarded(
                command(&directory, plan, &reference),
                &directory,
                "worker",
                Duration::from_nanos(timeout),
                cancelled,
                |force| {
                    if force || last_check.elapsed() >= Duration::from_secs(1) {
                        last_check = Instant::now();
                        if let Err(e) = artifacts::tree(
                            &root.join("workers"),
                            usize::from(plan.repetitions) * 4 * (a.catalog.len() * 64 + 128),
                        ) {
                            guard_error = Some(e.to_string());
                            return Err(e);
                        }
                    }
                    Ok(())
                },
            )?;
            let ended_ns = elapsed(origin)?;
            let step = receipt
                .steps
                .last_mut()
                .ok_or("missing owned worker record")?;
            step.outcome_sha256 = Some(save(&directory.join("outcome.json"), &outcome)?);
            if !outcome.reaped {
                return Err("owned worker was not reaped; preserve inputs without inspecting live artifacts".into());
            }
            for (name, slot) in [
                ("stdout", &mut step.stdout_sha256),
                ("stderr", &mut step.stderr_sha256),
            ] {
                if let Ok(bytes) = bounded_log(&directory.join(format!("worker.{name}"))) {
                    *slot = Some(digest(&bytes));
                }
            }
            let produced = artifacts::collect(
                root,
                &directory,
                &root.join(format!("corpus/records/{repetition:02}/{stage}")),
                manifest,
                plan,
            );
            let case_error = match &produced {
                Ok(p) => p.error.clone(),
                Err(e) => Some(e.to_string()),
            };
            let valid = produced.as_ref().is_ok_and(|p| p.valid);
            let (cases, case_means) = if let Ok(produced) = produced {
                step.copies = produced.copies;
                (produced.cases, produced.means)
            } else {
                (Vec::new(), Vec::new())
            };
            let unchanged = inputs.unchanged().is_ok()
                && crate::fingerprint::file(&directory.join("binary"))
                    .is_ok_and(|hash| hash == manifest.binary_sha256);
            let attempt = json!({"repetition":repetition,"stage":stage,"manifest_sha256":plan.pin(index),"outcome":outcome,"originals_unchanged":unchanged,"cases":cases});
            let attempt_pin = digest(&serde_json::to_vec(&attempt)?);
            attempts.push(attempt);
            let after_context = observe(plan, &reference, observer)?;
            let after_pin = save(
                &context_prefix.join("after.json"),
                &json!({"session_sha256":plan.session_sha256,"observed_ns":elapsed(origin)?,"context":after_context}),
            )?;
            let execution = json!({"repetition":repetition,"stage":stage,"manifest_sha256":plan.pin(index),"attempt_sha256":attempt_pin,"before_sha256":before_pin,"after_sha256":after_pin,"started_ns":started_ns,"ended_ns":ended_ns});
            step.execution = Some(execution.clone());
            executed.push(execution);
            previous = elapsed(origin)?;
            if outcome.exit_code != Some(0)
                || outcome.reason != "exited"
                || !outcome.reaped
                || !valid
                || !unchanged
                || case_error.is_some()
                || guard_error.is_some()
            {
                return Err(format!(
                    "{stage} failed; artifacts: {case_error:?}; guard: {guard_error:?}"
                )
                .into());
            }
            for (case, mean) in case_means.into_iter().enumerate() {
                means[index / 4][case][index % 4] = mean;
                for value in samples::drift_values(means[index / 4][case], means[0][case])
                    .into_iter()
                    .flatten()
                {
                    let drift =
                        value.map_err(|e| format!("{stage}/{}: {e}", manifest.catalog[case]))?;
                    if drift.abs() > plan.max_drift {
                        return Err(format!(
                            "{stage}/{}: observed statistical drift exceeds the predeclared limit",
                            manifest.catalog[case]
                        )
                        .into());
                    }
                }
            }
            inputs.unchanged()?;
        }
        Ok(())
    })();
    receipt.samples_sha256 = Some(save(
        &root.join("corpus/samples.json"),
        &plan.document(attempts),
    )?);
    receipt.execution_sha256 = Some(save(
        &root.join("execution.json"),
        &json!({"schema":"plan-bench-execution-inputs-v1","session_sha256":plan.session_sha256,"samples_sha256":receipt.samples_sha256,"baseline_members":plan.baseline_members,"renderer_sha256":plan.renderer_sha256,"analyzer_sha256":plan.analyzer_sha256,"stage_seconds":plan.stage_seconds,"window_seconds":plan.window_seconds,"gap_seconds":plan.gap_seconds,"ended_ns":elapsed(origin)?,"attempts":executed}),
    )?);
    result?;
    inputs.finish()?;
    Ok(())
}
fn capture_owned(
    source: &Path,
    root: &Path,
    pin: &str,
    cancelled: &AtomicBool,
    observer: &mut dyn FnMut() -> Result<Value>,
) -> Result<bool> {
    valid_digest(pin)?;
    new_directory(root)?;
    let root = fs::canonicalize(root)?;
    let mut receipt = Receipt {
        schema: SCHEMA.into(),
        plan_sha256: pin.into(),
        controller_sha256: env!("RSE_NATIVE_SOURCE_SHA256").into(),
        completed: false,
        execution_sha256: None,
        samples_sha256: None,
        steps: Vec::new(),
        error: None,
    };
    let result = (|| {
        let plan = prepare(source, &root, pin)?;
        collect(&root, pin, &plan, &mut receipt, cancelled, observer)
    })();
    if let Err(e) = result {
        receipt.error = Some(e.to_string());
    } else {
        receipt.completed = true;
    }
    let receipt_pin = save(&root.join("collection.receipt.json"), &receipt)?;
    println!("collection receipt sha256: {receipt_pin}");
    let check = replay::verify(&root, pin, &receipt_pin)?;
    save(&root.join("collection.replay.json"), &check)?;
    println!(
        "{}",
        json!({"accepted":check.accepted,"observed_steps":check.observed_steps,"error":receipt.error,"validation_errors":check.errors,"capture_provenance_verified":false,"performance_accepted":false})
    );
    Ok(check.accepted)
}
pub(crate) fn capture(args: &Args) -> Result<bool> {
    if !cfg!(unix) {
        return Err("supervised plan collection requires Unix".into());
    }
    crate::control::source_current()?;
    let mut observer = || {
        crate::control::plan_resource_context(
            &args.root(),
            Path::new(args.required("--isolation-cgroup")?),
        )
    };
    capture_owned(
        Path::new(args.required("--input")?),
        Path::new(args.required("--output")?),
        args.required("--plan-sha256")?,
        run::cancellation()?,
        &mut observer,
    )
}
