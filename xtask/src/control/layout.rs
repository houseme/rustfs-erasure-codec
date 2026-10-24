use super::{environment as env, io::Context, overlay::Overlay, sampling, *};
use std::{collections::BTreeMap, fs, path::Path};
pub(super) const CASES: [&str; 2] = ["verify_with_buffer:4:2:1024", "verify_with_buffer:4:2:4096"];
pub(super) fn code() -> Value {
    json!({"native_root":tool(),"controller":tool().join("xtask/src/control/layout.rs"),"template":tool().join("scripts/diagnostics/verify_layout.rs"),"sampler":tool().join("xtask/src/control/sampling.rs")})
}
pub(super) fn contract() -> Result<Value> {
    model(
        "diagnostic-layout",
        json!({"operation":"contract","code":code()}),
    )
}
pub(super) fn iterations(sampling: &Value) -> Result<u64> {
    model(
        "diagnostic-layout",
        json!({"operation":"campaign-iterations","sampling":sampling}),
    )?
    .as_u64()
    .ok_or_else(|| "invalid layout calibration count".into())
}
pub(super) fn verify(
    directory: &Path,
    candidate: &Path,
    compiler: bool,
    diagnostic: bool,
    sha: Option<&str>,
    campaign: Option<Value>,
) -> Result<Value> {
    let request = json!({"code":code(),"baseline":env::baseline(candidate)?,"candidate":env::source(candidate)?,"tool":env::source(tool())?});
    let compiler = if compiler {
        Some(env::compiler(candidate)?)
    } else {
        None
    };
    bound(
        directory,
        diagnostic,
        sha,
        campaign,
        request,
        compiler.as_deref(),
    )
}
pub(super) fn bound(
    directory: &Path,
    diagnostic: bool,
    sha: Option<&str>,
    campaign: Option<Value>,
    mut request: Value,
    compiler: Option<&str>,
) -> Result<Value> {
    request["directory"] = serde_json::to_value(directory)?;
    request["diagnostic"] = diagnostic.into();
    request["campaign"] = campaign.unwrap_or(Value::Null);
    if let Some(sha) = sha {
        request["manifest_sha256"] = sha.into();
    }
    let result = model(
        "diagnostic-layout",
        json!({"operation":"paired-replay","request":request}),
    )?;
    if compiler.is_some_and(|compiler| result["context"]["compiler"] != compiler) {
        return Err("paired compiler differs from the current compiler".into());
    }
    Ok(result)
}

fn files() -> Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([(
        "root.rs".into(),
        fs::read_to_string(tool().join("scripts/diagnostics/verify_layout.rs"))?,
    )]))
}
pub(super) fn phase_passed(report: &Value) -> bool {
    ["short_samples", "wide_cases", "repeat_failures"]
        .iter()
        .all(|k| report[k].as_array().is_some_and(Vec::is_empty))
}
pub(super) fn capture(
    context: &Context<'_>,
    root: &Path,
    output: &Path,
    group: &Path,
    iterations: u64,
) -> Result<Value> {
    if !(1..=1_000_000_000).contains(&iterations) {
        return Err("invalid bounded iteration count".into());
    }
    io::directory(output)?;
    let environment = env::build(root, &env::capture())?;
    let resource = env::context(root, &environment, group)?;
    let source = env::source(root)?;
    let tool_source = env::source(tool())?;
    let cpu = resource["isolation"]["cpu_affinity"][0]
        .as_u64()
        .ok_or("missing measuring CPU")?;
    let page = resource["runtime"]["page_size"]
        .as_u64()
        .ok_or("missing page size")?;
    let mut metadata = json!({"schema_version":"verify-layout-diagnostic-v3","source":source,"tool":tool_source,"context":resource,"iterations":iterations,"measuring_cpu":cpu,"purpose":"supplemental controlled slices; never replace original Vec release cases","candidate_attribution":null,"release_approval":false});
    metadata.as_object_mut().ok_or("invalid metadata")?.extend(
        contract()?
            .as_object()
            .ok_or("invalid layout contract")?
            .clone(),
    );
    io::write_json(&output.join("provenance.json"), &metadata, false)?;
    let overlay = Overlay::generated(context, root, files()?)?;
    context.logged(
        sampling::command(root, &environment, &["--no-run"])?,
        &output.join("build.log"),
        true,
    )?;
    let mut worker = environment.clone();
    worker.extend([
        ("RSE_LAYOUT_ITERATIONS".into(), iterations.to_string()),
        ("RSE_LAYOUT_PAGE_SIZE".into(), page.to_string()),
        ("RSE_LAYOUT_CPU".into(), cpu.to_string()),
    ]);
    let mut args = vec!["--cpu-list".into(), cpu.to_string(), "cargo".into()];
    args.extend(sampling::TEST_ARGS.iter().map(|v| (*v).into()));
    args.extend(
        [
            "verify_alignment_control",
            "--",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(str::to_owned),
    );
    let log = output.join("measurement.log");
    let outcome = context.logged(io::command(root, &worker, "taskset", args), &log, true);
    let parsed = if io::present(&log) {
        sampling::parsed(&fs::read_to_string(&log)?, "RSE_LAYOUT_SAMPLE ")
    } else {
        Err("missing layout worker log".into())
    };
    match &parsed {
        Ok(rows) => io::write_json(&output.join("samples.partial.json"), &rows, false)?,
        Err(error) => fs::write(output.join("extraction-error.txt"), format!("{error}\n"))?,
    };
    outcome?;
    let rows = parsed?;
    io::write_json(&output.join("samples.json"), &rows, false)?;
    let report = model(
        "diagnostic-layout",
        json!({"operation":"verify","records":rows,"iterations":iterations,"page_size":page,"measuring_cpu":cpu}),
    )?;
    if env::source(root)? != source
        || env::source(tool())? != tool_source
        || env::context(root, &environment, group)? != resource
    {
        return Err("source or resource context changed during diagnosis".into());
    }
    overlay.finish()?;
    metadata
        .as_object_mut()
        .ok_or("invalid metadata")?
        .extend(report.as_object().ok_or("invalid layout report")?.clone());
    io::write_json(&output.join("diagnosis.json"), &metadata, false)?;
    Ok(report)
}
pub(super) fn capture_paired(
    context: &Context<'_>,
    baseline: &Path,
    candidate: &Path,
    output: &Path,
    group: &Path,
    iterations: u64,
    diagnostic: bool,
) -> Result<Value> {
    let sources = json!({"baseline":env::source(baseline)?,"candidate":env::source(candidate)?});
    if sources["baseline"]["commit"] != BASELINE
        || sources["baseline"]["tree"] == sources["candidate"]["tree"]
    {
        return Err("pair the published 9.0.0 with a different candidate".into());
    }
    io::directory(output)?;
    let mut document = json!({"schema_version":"verify-layout-paired-v2","phase_order":crate::abba::STAGES,"contract":contract()?,"sources":sources,"tool":env::source(tool())?,"files":{},"release_approval":false});
    if diagnostic {
        document["diagnostic_only"] = true.into();
    }
    io::write_json(&output.join("manifest.partial.json"), &document, false)?;
    let result = (|| -> Result<Value> {
        let mut selected = None;
        for phase in crate::abba::STAGES {
            let root = if phase.starts_with('A') {
                baseline
            } else {
                candidate
            };
            let report = capture(context, root, &output.join(phase), group, iterations)?;
            let metadata = io::json(&output.join(phase).join("provenance.json"), None)?;
            let role = if phase.starts_with('A') {
                "baseline"
            } else {
                "candidate"
            };
            if metadata["source"] != sources[role]
                || metadata["tool"] != document["tool"]
                || selected.as_ref().is_some_and(|c| c != &metadata["context"])
            {
                return Err("paired phase source/tool/resource context differs".into());
            }
            selected = Some(metadata["context"].clone());
            for name in ["provenance.json", "samples.json", "measurement.log"] {
                document["files"][phase][name] = json!({"path":format!("{phase}/{name}"),"sha256":io::digest(&output.join(phase).join(name))?});
            }
            io::write_json(&output.join("manifest.partial.json"), &document, false)?;
            if !diagnostic && !phase_passed(&report) {
                return Err(super::release::refused(
                    "layout phase quality failed; stop later phases",
                    json!({"phase":phase,"report":report}),
                ));
            }
        }
        io::write_json(
            &output.join(if diagnostic {
                "diagnostic-manifest.json"
            } else {
                "manifest.json"
            }),
            &document,
            false,
        )?;
        let report = verify(output, candidate, true, diagnostic, None, None)?;
        io::write_json(&output.join("pairing.json"), &report, false)?;
        if !diagnostic && report["layout_gate_passed"] != true {
            return Err(super::release::refused(
                "paired layout drift or regression failed",
                report,
            ));
        }
        Ok(report)
    })();
    if let Err(error) = &result {
        io::write_json(
            &output.join("failure.json"),
            &json!({"error":error.to_string(),"candidate_attribution":null,"release_approval":false}),
            false,
        )?;
    }
    result
}
pub(super) fn check(context: &Context<'_>) -> Result<()> {
    let root = tool();
    let overlay = Overlay::generated(context, root, files()?)?;
    let environment = env::capture();
    context.run(sampling::command(root, &environment, &[])?)?;
    #[cfg(target_os = "linux")]
    {
        let cpu = *env::affinity(0)?.first().ok_or("empty affinity")?;
        let page = env::page_size()?;
        let mut environment = environment;
        environment.extend([
            ("RSE_LAYOUT_ITERATIONS".into(), "1".into()),
            ("RSE_LAYOUT_PAGE_SIZE".into(), page.to_string()),
            ("RSE_LAYOUT_CPU".into(), cpu.to_string()),
        ]);
        let mut args = vec!["--cpu-list".into(), cpu.to_string(), "cargo".into()];
        args.extend(sampling::TEST_ARGS.iter().map(|v| (*v).into()));
        args.extend(
            [
                "verify_alignment_control",
                "--",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ]
            .map(str::to_owned),
        );
        let text = context.capture(io::command(root, &environment, "taskset", args))?;
        model(
            "diagnostic-layout",
            json!({"operation":"verify","records":sampling::parsed(&text,"RSE_LAYOUT_SAMPLE ")?,"iterations":1,"page_size":page,"measuring_cpu":cpu}),
        )?;
    }
    overlay.finish()
}
pub(super) fn cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("diagnose-verify-layout", args)?;
    if options.has("--check") {
        return check(context);
    }
    if options.get("--verify").is_some() {
        let report = verify(
            &options.path("--verify")?,
            &options.path("--candidate")?,
            true,
            false,
            None,
            None,
        )?;
        println!("{report}");
        if report["layout_gate_passed"] != true {
            return Err(super::release::refused("layout gate failed", report));
        }
        return Ok(());
    }
    let output = options.path("--output")?;
    let group = options.path("--isolation-cgroup")?;
    let iterations = options.number("--iterations", 262144, 1_000_000_000)?;
    if options.get("--repository").is_some() {
        if options.get("--candidate").is_some() || options.get("--baseline").is_some() {
            return Err("choose repository or baseline/candidate".into());
        }
        capture(
            context,
            &options.path("--repository")?,
            &output,
            &group,
            iterations,
        )?;
    } else {
        capture_paired(
            context,
            &options.path("--baseline")?,
            &options.path("--candidate")?,
            &output,
            &group,
            iterations,
            false,
        )?;
    }
    Ok(())
}
