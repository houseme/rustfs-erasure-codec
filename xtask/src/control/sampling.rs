use super::{
    environment as env,
    io::{Context, Environment, Temporary},
    overlay::Overlay,
    *,
};
use crate::sampling_contract;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::Command,
};
pub(super) const TEST_ARGS: &[&str] = &[
    "test",
    "--locked",
    "--release",
    "--features",
    "std simd-accel",
    "--test",
    "rse_release_sampling",
];
pub(super) fn render(
    root: &Path,
    variant: Option<&str>,
    allocation: bool,
) -> Result<BTreeMap<String, String>> {
    match variant {
        None => crate::diagnostics::sampler::render(root, tool()),
        Some(variant) => crate::diagnostics::overlays::render(root, tool(), variant, allocation),
    }
}
pub(super) fn overlay_digest(root: &Path, variant: Option<&str>) -> Result<String> {
    crate::fingerprint::overlay(&render(root, variant, false)?, root)
}
pub(super) fn command(root: &Path, environment: &Environment, extra: &[&str]) -> Result<Command> {
    let mut command = io::command(root, &env::build(root, environment)?, "cargo", TEST_ARGS);
    command.args(extra);
    Ok(command)
}
pub(super) fn parsed(text: &str, prefix: &str) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    sampling_contract::output::visit(text, &[prefix], |_, value| {
        rows.push(value);
        Ok(())
    })?;
    Ok(rows)
}
pub(super) fn catalog(profile: &str) -> Result<BTreeSet<String>> {
    sampling_contract::case_catalog()
        .remove(profile)
        .map(|v| v.into_iter().collect())
        .ok_or_else(|| "unknown sampling profile".into())
}
pub(super) struct Journal<'a> {
    pub stream: &'a mut File,
    pub phase: &'a str,
}
pub(super) struct Run<'a> {
    pub root: &'a Path,
    pub environment: &'a Environment,
    pub profile: &'a str,
    pub mode: &'a str,
    pub log: &'a Path,
    pub cases: &'a BTreeSet<String>,
    pub counts: Option<&'a BTreeMap<String, u64>>,
    pub journal: Option<Journal<'a>>,
}
pub(super) fn run_profile(context: &Context<'_>, mut run: Run<'_>) -> Result<Vec<Value>> {
    if !["calibrate", "sample"].contains(&run.mode) || run.cases.is_empty() {
        return Err("a valid mode and nonempty unique case catalog are required".into());
    }
    if run.mode == "sample"
        && run
            .counts
            .is_none_or(|c| c.keys().cloned().collect::<BTreeSet<_>>() != *run.cases)
    {
        return Err("complete frozen case counts are required".into());
    }
    let mut environment = env::worker(&env::build(run.root, run.environment)?)?;
    environment.extend([
        ("RSE_SAMPLER_PROFILE".into(), run.profile.into()),
        ("RSE_SAMPLER_MODE".into(), run.mode.into()),
        (
            "RSE_SAMPLER_PAGE_SIZE".into(),
            env::page_size()?.to_string(),
        ),
    ]);
    // One append-only worker log; each calibration parses only its newly appended bytes.
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(run.log)?;
    let mut records = Vec::new();
    for case in run.cases {
        let (mut count, mut batch) = if run.mode == "calibrate" {
            (1, 1)
        } else {
            (
                *run.counts
                    .and_then(|c| c.get(case))
                    .ok_or("missing frozen case count")?,
                0,
            )
        };
        loop {
            if !(1..=1_000_000_000).contains(&count) {
                return Err("invalid frozen case count".into());
            }
            environment.extend([
                ("RSE_SAMPLER_CASE".into(), case.clone()),
                ("RSE_SAMPLER_ITERATIONS".into(), count.to_string()),
                ("RSE_SAMPLER_BATCH".into(), batch.to_string()),
            ]);
            let offset = log.seek(SeekFrom::End(0))?;
            let identity = json!({"phase":run.journal.as_ref().map(|j|j.phase),"profile":run.profile,"case":case,"mode":run.mode,"iterations":count,"calibration_batch":batch});
            if let Some(journal) = &mut run.journal {
                let mut event = identity.clone();
                event["event"] = "start".into();
                event["monotonic_ns"] = context.now_ns()?.into();
                event["utc_ns"] = io::utc_ns()?.into();
                io::append_json(journal.stream, &event)?;
            }
            let command = command(
                run.root,
                &environment,
                &[
                    "release_controlled_profile",
                    "--",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ],
            )?;
            let outcome = context.logged(command, run.log, false);
            if let Some(journal) = &mut run.journal {
                let mut event = identity;
                event["event"] = "end".into();
                event["monotonic_ns"] = context.now_ns()?.into();
                event["utc_ns"] = io::utc_ns()?.into();
                event["exit_code"] = if outcome.is_ok() {
                    json!(0)
                } else {
                    Value::Null
                };
                if let Err(error) = &outcome {
                    event["error"] = error.to_string().into();
                }
                io::append_json(journal.stream, &event)?;
            }
            outcome?;
            let mut reader = File::open(run.log)?;
            reader.seek(SeekFrom::Start(offset))?;
            let mut text = String::new();
            reader
                .take(crate::artifact::snapshot::MEMBER_LIMIT + 1)
                .read_to_string(&mut text)?;
            if text.len() as u64 > crate::artifact::snapshot::MEMBER_LIMIT {
                return Err("bounded worker journal exceeded".into());
            }
            let current = parsed(&text, "RSE_SAMPLE ")?;
            if current.is_empty()
                || current
                    .iter()
                    .any(|r| r["case"] != *case || r["isolated_case"] != true)
            {
                return Err("case process did not emit exactly its selected workload".into());
            }
            let following = if run.mode == "calibrate" {
                sampling_model(
                    "calibration-batch",
                    json!({"records":current,"count":count,"batch":batch}),
                )?
            } else {
                Value::Null
            };
            records.extend(current);
            if following.is_null() {
                break;
            }
            count = following
                .as_u64()
                .ok_or("invalid following calibration count")?;
            batch += 1;
            if batch > 12 {
                return Err("calibration batch budget exceeded; observations preserved".into());
            }
        }
    }
    Ok(records)
}
pub(super) fn checkout(
    context: &Context<'_>,
    source: &Path,
    destination: &Path,
    revision: &str,
) -> Result<()> {
    if !io::revision_valid(revision) {
        return Err("require an exact source commit".into());
    }
    context.run(io::command(
        source,
        &env::capture(),
        "git",
        [
            "init",
            "--quiet",
            destination.to_str().ok_or("invalid source path")?,
        ],
    ))?;
    context.run(io::command(
        destination,
        &env::capture(),
        "git",
        [
            "fetch",
            "--quiet",
            "--no-tags",
            "--depth=1",
            source.to_str().ok_or("invalid source path")?,
            revision,
        ],
    ))?;
    context.run(io::command(
        destination,
        &env::capture(),
        "git",
        ["checkout", "--quiet", "--detach", revision],
    ))
}
pub(super) fn select(
    records: Vec<Value>,
    profile: &str,
    revision: &str,
    counts: &Value,
    cpus: &Value,
    page: &Value,
    diagnostic: bool,
) -> Result<Value> {
    let metric = crate::release_rows::metric(profile)?;
    let selected = sampling_model(
        "batch",
        json!({"request":{"profile":profile,"revision":revision,"records":records,"expected":catalog(profile)?,"counts":counts,"allowed_cpus":cpus,"expected_page_size":page,"output":{"kind":"select","metric":metric,"diagnostic":diagnostic}}}),
    )?;
    let indices: Vec<usize> = serde_json::from_value(selected["selected_indices"].clone())?;
    Ok(
        json!({"selected":indices.into_iter().map(|i|records[i]["rows"][0].clone()).collect::<Vec<_>>(),"spreads":selected["spreads"],"preparation":selected["preparation"],"worker_starts":selected["worker_starts"]}),
    )
}
/// Adapt the ordered wire counts once for both current and frozen Rust models.
pub(super) fn calibrated_profile(mut report: Value) -> Result<Value> {
    let pairs: Vec<(String, u64)> = serde_json::from_value(
        report
            .get_mut("counts")
            .ok_or("missing calibrated case counts")?
            .take(),
    )?;
    let mut counts = BTreeMap::new();
    for (case, count) in pairs {
        if !(1..=1_000_000_000).contains(&count) || counts.insert(case, count).is_some() {
            return Err("require unique bounded calibrated case counts".into());
        }
    }
    if counts.is_empty() {
        return Err("require nonempty calibrated case counts".into());
    }
    report["counts"] = serde_json::to_value(counts)?;
    Ok(report)
}
pub(super) fn calibration(
    records: &[Value],
    profile: &str,
    revision: &str,
    context: &Value,
) -> Result<Value> {
    let cpus: Option<Vec<u64>> =
        serde_json::from_value(context["isolation"]["cpu_affinity"].clone())?;
    calibrated_profile(sampling_contract::calibration_profile::analyze(
        records,
        profile,
        revision,
        serde_json::from_value(context["runtime"]["page_size"].clone())?,
        cpus.as_deref(),
    )?)
}
pub(super) fn check(context: &Context<'_>, root: &Path, workers: bool) -> Result<Value> {
    let overlay = Overlay::generated(context, root, render(root, None, false)?)?;
    let environment = env::capture();
    let output = context.capture(command(
        root,
        &environment,
        &["--", "--skip", "release_controlled_profile", "--nocapture"],
    )?)?;
    print!("{output}");
    let cpus = if cfg!(target_os = "linux") {
        Some(env::affinity(0)?)
    } else {
        None
    };
    let mut actual = BTreeMap::new();
    if workers {
        for profile in ["smoke", "hotspot"] {
            for case in catalog(profile)? {
                let mut environment = env::worker(&environment)?;
                environment.extend([
                    ("RSE_SAMPLER_PROFILE".into(), profile.into()),
                    ("RSE_SAMPLER_MODE".into(), "sample".into()),
                    ("RSE_SAMPLER_CASE".into(), case.clone()),
                    ("RSE_SAMPLER_ITERATIONS".into(), "1".into()),
                    ("RSE_SAMPLER_BATCH".into(), "0".into()),
                    (
                        "RSE_SAMPLER_PAGE_SIZE".into(),
                        env::page_size()?.to_string(),
                    ),
                ]);
                let output = context.capture(command(
                    root,
                    &environment,
                    &[
                        "release_controlled_profile",
                        "--",
                        "--exact",
                        "--ignored",
                        "--nocapture",
                        "--test-threads=1",
                    ],
                )?)?;
                let rows = parsed(&output, "RSE_SAMPLE ")?;
                if rows.len() != 12
                    || rows.iter().enumerate().any(|(i, r)| {
                        r["index"].as_u64() != Some(i as u64)
                            || r["case"] != case
                            || r["iterations"] != 1
                    })
                {
                    return Err("complete one-iteration worker output is required".into());
                }
                sampling_model(
                    "validate-workload-sequence",
                    json!({"profile":profile,"records":rows}),
                )?;
                if let Some(cpus) = &cpus {
                    for row in &rows {
                        sampling_model(
                            "validate-placement",
                            json!({"record":row,"allowed_cpus":cpus}),
                        )?;
                    }
                }
                actual.insert(
                    case,
                    rows.iter()
                        .map(|r| json!({"workload":r["workload"],"placement":r["placement"]}))
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
    let mut prefixes = parsed(&output, "RSE_INPUT_REPLAY ")?;
    let expected: BTreeSet<_> = crate::diagnostics::catalog::HOTSPOT_CASES
        .iter()
        .map(|c| c.name)
        .collect();
    if prefixes.len() != expected.len()
        || prefixes
            .iter()
            .filter_map(|r| r["case"].as_str())
            .collect::<BTreeSet<_>>()
            != expected
    {
        return Err("complete native input replay witnesses are required".into());
    }
    prefixes.sort_by(|a, b| a["case"].as_str().cmp(&b["case"].as_str()));
    if workers && cpus.is_some() {
        let placements: BTreeSet<_> = actual
            .values()
            .flatten()
            .map(|r| serde_json::to_string(&r["placement"]))
            .collect::<std::result::Result<_, _>>()?;
        if placements.len() != 1 {
            return Err("native workers changed constructor/pool placement".into());
        }
        println!(
            "RSE_PLACEMENT_CHECK {}",
            placements.first().ok_or("missing placement")?
        );
    }
    overlay.finish()?;
    Ok(json!({"prefixes":prefixes,"workers":actual}))
}
pub(super) fn cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("release-sampling", args)?;
    if !options.has("--check") {
        return Err("--check is required".into());
    }
    let current = check(context, tool(), options.has("--check-workers"))?;
    if let Some(revision) = options.get("--baseline-ref") {
        if revision != BASELINE {
            return Err("baseline-ref must be the published 9.0.0 commit".into());
        }
        let temporary = Temporary::new(context, "input-replay")?;
        let baseline = temporary.path.join("baseline");
        checkout(context, tool(), &baseline, revision)?;
        if current != check(context, &baseline, options.has("--check-workers"))? {
            return Err("published and candidate input replay witnesses differ".into());
        }
        println!("Published 9.0.0/candidate input replay matches; correctness only");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
