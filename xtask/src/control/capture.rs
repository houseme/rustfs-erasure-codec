//! Complete campaigns share the native worker, calibration and immutable replay paths.
use super::{
    environment as env,
    io::{Context, Environment},
    overlay::Overlay,
    release, sampling, *,
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::Path,
    time::{Duration, Instant},
};
pub(super) struct Collector<'a, 'b> {
    pub context: &'a Context<'b>,
    pub environment: Environment,
    pub output: &'a Path,
    pub group: &'a Path,
}
impl Collector<'_, '_> {
    pub fn profile(
        &self,
        root: &Path,
        folder: &Path,
        profile: &str,
        mode: &str,
        counts: Option<&BTreeMap<String, u64>>,
        journal: Option<&mut File>,
    ) -> Result<(Vec<Value>, Value)> {
        let before = env::context(root, &self.environment, self.group)?;
        let cases = sampling::catalog(profile)?;
        let log = folder.join(format!("{profile}.log"));
        let phase = folder
            .file_name()
            .and_then(|p| p.to_str())
            .ok_or("missing sampling phase")?;
        let result = sampling::run_profile(
            self.context,
            sampling::Run {
                root,
                environment: &self.environment,
                profile,
                mode,
                log: &log,
                cases: &cases,
                counts,
                journal: journal.map(|stream| sampling::Journal { stream, phase }),
            },
        );
        if result.is_err() && io::present(&log) {
            let text = fs::read_to_string(&log)?;
            for (kind, prefix) in [("samples", "RSE_SAMPLE "), ("timing", "RSE_TIMING ")] {
                match sampling::parsed(&text, prefix) {
                    Ok(rows) => io::write_json(
                        &folder.join(format!("{profile}-{kind}.partial.json")),
                        &rows,
                        false,
                    )?,
                    Err(error) => io::write_json(
                        &folder.join(format!("{profile}-{kind}.error.json")),
                        &json!({"status":"failed","error":error.to_string()}),
                        false,
                    )?,
                }
            }
        }
        let records = result?;
        let member = folder.join(format!("{profile}-samples.json"));
        io::write_json(&member, &records, false)?;
        if env::context(root, &self.environment, self.group)? != before {
            return Err("measurement environment changed".into());
        }
        let path = member
            .strip_prefix(self.output)?
            .to_str()
            .ok_or("invalid sampling member")?
            .replace('\\', "/");
        Ok((records, json!({"path":path,"sha256":io::digest(&member)?})))
    }
    pub fn prepare<'a>(
        &'a self,
        root: &Path,
        log: &Path,
        variant: Option<&str>,
    ) -> Result<Overlay<'a, 'a>> {
        let overlay =
            Overlay::generated(self.context, root, sampling::render(root, variant, false)?)?;
        self.context.logged(
            sampling::command(root, &self.environment, &["--no-run"])?,
            log,
            true,
        )?;
        Ok(overlay)
    }
    pub fn pause(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.context.cancelled.load(Ordering::Relaxed) {
                return Err("capture cancelled during phase cooldown".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }
}
pub(super) fn observations(
    profile: &str,
    records: &[Value],
    reference: Option<&Value>,
) -> Result<Value> {
    let supplied: Option<BTreeMap<String, (Value, Value)>> = reference
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?;
    let starts = crate::benchmark_contract::observations_ref(
        profile,
        records,
        supplied,
        reference.is_none(),
    )?;
    if let Some(reference) = reference {
        return Ok(reference.clone());
    }
    let mut contracts = BTreeMap::new();
    let starts: Vec<(String, usize)> = serde_json::from_value(starts)?;
    for (case, index) in starts {
        contracts.insert(
            case,
            json!([records[index]["rows"][0], records[index]["placement"]]),
        );
    }
    Ok(serde_json::to_value(contracts)?)
}
fn complete(records: &[Value]) -> Vec<Value> {
    records
        .iter()
        .filter(|r| r["complete"].as_bool() == Some(true))
        .cloned()
        .collect()
}
pub(super) fn index(profile: &str, rows: Value, revision: &str, counts: &Value) -> Result<Value> {
    let result = crate::release_rows::Request {
        profile: profile.into(),
        rows,
        revision: revision.into(),
        iteration_counts: Some(serde_json::from_value(counts.clone())?),
        default_iterations: None,
        complete: true,
    }
    .indexed()?;
    Ok(serde_json::to_value(
        result.into_iter().collect::<BTreeMap<_, _>>(),
    )?)
}
fn unchanged(
    collector: &Collector<'_, '_>,
    roots: &[(&str, &Path)],
    sources: &Value,
    tool_source: &Value,
    resource: Option<&Value>,
) -> Result<()> {
    for (role, root) in roots {
        if env::source(root)? != sources[*role] {
            return Err("source changed during capture".into());
        }
    }
    if env::source(tool())? != *tool_source {
        return Err("capture tool source changed".into());
    }
    if let Some(resource) = resource
        && env::context(
            roots.first().ok_or("missing source")?.1,
            &collector.environment,
            collector.group,
        )? != *resource
    {
        return Err("resource context changed".into());
    }
    Ok(())
}
pub(super) fn campaign(
    context: &Context<'_>,
    baseline: &Path,
    candidate: &Path,
    output: &Path,
    group: &Path,
    diagnostic: bool,
) -> Result<()> {
    io::directory(output)?;
    let collector = Collector {
        context,
        environment: env::capture(),
        output,
        group,
    };
    env::context(candidate, &collector.environment, group)?;
    let sources = json!({"baseline":env::source(baseline)?,"candidate":env::source(candidate)?});
    if sources["baseline"]["commit"] != BASELINE || sources["candidate"]["commit"] == BASELINE {
        return Err("published baseline and a different candidate are required".into());
    }
    let tool_source = env::source(tool())?;
    for key in ["tree", "lock_sha256"] {
        if tool_source[key] != sources["candidate"][key] {
            return Err("capture tool must match the frozen candidate tree/lock".into());
        }
    }
    let harness = release::harness(candidate)?;
    if release::harness(baseline)? != harness
        || sampling::overlay_digest(baseline, None)? != sampling::overlay_digest(candidate, None)?
    {
        return Err("both codecs require the same original harness and external overlay".into());
    }
    let mut contract = release::contract(candidate)?;
    let mut metadata = json!({"schema_version":4,"baseline_release":"9.0.0","phase_order":crate::abba::STAGES,"sources":sources,"harness_sha256":harness,"sampling":contract,"contexts":{},"files":{},"calibration":{}});
    if diagnostic {
        metadata["schema_version"] = release::DIAGNOSTIC.into();
        metadata["diagnostic_only"] = true.into();
        metadata["release_approval"] = false.into();
    }
    let partial = output.join(if diagnostic {
        "diagnostic-manifest.partial.json"
    } else {
        "manifest.partial.json"
    });
    io::write_json(
        &output.join("capture-provenance.json"),
        &json!({"purpose":"capture setup only; never release acceptance","sources":sources,"tool":tool_source,"capture_mode":if diagnostic{"diagnose-complete"}else{"capture"},"harness_sha256":harness,"sampling":contract,"initial_context":env::context(candidate,&collector.environment,group)?}),
        false,
    )?;
    let roots = [("baseline", baseline), ("candidate", candidate)];
    let mut journal = if diagnostic {
        Some(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output.join("worker-journal.jsonl"))?,
        )
    } else {
        None
    };
    {
        let baseline_overlay =
            collector.prepare(baseline, &output.join("baseline-build.log"), None)?;
        let candidate_overlay =
            collector.prepare(candidate, &output.join("candidate-build.log"), None)?;
        let mut pilots = json!({});
        let mut contracts = json!({});
        for (role, root) in roots {
            let folder = output.join(format!("calibration-{role}"));
            fs::create_dir(&folder)?;
            for (profile, _) in release::FILES {
                let (records, entry) = collector.profile(
                    root,
                    &folder,
                    profile,
                    "calibrate",
                    None,
                    journal.as_mut(),
                )?;
                metadata["calibration"][role][profile] = entry;
                pilots[role][profile] = sampling::calibration(
                    records.clone(),
                    profile,
                    sources[role]["commit"].as_str().ok_or("missing source")?,
                    &env::context(root, &collector.environment, group)?,
                )?["counts"]
                    .clone();
                contracts[role][profile] = observations(
                    profile,
                    &complete(&records),
                    if role == "candidate" {
                        Some(&contracts["baseline"][profile])
                    } else {
                        None
                    },
                )?;
            }
        }
        let mut counts: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        for (profile, _) in release::FILES {
            let mut selected = BTreeMap::new();
            for case in sampling::catalog(profile)? {
                selected.insert(
                    case.clone(),
                    pilots["baseline"][profile][&case]
                        .as_u64()
                        .ok_or("invalid baseline count")?
                        .max(
                            pilots["candidate"][profile][&case]
                                .as_u64()
                                .ok_or("invalid candidate count")?,
                        ),
                );
            }
            fs::write(
                output.join(format!("counts-{profile}.tsv")),
                selected
                    .iter()
                    .map(|(case, count)| format!("{case}\t{count}\n"))
                    .collect::<String>(),
            )?;
            counts.insert(profile.into(), selected);
        }
        contract["common_iterations"] = serde_json::to_value(&counts)?;
        metadata["sampling"] = contract.clone();
        for phase in crate::abba::STAGES {
            collector.pause()?;
            let (role, root) = if phase.starts_with('A') {
                roots[0]
            } else {
                roots[1]
            };
            metadata["contexts"][phase] = env::context(root, &collector.environment, group)?;
            let folder = output.join(phase);
            fs::create_dir(&folder)?;
            let mut spreads = Vec::new();
            for (profile, filename) in release::FILES {
                let (records, entry) = collector.profile(
                    root,
                    &folder,
                    profile,
                    "sample",
                    Some(&counts[profile]),
                    journal.as_mut(),
                )?;
                observations(profile, &records, Some(&contracts[role][profile]))?;
                let selected = sampling::select(
                    records,
                    profile,
                    sources[role]["commit"].as_str().ok_or("missing source")?,
                    &contract["common_iterations"][profile],
                    &Value::Null,
                    &Value::Null,
                    diagnostic,
                )?;
                for mut spread in selected["spreads"]
                    .as_array()
                    .ok_or("missing sample spreads")?
                    .clone()
                {
                    spread["profile"] = profile.into();
                    spreads.push(spread);
                }
                let member = folder.join(filename);
                io::write_json(&member, &selected["selected"], false)?;
                metadata["files"][phase][profile] = json!({"path":format!("{phase}/{filename}"),"sha256":io::digest(&member)?,"samples":entry});
                io::write_json(&partial, &metadata, false)?;
            }
            if !diagnostic
                && spreads.iter().any(|r| {
                    r["central_spread"]
                        .as_f64()
                        .is_some_and(|n| n > crate::sampling_contract::CENTRAL_SPREAD_LIMIT)
                })
            {
                return Err(release::refused(
                    "sample distributions are unstable; no later phase or candidate attribution",
                    json!({"failed_phase":phase,"sample_spreads":spreads,"candidate_attribution":null}),
                ));
            }
        }
        metadata["contexts"]["after"] = env::context(candidate, &collector.environment, group)?;
        unchanged(&collector, &roots, &sources, &tool_source, None)?;
        if !diagnostic {
            release::profiles(&metadata, output, candidate, true, false)?;
        }
        candidate_overlay.finish()?;
        baseline_overlay.finish()?;
    }
    super::layout::capture_paired(
        context,
        baseline,
        candidate,
        &output.join("layout-pair"),
        group,
        super::layout::iterations(&contract)?,
        diagnostic,
    )?;
    let name = if diagnostic {
        "diagnostic-manifest.json"
    } else {
        "manifest.json"
    };
    let layout_path = output.join("layout-pair").join(name);
    metadata["layout_supplement"] =
        json!({"path":format!("layout-pair/{name}"),"sha256":io::digest(&layout_path)?});
    metadata["contexts"]["after"] = env::context(candidate, &collector.environment, group)?;
    io::write_json(&partial, &metadata, false)?;
    if metadata["contexts"]["after"] != metadata["contexts"]["A1"] {
        return Err("resource context changed during supplemental capture".into());
    }
    unchanged(&collector, &roots, &sources, &tool_source, None)?;
    if diagnostic {
        drop(journal);
        metadata["worker_journal"] = json!({"path":"worker-journal.jsonl","sha256":io::digest(&output.join("worker-journal.jsonl"))?});
    }
    let manifest = output.join(name);
    io::write_json(&manifest, &metadata, false)?;
    let (_, report) = release::verify(&manifest, candidate, true, diagnostic)?;
    io::write_json(&output.join("report.json"), &report, false)
}
pub(super) fn qualify(
    context: &Context<'_>,
    baseline: &Path,
    output: &Path,
    group: &Path,
) -> Result<Value> {
    io::directory(output)?;
    let collector = Collector {
        context,
        environment: env::capture(),
        output,
        group,
    };
    let source = env::source(baseline)?;
    if source["commit"] != BASELINE {
        return Err("qualification requires published 9.0.0".into());
    }
    let tool_source = env::source(tool())?;
    if release::harness(baseline)? != release::harness(tool())?
        || sampling::overlay_digest(baseline, None)? != sampling::overlay_digest(tool(), None)?
    {
        return Err("qualification requires the same original harness and external overlay".into());
    }
    let resource = env::context(baseline, &collector.environment, group)?;
    let mut report = json!({"schema_version":"baseline-qualification-v2","source":source,"tool":tool_source,"context":resource,"sampling":release::contract(baseline)?,"harness_sha256":release::harness(baseline)?,"calibration":{},"phases":{},"repeat_checks":[],"baseline_controls_passed":false,"release_approval":false,"candidate_attribution":null});
    io::write_json(&output.join("provenance.json"), &report, false)?;
    let mut counts = json!({});
    let mut prepared = json!({});
    let mut calibrated = json!({});
    let mut contracts = json!({});
    let mut medians = json!({});
    let roots = [("baseline", baseline)];
    let sources = json!({"baseline":source});
    {
        let overlay = collector.prepare(baseline, &output.join("baseline-build.log"), None)?;
        let folder = output.join("calibration");
        fs::create_dir(&folder)?;
        for (profile, _) in release::FILES {
            let (records, entry) =
                collector.profile(baseline, &folder, profile, "calibrate", None, None)?;
            let analysis = sampling::calibration(records.clone(), profile, BASELINE, &resource)?;
            counts[profile] = analysis["counts"].clone();
            prepared[profile] = analysis["preparation"].clone();
            let final_rows: BTreeMap<_, _> = records
                .iter()
                .map(|r| {
                    Ok((
                        r["case"]
                            .as_str()
                            .ok_or("missing calibration case")?
                            .to_owned(),
                        r["rows"][0].clone(),
                    ))
                })
                .collect::<Result<_>>()?;
            calibrated[profile] = index(
                profile,
                final_rows.into_values().collect::<Vec<_>>().into(),
                BASELINE,
                &counts[profile],
            )?;
            contracts[profile] = observations(profile, &complete(&records), None)?;
            report["calibration"][profile] = entry;
            let selected: BTreeMap<String, u64> = serde_json::from_value(counts[profile].clone())?;
            fs::write(
                output.join(format!("counts-{profile}.tsv")),
                selected
                    .iter()
                    .map(|(c, n)| format!("{c}\t{n}\n"))
                    .collect::<String>(),
            )?;
            unchanged(&collector, &roots, &sources, &tool_source, Some(&resource))?;
            io::write_json(&output.join("qualification.partial.json"), &report, false)?;
        }
        report["sampling"]["common_iterations"] = counts.clone();
        for phase in ["A1", "A2"] {
            collector.pause()?;
            let folder = output.join(phase);
            fs::create_dir(&folder)?;
            for (profile, _) in release::FILES {
                let selected_counts = serde_json::from_value(counts[profile].clone())?;
                let (records, entry) = collector.profile(
                    baseline,
                    &folder,
                    profile,
                    "sample",
                    Some(&selected_counts),
                    None,
                )?;
                observations(profile, &records, Some(&contracts[profile]))?;
                let service = crate::diagnostics::baseline::service_times(&records)?;
                let selected = sampling::select(
                    records,
                    profile,
                    BASELINE,
                    &counts[profile],
                    &resource["isolation"]["cpu_affinity"],
                    &resource["runtime"]["page_size"],
                    false,
                )?;
                if selected["preparation"] != prepared[profile] {
                    return Err(
                        "caller layout, preparation or input replay differs from calibration"
                            .into(),
                    );
                }
                medians[phase][profile] = index(
                    profile,
                    selected["selected"].clone(),
                    BASELINE,
                    &counts[profile],
                )?;
                crate::benchmark_contract::compatible(
                    profile,
                    &serde_json::from_value(calibrated[profile].clone())?,
                    &serde_json::from_value(medians[phase][profile].clone())?,
                )?;
                report["phases"][phase][profile] = json!({"samples":entry,"case_count":selected["selected"].as_array().ok_or("missing selected rows")?.len(),"sample_spreads":selected["spreads"],"service_times":service});
                unchanged(&collector, &roots, &sources, &tool_source, Some(&resource))?;
                io::write_json(&output.join("qualification.partial.json"), &report, false)?;
            }
            if report["phases"][phase]
                .as_object()
                .ok_or("missing phase")?
                .values()
                .flat_map(|v| v["sample_spreads"].as_array().into_iter().flatten())
                .any(|w| {
                    w["central_spread"]
                        .as_f64()
                        .is_some_and(|n| n > crate::sampling_contract::CENTRAL_SPREAD_LIMIT)
                })
            {
                return Err(release::refused(
                    "baseline distributions are unstable; stop without candidate attribution",
                    report,
                ));
            }
        }
        let mut repeats = Vec::new();
        for (profile, _) in release::FILES {
            let a: BTreeMap<String, Value> =
                serde_json::from_value(medians["A1"][profile].clone())?;
            let b: BTreeMap<String, Value> =
                serde_json::from_value(medians["A2"][profile].clone())?;
            crate::benchmark_contract::compatible(profile, &a, &b)?;
            let metric = crate::release_rows::metric(profile)?;
            for (case, row) in a {
                let first = row[metric].as_f64().ok_or("missing first metric")?;
                let second = b[&case][metric].as_f64().ok_or("missing second metric")?;
                let drift = (second - first).abs() / first;
                if !drift.is_finite() {
                    return Err("nonfinite qualification drift".into());
                }
                repeats.push(json!({"profile":profile,"case":case,"first":first,"second":second,"metric":metric,"drift":drift}));
            }
        }
        report["repeat_checks"] = repeats.into();
        unchanged(&collector, &roots, &sources, &tool_source, Some(&resource))?;
        io::write_json(&output.join("qualification.partial.json"), &report, false)?;
        if report["repeat_checks"]
            .as_array()
            .ok_or("missing repeat checks")?
            .iter()
            .any(|r| {
                r["drift"]
                    .as_f64()
                    .is_some_and(|n| n > crate::sampling_contract::REPEAT_DRIFT_LIMIT)
            })
        {
            return Err(release::refused(
                "baseline drift exceeds 5%; no candidate attribution",
                report,
            ));
        }
        overlay.finish()?;
    }
    unchanged(&collector, &roots, &sources, &tool_source, Some(&resource))?;
    report["baseline_controls_passed"] = true.into();
    io::write_json(&output.join("qualification.json"), &report, false)?;
    Ok(report)
}
