//! Read-only VM counters and complete predeclared frequency cohorts.
use super::{baseline, capture, environment as env, io::Context, release, sampling, *};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::Path,
};
const FREQUENCY: &str = "baseline-worker-frequency-v1";
const SCOPE: &str = "whole original worker on guest CPUs, including setup, warmups, samples, export, kernel and other guest activity; virtualized counter fidelity and physical-host interference unproven; no exclusive API cycles, GHz inference or timing normalization";
fn vm(operation: &str, mut request: Value) -> Result<Value> {
    request
        .as_object_mut()
        .ok_or("invalid VM request")?
        .insert("operation".into(), operation.into());
    model("vm-analysis", request)
}
fn basic(records: Vec<Value>, native: bool) -> Result<Vec<Value>> {
    Ok(serde_json::from_value(vm(
        "basic",
        json!({"records":records,"native_service_clock":native}),
    )?)?)
}
fn probe(
    context: &Context<'_>,
    root: &Path,
    environment: &io::Environment,
    output: &Path,
) -> Result<Value> {
    let environment = env::worker(environment)?;
    let cpu = environment
        .get("RSE_SAMPLER_CPU")
        .ok_or("native counter probe requires Linux CPU affinity")?
        .parse::<u64>()?;
    let path = output.join("native-service-clock-probe.log");
    context.logged(
        sampling::command(
            root,
            &environment,
            &[
                "native_service_clock_probe",
                "--",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
        )?,
        &path,
        true,
    )?;
    let summary = vm("probe", json!({"text":io::text(&path,None)?,"cpu":cpu}))?;
    Ok(json!({"codec_calls":0,"log_sha256":io::digest(&path)?,"summary":summary}))
}
pub(super) fn capture(
    context: &Context<'_>,
    baseline: &Path,
    output: &Path,
    group: &Path,
    native: bool,
    probe_only: bool,
) -> Result<Value> {
    if probe_only && !native {
        return Err("probe-only requires native service counters".into());
    }
    io::directory(output)?;
    let result = (|| -> Result<Value> {
        let mut environment = env::capture();
        environment.insert("RSE_SAMPLER_DIAGNOSTIC_PROCESS".into(), "1".into());
        let source = env::source(baseline)?;
        if source["commit"] != BASELINE {
            return Err("diagnose the published 9.0.0 baseline only".into());
        }
        let resource = env::context(baseline, &environment, group)?;
        let tool_source = env::source(tool())?;
        let variant = if native {
            Some("native-service-clock")
        } else {
            None
        };
        let rendered = sampling::render(baseline, variant, false)?;
        let mut report = json!({"schema_version":"baseline-vm-diagnostic-v1","purpose":"within-version diagnosis only; never release acceptance","candidate_attribution":null,"source":source,"context":resource,"diagnostic_tool":tool_source,"renderer_sha256":release::implementation()?,"diagnostic_overlay_sha256":crate::fingerprint::overlay(&rendered,baseline)?,"diagnostic_process_observer":true,"observer":"anonymous live threads plus process minor/major faults; snapshots outside timers; counters may include observation/setup effects","native_service_clock":native,"release_approval":false,"formal_qualification":false,"cases":[]});
        let mut analyses = Vec::new();
        {
            let overlay = super::overlay::Overlay::generated(context, baseline, rendered.clone())?;
            context.logged(
                sampling::command(baseline, &environment, &["--no-run"])?,
                &output.join("build.log"),
                true,
            )?;
            if native {
                report["service_clock_probe"] = probe(context, baseline, &environment, output)?;
                report["service_clock_template_sha256"] = crate::artifact::snapshot::digest(
                    rendered
                        .get("native_service_clock.rs")
                        .ok_or("missing native clock source")?
                        .as_bytes(),
                )
                .into();
            }
            io::write_json(&output.join("diagnosis.partial.json"), &report, false)?;
            if !probe_only {
                let mut counts = BTreeMap::new();
                for (profile, _) in release::FILES {
                    let cases = sampling::catalog(profile)?;
                    let rows = sampling::run_profile(
                        context,
                        sampling::Run {
                            root: baseline,
                            environment: &environment,
                            profile,
                            mode: "calibrate",
                            log: &output.join(format!("{profile}-calibration.log")),
                            cases: &cases,
                            counts: None,
                            journal: None,
                        },
                    )?;
                    io::write_json(
                        &output.join(format!("{profile}-calibration.json")),
                        &rows,
                        true,
                    )?;
                    let calibrated = sampling::calibration(
                        basic(rows, native)?,
                        profile,
                        BASELINE,
                        &json!({"runtime":{"page_size":null},"isolation":{"cpu_affinity":null}}),
                    )?;
                    let selected: BTreeMap<String, u64> =
                        serde_json::from_value(calibrated["counts"].clone())?;
                    counts.insert(profile, selected);
                    if env::context(baseline, &environment, group)? != resource {
                        return Err("resource context changed during calibration".into());
                    }
                }
                for (profile, _) in release::FILES {
                    let cases = sampling::catalog(profile)?;
                    fs::write(
                        output.join(format!("{profile}-counts.tsv")),
                        counts[profile]
                            .iter()
                            .map(|(c, n)| format!("{c}\t{n}\n"))
                            .collect::<String>(),
                    )?;
                    let rows = sampling::run_profile(
                        context,
                        sampling::Run {
                            root: baseline,
                            environment: &environment,
                            profile,
                            mode: "sample",
                            log: &output.join(format!("{profile}.log")),
                            cases: &cases,
                            counts: Some(&counts[profile]),
                            journal: None,
                        },
                    )?;
                    io::write_json(&output.join(format!("{profile}-samples.json")), &rows, true)?;
                    let prepared = vm(
                        "prepare",
                        json!({"records":rows,"profile":profile,"native_service_clock":native}),
                    )?;
                    sampling_model(
                        "batch",
                        json!({"request":{"profile":profile,"revision":BASELINE,"records":prepared["basic"],"expected":cases,"counts":counts[profile],"allowed_cpus":resource["isolation"]["cpu_affinity"],"expected_page_size":null,"output":{"kind":"groups"}}}),
                    )?;
                    analyses.extend(
                        prepared["analysis"]
                            .as_array()
                            .ok_or("missing VM analysis")?
                            .iter()
                            .cloned(),
                    );
                    report["cases"] = json!(analyses);
                    io::write_json(&output.join("diagnosis.partial.json"), &report, false)?;
                    if env::context(baseline, &environment, group)? != resource {
                        return Err("resource context changed".into());
                    }
                }
            }
            overlay.finish()?;
        }
        if env::source(baseline)? != source
            || env::source(tool())? != tool_source
            || env::context(baseline, &environment, group)? != resource
        {
            return Err("source or resource context changed after cleanup".into());
        }
        report["status"] = if probe_only {
            "zero-codec-probe-complete; diagnostic only"
        } else {
            "collected; diagnostic only"
        }
        .into();
        io::write_json(&output.join("diagnosis.json"), &report, true)?;
        Ok(report)
    })();
    if let Err(error) = &result {
        io::failure(output, error)?;
    }
    result
}
fn cohort(report: &Value, input: &baseline::Inputs) -> Result<Value> {
    let mut failed = Vec::new();
    for case in report["cases"].as_array().ok_or("missing adjacent cases")? {
        if case["repeat"]["exceeds_repeat_limit"].as_bool() == Some(true) {
            failed.push((
                case["profile"]
                    .as_str()
                    .ok_or("missing failed profile")?
                    .to_owned(),
                case["case"]
                    .as_str()
                    .ok_or("missing failed case")?
                    .to_owned(),
            ));
        }
    }
    failed.sort();
    if failed.is_empty() {
        return Err("no full-catalog failures to diagnose".into());
    }
    let mut steps = Vec::new();
    for repetition in 0..4 {
        for (profile, case) in &failed {
            steps.push(json!({"ordinal":steps.len(),"profile":profile,"case":case,"repetition":repetition,"iterations":input.counts[profile][case]}));
        }
    }
    Ok(
        json!({"prior_and_capture_sha256":report["input_sha256"],"failed_cases":failed.into_iter().map(|(p,c)|format!("{p}/{c}")).collect::<Vec<_>>(),"repetitions":4,"steps":steps}),
    )
}
struct MsrReader {
    files: Vec<(String, File)>,
    registers: Vec<(String, u64)>,
}
impl MsrReader {
    fn new(cpus: &[u64]) -> Result<Self> {
        let contract = vm("contract", json!({}))?;
        let registers = serde_json::from_value(contract["msrs"].clone())?;
        let files = cpus
            .iter()
            .map(|cpu| Ok((cpu.to_string(), File::open(format!("/dev/cpu/{cpu}/msr"))?)))
            .collect::<Result<_>>()?;
        Ok(Self { files, registers })
    }
    #[cfg(unix)]
    fn snapshot(&self) -> Result<Value> {
        use std::os::unix::fs::FileExt;
        let start = io::monotonic_ns()?;
        let mut cpus = json!({});
        for (cpu, file) in &self.files {
            for (name, register) in &self.registers {
                let mut raw = [0u8; 8];
                if file.read_at(&mut raw, *register)? != raw.len() {
                    return Err("short MSR read".into());
                }
                cpus[cpu][name] = u64::from_le_bytes(raw).into();
            }
        }
        Ok(json!({"start_ns":start,"cpus":cpus,"end_ns":io::monotonic_ns()?}))
    }
    #[cfg(not(unix))]
    fn snapshot(&self) -> Result<Value> {
        Err("read-only MSR capture requires Linux".into())
    }
}
fn frequency_result(
    output: &Path,
    plan: &Value,
    input: &baseline::Inputs,
    context: &Value,
) -> Result<Value> {
    let (original, plan_sha) = io::bound_json(&output.join("plan.json"))?;
    if original != *plan {
        return Err("frequency plan differs from original".into());
    }
    let raw = io::bytes(&output.join("execution.jsonl"), None)?;
    let execution_sha = crate::artifact::snapshot::digest(&raw);
    let entries: Vec<Value> = String::from_utf8(raw)?
        .lines()
        .map(crate::artifact::python_json::parse)
        .collect::<Result<_>>()?;
    let steps = plan["cohort"]["steps"]
        .as_array()
        .ok_or("missing complete cohort")?;
    if entries.len() != steps.len() {
        return Err("incomplete frequency cohort".into());
    }
    let mut reports = Vec::new();
    let mut last = 0;
    for (step, entry) in steps.iter().zip(&entries) {
        let ordinal = step["ordinal"].as_u64().ok_or("invalid ordinal")?;
        let member = io::member(output, &format!("workers/{ordinal:03}.json"))?;
        let log = io::member(output, &format!("workers/{ordinal:03}.log"))?;
        let rows = io::json(
            &member,
            Some(
                entry["samples_sha256"]
                    .as_str()
                    .ok_or("missing samples digest")?,
            ),
        )?;
        let raw = io::text(
            &log,
            Some(entry["log_sha256"].as_str().ok_or("missing log digest")?),
        )?;
        let begin = io::json(
            &io::member(output, &format!("workers/{ordinal:03}.start.json"))?,
            None,
        )?;
        let replay = baseline::worker_request(rows, raw, step, &input.calibration, context)?;
        reports.push(vm("frequency-worker",json!({"request":{"replay":replay,"step":step,"entry":entry,"begin":begin,"context":context,"last_end":last}}))?);
        last = entry["end_ns"].as_u64().ok_or("invalid frequency end")?;
    }
    Ok(
        json!({"schema":FREQUENCY,"release_approval":false,"formal_qualification":false,"candidate_attribution":null,"exclusive_api_cycles":false,"time_normalization":false,"scope":plan["scope"],"cases":plan["cohort"]["failed_cases"].as_array().ok_or("missing failed case cohort")?.len(),"observations":12*reports.len(),"workers":reports,"plan_sha256":plan_sha,"execution_sha256":execution_sha}),
    )
}
fn validate_frequency_plan(
    plan: &Value,
    cohort: &Value,
    input: &baseline::Inputs,
    context: &Value,
) -> Result<()> {
    if plan["schema"] != FREQUENCY
        || plan["release_approval"].as_bool() != Some(false)
        || !plan["candidate_attribution"].is_null()
        || plan["cohort"] != *cohort
        || plan["baseline"] != input.provenance["source"]
        || !crate::artifact::python_json::equivalent(&plan["context"], context)
        || plan["controller_sha256"] != controller_digest("vm")?
        || plan["scope"] != SCOPE
    {
        return Err("frequency plan differs".into());
    }
    let actual = env::source(tool())?;
    if env::source_at(
        tool(),
        plan["tool"]["commit"]
            .as_str()
            .ok_or("missing frequency tool commit")?,
    )? != plan["tool"]
        || ["tree", "lock_sha256"]
            .iter()
            .any(|k| actual[k] != plan["tool"][k])
    {
        return Err("frozen frequency tool differs".into());
    }
    Ok(())
}
pub(super) struct Frequency<'a> {
    capture: &'a Path,
    prior: &'a Path,
    frozen: &'a Path,
    output: &'a Path,
}
fn replay_frequency(context: &Context<'_>, paths: &Frequency<'_>) -> Result<Value> {
    if io::present(&paths.output.join("failure.json")) {
        return Err("failed frequency capture retained".into());
    }
    let (report, input, resource) =
        baseline::frequency_inputs(context, paths.capture, paths.prior, paths.frozen)?;
    let cohort = cohort(&report, &input)?;
    let plan = io::json(&paths.output.join("plan.json"), None)?;
    validate_frequency_plan(&plan, &cohort, &input, &resource)?;
    let result = frequency_result(paths.output, &plan, &input, &resource)?;
    if io::json(&paths.output.join("result.json"), None)? != result {
        return Err("frequency result differs".into());
    }
    Ok(result)
}
fn capture_frequency(
    context: &Context<'_>,
    baseline: &Path,
    group: &Path,
    paths: &Frequency<'_>,
) -> Result<Value> {
    io::directory(paths.output)?;
    let result = (|| -> Result<Value> {
        let (report, input, resource) =
            baseline::frequency_inputs(context, paths.capture, paths.prior, paths.frozen)?;
        let cohort = cohort(&report, &input)?;
        let tool_source = env::source(tool())?;
        let environment = env::capture();
        if env::source(baseline)? != input.provenance["source"]
            || release::harness(baseline)? != release::harness(tool())?
            || sampling::overlay_digest(baseline, None)? != sampling::overlay_digest(tool(), None)?
        {
            return Err("original baseline/harness/worker changed".into());
        }
        if env::context(baseline, &environment, group)? != resource {
            return Err("frequency context differs".into());
        }
        let plan = json!({"schema":FREQUENCY,"release_approval":false,"candidate_attribution":null,"baseline":input.provenance["source"],"tool":tool_source,"controller_sha256":controller_digest("vm")?,"cohort":cohort,"context":resource,"scope":SCOPE});
        io::write_json(&paths.output.join("plan.json"), &plan, true)?;
        fs::create_dir(paths.output.join("workers"))?;
        fs::create_dir(paths.output.join("counts"))?;
        {
            let cpus: Vec<u64> =
                serde_json::from_value(resource["isolation"]["cpu_affinity"].clone())?;
            let reader = MsrReader::new(&cpus)?;
            let collector = capture::Collector {
                context,
                environment: environment.clone(),
                output: paths.output,
                group,
            };
            let overlay = collector.prepare(baseline, &paths.output.join("build.log"), None)?;
            let mut ledger = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(paths.output.join("execution.jsonl"))?;
            for step in cohort["steps"]
                .as_array()
                .ok_or("missing frequency steps")?
            {
                if env::source(tool())? != tool_source
                    || env::source(baseline)? != input.provenance["source"]
                    || env::context(baseline, &environment, group)? != resource
                {
                    return Err("frequency source/context changed".into());
                }
                let ordinal = step["ordinal"].as_u64().ok_or("invalid ordinal")?;
                let case = step["case"].as_str().ok_or("missing case")?;
                let profile = step["profile"].as_str().ok_or("missing profile")?;
                let count = step["iterations"].as_u64().ok_or("missing count")?;
                fs::write(
                    paths
                        .output
                        .join("counts")
                        .join(format!("{ordinal:03}.tsv")),
                    format!("{case}\t{count}\n"),
                )?;
                let log = paths
                    .output
                    .join("workers")
                    .join(format!("{ordinal:03}.log"));
                let point = reader.snapshot()?;
                let start = io::monotonic_ns()?;
                let begin =
                    json!({"step":step,"before":resource,"start_ns":start,"counters_before":point});
                io::write_json(
                    &paths
                        .output
                        .join("workers")
                        .join(format!("{ordinal:03}.start.json")),
                    &begin,
                    true,
                )?;
                let cases = [case.to_owned()].into();
                let counts = BTreeMap::from([(case.to_owned(), count)]);
                let rows = sampling::run_profile(
                    context,
                    sampling::Run {
                        root: baseline,
                        environment: &environment,
                        profile,
                        mode: "sample",
                        log: &log,
                        cases: &cases,
                        counts: Some(&counts),
                        journal: None,
                    },
                )?;
                let end = io::monotonic_ns()?;
                let after = reader.snapshot();
                let member = paths
                    .output
                    .join("workers")
                    .join(format!("{ordinal:03}.json"));
                io::write_json(&member, &rows, true)?;
                let after = after?;
                let mut entry = begin;
                entry.as_object_mut().ok_or("invalid frequency begin")?.extend(json!({"end_ns":end,"counters_after":after,"after":env::context(baseline,&environment,group)?,"samples_sha256":io::digest(&member)?,"log_sha256":io::digest(&log)?}).as_object().ok_or("invalid frequency end")?.clone());
                io::append_json(&mut ledger, &entry)?;
                vm(
                    "frequency",
                    json!({"before":point,"after":after,"cpus":cpus,"start":start,"end":end}),
                )?;
                let replay = baseline::worker_request(
                    rows.into(),
                    io::text(&log, None)?,
                    step,
                    &input.calibration,
                    &resource,
                )?;
                sampling_model("replay-worker", json!({"request":replay}))?;
                if entry["after"] != resource {
                    return Err("frequency runtime changed".into());
                }
            }
            overlay.finish()?;
        }
        if env::source(baseline)? != input.provenance["source"]
            || env::source(tool())? != tool_source
            || env::context(baseline, &environment, group)? != resource
        {
            return Err("frequency source/context not restored".into());
        }
        let result = frequency_result(paths.output, &plan, &input, &resource)?;
        io::write_json(&paths.output.join("result.json"), &result, true)?;
        let replay = replay_frequency(context, paths)?;
        io::write_json(&paths.output.join("replay.json"), &replay, true)?;
        Ok(result)
    })();
    if let Err(error) = &result {
        io::failure(paths.output, error)?;
    }
    result
}
pub(super) fn cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("diagnose-release-vm", args)?;
    if options.has("--probe-only") && !options.has("--native-service-clock") {
        return Err("probe-only requires native-service-clock".into());
    }
    let output = options.path("--output")?;
    let result = if options.get("--frequency-from-adjacent").is_some() {
        if options.has("--native-service-clock") || options.has("--probe-only") {
            return Err("native window counters and external frequency modes are distinct".into());
        }
        let capture = options.path("--frequency-from-adjacent")?;
        let prior = options.path("--prior")?;
        let frozen = options.path("--frozen-tool")?;
        let paths = Frequency {
            capture: &capture,
            prior: &prior,
            frozen: &frozen,
            output: &output,
        };
        if options.has("--replay-frequency") {
            replay_frequency(context, &paths)?
        } else {
            capture_frequency(
                context,
                &options.path("--baseline")?,
                &options.path("--isolation-cgroup")?,
                &paths,
            )?
        }
    } else {
        if options.has("--replay-frequency")
            || options.get("--prior").is_some()
            || options.get("--frozen-tool").is_some()
        {
            return Err("frequency options require frequency-from-adjacent".into());
        }
        capture(
            context,
            &options.path("--baseline")?,
            &output,
            &options.path("--isolation-cgroup")?,
            options.has("--native-service-clock"),
            options.has("--probe-only"),
        )?
    };
    println!(
        "{}",
        json!({"release_approval":false,"formal_qualification":false,"cases":result["cases"],"observations":result["observations"]})
    );
    Ok(())
}

#[cfg(test)]
mod tests;
