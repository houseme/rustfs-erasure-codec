use super::{environment as env, io::Context, layout, sampling, *};
use crate::{Args, artifact::python_json::equivalent, sampling_contract};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug)]
pub(super) struct Failure {
    message: String,
    pub details: Value,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
pub(super) fn refused(message: &str, details: Value) -> Box<dyn std::error::Error> {
    Box::new(Failure {
        message: message.into(),
        details,
    })
}
pub(super) const FILES: [(&str, &str); 4] = [
    ("smoke", "smoke-results.json"),
    ("small_file", "small-file-results.json"),
    ("hotspot", "reconstruction-hotspot-results.json"),
    ("stream", "stream-path-results.json"),
];
pub(super) const VARIABLES: [&str; 4] = [
    "RSE_SMOKE_BASELINE",
    "RSE_SMALL_FILE_BASELINE",
    "RSE_RECONSTRUCTION_HOTSPOT_BASELINE",
    "RSE_STREAM_PATH_BASELINE",
];
pub(super) const DIAGNOSTIC: &str = "complete-campaign-diagnostic-v1";
pub(super) fn implementation() -> Result<String> {
    crate::fingerprint::source(
        tool(),
        crate::fingerprint::Kind::Sampling,
        Some(&tool().join("xtask/src/control/sampling.rs")),
    )
}
pub(super) fn harness(root: &Path) -> Result<String> {
    crate::fingerprint::source(root, crate::fingerprint::Kind::Harness, None)
}
pub(super) fn contract(root: &Path) -> Result<Value> {
    let mut value = sampling_contract::protocol_value();
    value["implementation_sha256"] = implementation()?.into();
    value["overlay_sha256"] = sampling::overlay_digest(root, None)?.into();
    Ok(value)
}
fn checker_args(
    profile: &str,
    baseline: &Path,
    current: &Path,
    old_sha: &str,
    new_sha: &str,
) -> Result<Args> {
    let mut values = BTreeMap::from([
        ("--baseline".into(), baseline.display().to_string()),
        ("--current".into(), current.display().to_string()),
        ("--baseline-sha256".into(), old_sha.into()),
        ("--current-sha256".into(), new_sha.into()),
    ]);
    let mut repeated = BTreeMap::new();
    let required = match profile {
        "smoke" => vec![
            "encode:4:2:65536",
            "encode:10:4:1048576",
            "verify:10:4:1048576",
            "reconstruct:10:4:1048576",
            "reconstruct_data:10:4:1048576",
        ],
        "small_file" => vec![
            "encode:4:2:1024",
            "verify_with_buffer:4:2:4096",
            "reconstruct:4:2:16384",
            "reconstruct_data:10:4:65536",
        ],
        "stream" => vec![
            "encode_stream:4:2:65536:65536",
            "verify_stream:4:2:65536:65536",
            "reconstruct_stream:10:4:1048576:1048576",
        ],
        "hotspot" => crate::diagnostics::catalog::HOTSPOT_CASES
            .iter()
            .map(|c| c.name)
            .collect(),
        _ => return Err("unknown regression profile".into()),
    };
    repeated.insert(
        if profile == "hotspot" {
            "--require-scenario"
        } else {
            "--require-case"
        }
        .into(),
        required.into_iter().map(str::to_owned).collect(),
    );
    if profile != "hotspot" && profile != "smoke" {
        values.insert(
            "--metric".into(),
            crate::release_rows::metric(profile)?.into(),
        );
    }
    let thresholds = match profile {
        "small_file" => vec![
            "encode=0.12",
            "verify=0.12",
            "verify_with_buffer=0.12",
            "reconstruct=0.18",
            "reconstruct_data=0.18",
        ],
        "stream" => vec![
            "encode_stream=0.15",
            "verify_stream=0.15",
            "reconstruct_stream=0.18",
        ],
        _ => Vec::new(),
    };
    repeated.insert(
        "--threshold".into(),
        thresholds.into_iter().map(str::to_owned).collect(),
    );
    Ok(Args(values, repeated))
}
#[derive(Clone)]
struct Observed {
    baseline: Value,
    candidate: Value,
    tool: Value,
    compiler: Option<String>,
    harness: String,
    sampling: Value,
    layout: Value,
}
impl Observed {
    fn live(repository: &Path, compiler: bool) -> Result<Self> {
        source_current()?;
        let baseline = env::baseline(repository)?;
        let candidate = env::source(repository)?;
        let tool_source = env::source(tool())?;
        let compiler = if compiler {
            Some(env::compiler(repository)?)
        } else {
            None
        };
        let layout = json!({"baseline":baseline,"candidate":candidate,"tool":tool_source,"code":layout::code()});
        Ok(Self {
            baseline,
            candidate,
            tool: tool_source,
            compiler,
            harness: harness(repository)?,
            sampling: contract(repository)?,
            layout,
        })
    }
    fn unchanged(&self, other: &Self) -> bool {
        self.baseline == other.baseline
            && ["tree", "lock_sha256"]
                .iter()
                .all(|k| self.candidate[k] == other.candidate[k] && self.tool[k] == other.tool[k])
            && self.compiler == other.compiler
            && self.harness == other.harness
            && self.sampling == other.sampling
            && self.layout["contract"] == other.layout["contract"]
    }
}
pub(super) fn profiles(
    document: &Value,
    root: &Path,
    repository: &Path,
    compiler: bool,
    diagnostic: bool,
) -> Result<(Value, Value)> {
    profiles_bound(
        document,
        root,
        &Observed::live(repository, compiler)?,
        diagnostic,
    )
}
fn profiles_bound(
    document: &Value,
    root: &Path,
    observed: &Observed,
    diagnostic: bool,
) -> Result<(Value, Value)> {
    let actual = &observed.candidate;
    let bindings = json!({"baseline":observed.baseline,"candidate":actual,"harness_sha256":observed.harness,"compiler":observed.compiler});
    let expected = &observed.sampling;
    if expected
        .as_object()
        .ok_or("invalid sampling contract")?
        .iter()
        .any(|(k, v)| {
            !document["sampling"]
                .get(k)
                .is_some_and(|recorded| equivalent(recorded, v))
        })
    {
        return Err("sampling protocol/implementation mismatch".into());
    }
    let replay = sampling_model(
        "campaign",
        json!({"request":{"capture":root,"document":document,"diagnostic":diagnostic,"bindings":bindings}}),
    )?;
    if !diagnostic && replay["unstable"] == true {
        return Err(refused(
            "sample distributions are unstable; no candidate attribution",
            json!({"sample_spreads":replay["sample_spreads"],"candidate_attribution":null}),
        ));
    }
    let mut analysis = replay["analysis"].clone();
    let drifts = analysis["repeat_checks"]
        .as_array()
        .ok_or("missing repeat checks")?;
    if !diagnostic
        && drifts.iter().any(|r| {
            r["drift"]
                .as_f64()
                .is_some_and(|n| n > sampling_contract::REPEAT_DRIFT_LIMIT)
        })
    {
        return Err(refused(
            "ABBA drift exceeds 5%; no candidate attribution or release approval",
            json!({"drift_checks":drifts,"sample_spreads":replay["sample_spreads"],"candidate_attribution":null}),
        ));
    }
    let drift_count = drifts.len();
    let mut checks = Vec::new();
    for (profile, _) in FILES {
        for a in ["A1", "A2"] {
            for b in ["B1", "B2"] {
                let old = &document["files"][a][profile];
                let new = &document["files"][b][profile];
                let old_path =
                    io::member(root, old["path"].as_str().ok_or("missing summary path")?)?;
                let new_path =
                    io::member(root, new["path"].as_str().ok_or("missing summary path")?)?;
                let args = checker_args(
                    profile,
                    &old_path,
                    &new_path,
                    old["sha256"].as_str().ok_or("missing summary hash")?,
                    new["sha256"].as_str().ok_or("missing summary hash")?,
                )?;
                let result = if profile == "hotspot" {
                    crate::regression::hotspot_report(&args)?
                } else {
                    crate::regression::benchmark_report(&args)?
                };
                if diagnostic {
                    checks.push(json!({"profile":profile,"pair":format!("{a}/{b}"),"exit_code":if result.passed{0}else{1},"stdout":result.stdout,"stderr":result.stderr}));
                } else if !result.passed {
                    return Err(refused(
                        "regression validation failed",
                        json!({"profile":profile,"pair":format!("{a}/{b}")}),
                    ));
                }
            }
        }
    }
    if diagnostic {
        analysis["worker_invocations"] = replay["worker_invocations"].clone();
        let passed = checks.iter().all(|r| r["exit_code"] == 0);
        analysis["regression_checks"] = checks.into();
        analysis["all_original_limits_passed"] =
            (analysis["all_original_limits_passed"] == true && passed).into();
        Ok((replay["layout_rows"].clone(), analysis))
    } else {
        Ok((
            replay["layout_rows"].clone(),
            json!({"status":"profiles_verified","candidate_commit":actual["commit"],"candidate_tree":actual["tree"],"measured_candidate_commit":document["sources"]["candidate"]["commit"],"baseline_commit":document["sources"]["baseline"]["commit"],"drift_checks":drift_count,"sample_distribution_checks":replay["sample_spreads"].as_array().ok_or("missing spreads")?.len(),"case_counts":replay["case_counts"],"scope":"controlled recorded VM campaign; no new measurement on the verification runner"}),
        ))
    }
}
pub(super) fn verify(
    manifest: &Path,
    repository: &Path,
    compiler: bool,
    diagnostic: bool,
) -> Result<(Value, Value)> {
    verify_observed(manifest, diagnostic, || {
        Observed::live(repository, compiler)
    })
}
fn verify_observed(
    manifest: &Path,
    diagnostic: bool,
    mut observe: impl FnMut() -> Result<Observed>,
) -> Result<(Value, Value)> {
    let (document, sha) = io::bound_json(manifest)?;
    let root = manifest.parent().ok_or("missing manifest directory")?;
    if diagnostic {
        if !document.is_object()
            || document["schema_version"] != DIAGNOSTIC
            || document["phase_order"] != json!(crate::abba::STAGES)
            || document["diagnostic_only"].as_bool() != Some(true)
            || document["release_approval"].as_bool() != Some(false)
        {
            return Err("a complete diagnostic manifest is required".into());
        }
    } else {
        if !document.is_object() {
            return Err("evidence manifest must be an object".into());
        }
        for field in ["sources", "contexts", "files"] {
            if !document[field].is_object() {
                return Err(format!("invalid manifest container: {field}").into());
            }
        }
        if document["schema_version"].as_u64() != Some(4)
            || document["phase_order"] != json!(crate::abba::STAGES)
        {
            return Err("sampled schema 4 with kernel memory policy, complete ABBA and paired layouts is required; older envelopes cannot approve release".into());
        }
    }
    let expected = if diagnostic {
        "layout-pair/diagnostic-manifest.json"
    } else {
        "layout-pair/manifest.json"
    };
    let entry = &document["layout_supplement"];
    if !entry.is_object()
        || entry["path"] != expected
        || !entry["sha256"].as_str().is_some_and(io::digest_valid)
    {
        return Err("a bound complete paired-layout supplement is required".into());
    }
    let layout_path = io::member(root, expected)?;
    let layout_sha = entry["sha256"].as_str().ok_or("missing layout digest")?;
    io::json(&layout_path, Some(layout_sha))?;
    let observed = observe()?;
    let (data, mut report) = profiles_bound(&document, root, &observed, diagnostic)?;
    let counts: BTreeMap<_, _> = layout::CASES
        .map(|case| {
            (
                case,
                document["sampling"]["common_iterations"]["small_file"][case].clone(),
            )
        })
        .into();
    let rows: BTreeMap<_, _> = layout::CASES
        .map(|case| (case, data["A1"]["small_file"][case].clone()))
        .into();
    let layout = layout::bound(
        layout_path.parent().ok_or("missing layout directory")?,
        diagnostic,
        Some(layout_sha),
        Some(
            json!({"sources":document["sources"],"context":document["contexts"]["A1"],"counts":counts,"rows":rows}),
        ),
        observed.layout.clone(),
        observed.compiler.as_deref(),
    )?;

    if diagnostic {
        report["status"] = "diagnostic_complete".into();
        report["diagnostic_only"] = true.into();
        report["release_approval"] = false.into();
        report["candidate_attribution"] = Value::Null;
        report["all_checks_executed"] = true.into();
        report["manifest_sha256"] = sha.into();
        report["all_original_limits_passed"] = (report["all_original_limits_passed"] == true
            && layout["layout_gate_passed"] == true)
            .into();
        report["layout"] = layout;
    } else {
        if layout["layout_gate_passed"] != true {
            return Err(refused(
                "paired layouts failed; no release approval",
                json!({"layout":layout,"candidate_attribution":null}),
            ));
        }
        let phases = layout["phase_reports"]
            .as_object()
            .ok_or("missing layout phases")?;
        report["status"] = "passed".into();
        report["manifest_sha256"] = sha.into();
        report["layout_manifest_sha256"] = layout_sha.into();
        report["layout_distribution_checks"] = json!(
            phases
                .values()
                .map(|r| r["case_count"].as_u64().unwrap_or(0))
                .sum::<u64>()
        );
        report["layout_repeat_checks"] = json!(
            layout["repeat_checks"]
                .as_array()
                .ok_or("missing layout repeats")?
                .len()
                + phases
                    .values()
                    .map(|r| r["repeat_checks"].as_array().map_or(0, Vec::len))
                    .sum::<usize>()
        );
        report["layout_comparisons"] = json!(
            layout["candidate_attribution"]
                .as_array()
                .ok_or("missing layout comparisons")?
                .len()
        );
    }
    if !observed.unchanged(&observe()?) {
        return Err("source, compiler or tool contract changed during verification".into());
    }
    Ok((document, report))
}
pub(super) fn materialize(manifest: &Path, repository: &Path, env_file: &Path) -> Result<()> {
    let (document, report) = verify(manifest, repository, true, false)?;
    let inputs = repository.join("target/release-preflight/inputs");
    let current = repository.join("target/benchmark-smoke");
    fs::create_dir_all(&inputs)?;
    fs::create_dir_all(&current)?;
    let mut values = String::new();
    for (index, (profile, name)) in FILES.into_iter().enumerate() {
        let root = manifest.parent().ok_or("missing manifest parent")?;
        let baseline = io::member(
            root,
            document["files"]["A1"][profile]["path"]
                .as_str()
                .ok_or("missing baseline path")?,
        )?;
        let candidate = io::member(
            root,
            document["files"]["B2"][profile]["path"]
                .as_str()
                .ok_or("missing candidate path")?,
        )?;
        fs::copy(baseline, inputs.join(name))?;
        fs::copy(candidate, current.join(name))?;
        let path = inputs.join(name).display().to_string();
        if path.contains(['\n', '\r']) {
            return Err("invalid output path".into());
        }
        values.push_str(&format!("{}={path}\n", VARIABLES[index]));
    }
    fs::write(env_file, values)?;
    io::write_json(
        &repository.join("target/release-preflight/evidence-validation.json"),
        &report,
        false,
    )
}
pub(super) fn cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("release-evidence", args)?;
    let repository = options
        .get("--repository")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let repository = io::absolute(&repository)?;
    let mode = options
        .mode
        .as_deref()
        .ok_or("release-evidence mode is required")?;
    options.only(match mode {
        "capture" | "diagnose-complete" => &[
            "--baseline",
            "--candidate",
            "--output",
            "--isolation-cgroup",
        ][..],
        "qualify-baseline" => &["--baseline", "--output", "--isolation-cgroup"],
        "materialize" => &["--manifest", "--repository", "--env-file"],
        _ => &["--manifest", "--repository", "--report"],
    })?;
    let capture = matches!(mode, "capture" | "diagnose-complete" | "qualify-baseline");
    let absent = capture
        && options
            .get("--output")
            .is_some_and(|p| !io::present(Path::new(p)));
    let outcome = (|| -> Result<()> {
        source_current()?;
        match mode {
            "capture" | "diagnose-complete" => super::capture::campaign(
                context,
                &options.path("--baseline")?,
                &options.path("--candidate")?,
                &options.path("--output")?,
                &options.path("--isolation-cgroup")?,
                mode == "diagnose-complete",
            ),
            "qualify-baseline" => {
                super::capture::qualify(
                    context,
                    &options.path("--baseline")?,
                    &options.path("--output")?,
                    &options.path("--isolation-cgroup")?,
                )?;
                Ok(())
            }
            "materialize" => materialize(
                &options.path("--manifest")?,
                &repository,
                &options.path("--env-file")?,
            ),
            _ => {
                let (_, report) = verify(
                    &options.path("--manifest")?,
                    &repository,
                    true,
                    mode == "verify-diagnostic",
                )?;
                if options.get("--report").is_some() {
                    let path = options.path("--report")?;
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    io::write_json(&path, &report, false)?;
                }
                Ok(())
            }
        }
    })();
    if let Err(error) = &outcome {
        let path = if mode == "materialize" {
            Some(repository.join("target/release-preflight/evidence-validation.json"))
        } else if absent
            && options
                .get("--output")
                .is_some_and(|p| Path::new(p).is_dir())
        {
            Some(options.path("--output")?.join("report.json"))
        } else {
            options.get("--report").map(PathBuf::from)
        };
        if let Some(path) = path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut report = json!({"status":"failed","error":error.to_string()});
            if let Some(failure) = error.downcast_ref::<Failure>()
                && let Some(details) = failure.details.as_object()
            {
                report
                    .as_object_mut()
                    .ok_or("invalid failed report")?
                    .extend(details.clone());
            }
            io::write_json(&path, &report, false)?;
        }
    }
    outcome?;
    println!(
        "{}",
        if mode == "verify-diagnostic" || mode == "diagnose-complete" {
            "complete diagnostic; no release approval"
        } else if mode == "qualify-baseline" {
            "baseline qualification completed; not release acceptance"
        } else {
            "release evidence verified"
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests;
