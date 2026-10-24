use super::{environment as env, io::Context, overlay::Overlay, release, sampling, *};
use std::{collections::BTreeMap, fs, path::Path};
fn views_digest() -> Result<String> {
    crate::fingerprint::source(
        tool(),
        crate::fingerprint::Kind::Views,
        Some(&tool().join("xtask/src/control/inventory.rs")),
    )
}
fn inventory(operation: &str, mut request: Value) -> Result<Value> {
    request
        .as_object_mut()
        .ok_or("invalid inventory request")?
        .insert("operation".into(), operation.into());
    model(
        "diagnostic-layout",
        json!({"operation":"inventory","request":request}),
    )
}
struct Views<'a> {
    ledgers: &'a [Value],
    samples: &'a [Value],
    iterations: u64,
    page: u64,
    counters: bool,
    inventory: bool,
}
fn validate_views(input: Views<'_>) -> Result<Value> {
    if input.ledgers.len() != input.samples.len() {
        return Err("missing stream-view observations".into());
    }
    let mut lifecycle = Vec::new();
    let mut signatures = Vec::new();
    for (views, samples) in input.ledgers.chunks(4).zip(input.samples.chunks(4)) {
        let observations:Vec<_>=views.iter().zip(samples).map(|(view,row)|json!({"view":view,"case":row["case"],"iterations":input.iterations,"page_size":input.page,"observation":row["index"],"source_fixture":row["fixture"]["source"],"require_counters":input.counters})).collect();
        let request = if input.inventory {
            json!({"operation":"inventory","observations":observations})
        } else {
            json!({"operation":"validate","observations":observations,"summary":false})
        };
        let result = model("stream-views", request)?;
        if input.inventory {
            lifecycle.extend(
                result["lifecycle"]
                    .as_array()
                    .ok_or("missing lifecycle summaries")?
                    .iter()
                    .cloned(),
            );
            signatures.extend(
                result["signatures"]
                    .as_array()
                    .ok_or("missing view signatures")?
                    .iter()
                    .cloned(),
            );
        } else {
            lifecycle.extend(
                result
                    .as_array()
                    .ok_or("invalid view validation result")?
                    .iter()
                    .cloned(),
            );
        }
    }
    Ok(if input.inventory {
        json!({"lifecycle":lifecycle,"signatures":signatures})
    } else {
        lifecycle.into()
    })
}
pub(super) fn check_views(context: &Context<'_>, root: &Path) -> Result<Vec<String>> {
    let environment = env::capture();
    let overlay = Overlay::generated(context, root, sampling::render(root, Some("views"), false)?)?;
    context.run(sampling::command(
        root,
        &environment,
        &["stream_views", "--", "--nocapture"],
    )?)?;
    let cases = sampling::catalog("stream")?;
    for case in &cases {
        let mut environment = env::worker(&environment)?;
        environment.extend([
            ("RSE_SAMPLER_PROFILE".into(), "stream".into()),
            ("RSE_SAMPLER_MODE".into(), "sample".into()),
            ("RSE_SAMPLER_CASE".into(), case.clone()),
            ("RSE_SAMPLER_ITERATIONS".into(), "1".into()),
            ("RSE_SAMPLER_BATCH".into(), "0".into()),
            (
                "RSE_SAMPLER_PAGE_SIZE".into(),
                env::page_size()?.to_string(),
            ),
        ]);
        let text = context.capture(sampling::command(
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
        let samples = sampling::parsed(&text, "RSE_SAMPLE ")?;
        let views = sampling::parsed(&text, "RSE_STREAM_VIEWS ")?;
        if samples.len() != 12 || views.len() != 12 {
            return Err("incomplete native stream-view worker".into());
        }
        let underlying = model(
            "stream-views",
            json!({"operation":"underlying","records":samples}),
        )?;
        sampling_model(
            "validate-workload-sequence",
            json!({"profile":"stream","records":underlying}),
        )?;
        validate_views(Views {
            ledgers: &views,
            samples: &samples,
            iterations: 1,
            page: env::page_size()?,
            counters: cfg!(target_os = "linux"),
            inventory: false,
        })?;
        for (index, row) in samples.iter().enumerate() {
            if row["case"] != *case || row["index"].as_u64() != Some(index as u64) {
                return Err("native view worker case/order mismatch".into());
            }
            #[cfg(target_os = "linux")]
            {
                let cpus = env::affinity(0)?;
                sampling_model(
                    "validate-placement",
                    json!({"record":row,"allowed_cpus":cpus}),
                )?;
                sampling_model(
                    "validate-telemetry",
                    json!({"record":row,"allowed_cpus":cpus}),
                )?;
            }
        }
    }
    overlay.finish()?;
    println!("All 15 stream view workers executed; structural/semantic checks only");
    Ok(cases.into_iter().collect())
}
fn analyze_stream(
    phases: &Value,
    views: &Value,
    sources: &Value,
    resource: &Value,
    iterations: u64,
) -> Result<Value> {
    let mut reference = Value::Null;
    let mut lifecycle = json!({});
    let mut signatures = json!({});
    let page = resource["runtime"]["page_size"]
        .as_u64()
        .ok_or("missing page size")?;
    for phase in crate::abba::STAGES {
        let role = if phase.starts_with('A') {
            "baseline"
        } else {
            "candidate"
        };
        reference = inventory(
            "stream-phase",
            json!({"records":phases[phase],"revision":sources[role]["commit"],"cpus":resource["isolation"]["cpu_affinity"],"page_size":page,"iterations":iterations,"reference":reference}),
        )?;
        let samples: Vec<Value> = serde_json::from_value(phases[phase].clone())?;
        let ledgers: Vec<Value> = serde_json::from_value(views[phase].clone())?;
        let report = validate_views(Views {
            ledgers: &ledgers,
            samples: &samples,
            iterations,
            page,
            counters: true,
            inventory: true,
        })?;
        lifecycle[phase] = report["lifecycle"].clone();
        signatures[phase] = report["signatures"].clone();
    }
    let mut report = inventory("stream-comparison", json!({"phases":signatures}))?;
    report["lifecycle_windows"] = lifecycle;
    Ok(report)
}
pub(super) fn capture(
    context: &Context<'_>,
    baseline: &Path,
    candidate: &Path,
    output: &Path,
    group: &Path,
    stream_views: bool,
    iterations: u64,
) -> Result<Value> {
    if stream_views && !(1..=16).contains(&iterations) {
        return Err("bounded stream-view iterations must be 1..16".into());
    }
    io::directory(output)?;
    let environment = env::capture();
    let profiles = if stream_views {
        vec!["stream"]
    } else {
        vec!["small_file", "stream"]
    };
    let iterations = if stream_views { iterations } else { 1 };
    let variant = if stream_views { Some("views") } else { None };
    let roots = [("baseline", baseline), ("candidate", candidate)];
    let sources = json!({"baseline":env::source(baseline)?,"candidate":env::source(candidate)?});
    if sources["baseline"]["commit"] != BASELINE || sources["candidate"]["commit"] == BASELINE {
        return Err("published baseline and a different candidate source are required".into());
    }
    let tool_source = env::source(tool())?;
    let resource = env::context(candidate, &environment, group)?;
    let overlay_sha = sampling::overlay_digest(candidate, variant)?;
    if sampling::overlay_digest(baseline, variant)? != overlay_sha
        || release::harness(baseline)? != release::harness(candidate)?
    {
        return Err("both sources require identical harness inputs".into());
    }
    let mut metadata = json!({"schema_version":if stream_views{"stream-view-inventory-v2"}else{"fixture-layout-inventory-v2"},"sources":sources,"tool":tool_source,"context":resource,"measuring_affinity":[resource["isolation"]["cpu_affinity"][0]],"constructor_affinity":resource["isolation"]["cpu_affinity"],"protocol":crate::sampling_contract::protocol_value(),"overlay_sha256":overlay_sha,"renderer_sha256":release::implementation()?,"view_audit_sha256":if stream_views{Some(views_digest()?)}else{None},"controller_sha256":controller_digest("inventory")?,"iterations":iterations,"purpose":"structural inventory; no timing inference","candidate_attribution":null,"release_approval":false});
    io::write_json(&output.join("provenance.json"), &metadata, false)?;
    let unchanged = || -> Result<()> {
        if env::context(candidate, &environment, group)? != resource
            || env::source(tool())? != tool_source
            || roots
                .iter()
                .any(|(role, root)| !env::source(root).is_ok_and(|s| s == sources[*role]))
        {
            return Err("source or resource context changed".into());
        }
        Ok(())
    };
    let mut counts = BTreeMap::new();
    for profile in &profiles {
        let rows: BTreeMap<_, _> = sampling::catalog(profile)?
            .into_iter()
            .map(|c| (c, iterations))
            .collect();
        fs::write(
            output.join(format!("counts-{profile}.tsv")),
            rows.iter()
                .map(|(c, n)| format!("{c}\t{n}\n"))
                .collect::<String>(),
        )?;
        counts.insert(*profile, rows);
    }
    let mut phases = json!({});
    let mut observed = json!({});
    let mut guards = Vec::new();
    for (role, root) in roots {
        guards.push(Overlay::generated(
            context,
            root,
            sampling::render(root, variant, false)?,
        )?);
        context.logged(
            sampling::command(root, &environment, &["--no-run"])?,
            &output.join(format!("{role}-build.log")),
            true,
        )?;
    }
    for phase in crate::abba::STAGES {
        let root = if phase.starts_with('A') {
            baseline
        } else {
            candidate
        };
        let mut samples = Vec::new();
        let mut timings = Vec::new();
        let mut views = Vec::new();
        for profile in &profiles {
            let cases = sampling::catalog(profile)?;
            let log = output.join(format!("{phase}-{profile}.log"));
            let result = sampling::run_profile(
                context,
                sampling::Run {
                    root,
                    environment: &environment,
                    profile,
                    mode: "sample",
                    log: &log,
                    cases: &cases,
                    counts: Some(&counts[profile]),
                    journal: None,
                },
            );
            let mut partial_samples = Vec::new();
            let mut partial_timing = Vec::new();
            let mut partial_views = Vec::new();
            let mut errors = Vec::new();
            if io::present(&log) {
                let text = fs::read_to_string(&log)?;
                for (kind, prefix) in [
                    ("samples", "RSE_SAMPLE "),
                    ("timing", "RSE_TIMING "),
                    ("views", "RSE_STREAM_VIEWS "),
                ] {
                    if kind == "views" && !stream_views {
                        continue;
                    }
                    match sampling::parsed(&text, prefix) {
                        Ok(rows) => {
                            io::write_json(
                                &output.join(format!("{phase}-{profile}-{kind}.partial.json")),
                                &rows,
                                false,
                            )?;
                            match kind {
                                "samples" => partial_samples = rows,
                                "timing" => partial_timing = rows,
                                _ => partial_views = rows,
                            }
                        }
                        Err(error) => {
                            errors.push(kind);
                            io::write_json(
                                &output.join(format!("{phase}-{profile}-{kind}.error.json")),
                                &json!({"status":"failed","error":error.to_string()}),
                                false,
                            )?;
                        }
                    }
                }
                io::write_json(
                    &output.join(format!("{phase}-samples.partial.json")),
                    &json!(samples.iter().chain(&partial_samples).collect::<Vec<_>>()),
                    false,
                )?;
                io::write_json(
                    &output.join(format!("{phase}-timing.partial.json")),
                    &json!(timings.iter().chain(&partial_timing).collect::<Vec<_>>()),
                    false,
                )?;
                if stream_views {
                    io::write_json(
                        &output.join(format!("{phase}-views.partial.json")),
                        &json!(views.iter().chain(&partial_views).collect::<Vec<_>>()),
                        false,
                    )?;
                }
            }
            samples.extend(result?);
            if !errors.is_empty() {
                return Err(format!("malformed inventory journals: {}", errors.join(",")).into());
            }
            timings.extend(partial_timing);
            views.extend(partial_views);
        }
        io::write_json(
            &output.join(format!("{phase}-samples.json")),
            &samples,
            false,
        )?;
        phases[phase] = samples.into();
        if stream_views {
            io::write_json(&output.join(format!("{phase}-views.json")), &views, false)?;
        }
        observed[phase] = views.into();
        unchanged()?;
    }
    let report = if stream_views {
        analyze_stream(&phases, &observed, &sources, &resource, iterations)?
    } else {
        inventory(
            "retained",
            json!({"phases":phases,"sources":sources,"cpus":resource["isolation"]["cpu_affinity"],"page_size":resource["runtime"]["page_size"]}),
        )?
    };
    while let Some(guard) = guards.pop() {
        guard.finish()?;
    }
    unchanged()?;
    metadata
        .as_object_mut()
        .ok_or("invalid inventory envelope")?
        .extend(
            report
                .as_object()
                .ok_or("invalid inventory report")?
                .clone(),
        );
    io::write_json(&output.join("inventory.json"), &metadata, false)?;
    Ok(report)
}
pub(super) fn cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("diagnose-fixture-layout", args)?;
    if options.has("--check-dynamic") || options.get("--replay-dynamic").is_some() {
        options.only(&[
            "--check-dynamic",
            "--replay-dynamic",
            "--output",
            "--baseline-ref",
            "--iterations",
        ])?;
        if options.get("--iterations").is_some_and(|v| v != "2") {
            return Err("dynamic mode does not accept iteration overrides".into());
        }
        if options.has("--check-dynamic") == options.get("--replay-dynamic").is_some()
            || options.has("--stream-views")
            || options.has("--check-stream-views")
        {
            return Err("dynamic source-copy inventory is separate from formal capture".into());
        }
        let report = if options.get("--replay-dynamic").is_some() {
            if options.get("--output").is_some() {
                return Err("replay does not accept output".into());
            }
            super::audits::verify(
                super::audits::Kind::Dynamic,
                &options.path("--replay-dynamic")?,
                tool(),
            )?
        } else {
            super::audits::check(
                context,
                super::audits::Kind::Dynamic,
                tool(),
                &options.path("--output")?,
                options.get("--baseline-ref").unwrap_or(BASELINE),
            )?
        };
        println!("{report}");
        return Ok(());
    }
    if options.has("--check-stream-views") {
        if options.has("--stream-views")
            || [
                "--tool",
                "--baseline",
                "--candidate",
                "--output",
                "--isolation-cgroup",
            ]
            .iter()
            .any(|k| options.get(k).is_some())
        {
            return Err("check-stream-views is separate from capture".into());
        }
        let current = check_views(context, tool())?;
        if let Some(revision) = options.get("--baseline-ref") {
            if revision != BASELINE {
                return Err("baseline-ref must identify published 9.0.0".into());
            }
            let temporary = io::Temporary::new(context, "stream-views")?;
            let root = temporary.path.join("baseline");
            sampling::checkout(context, tool(), &root, revision)?;
            if current != check_views(context, &root)? {
                return Err("stream view catalogs differ between sources".into());
            }
        }
        return Ok(());
    }
    if options.path("--tool")? != fs::canonicalize(tool())? {
        return Err("tool path differs from the native controller source".into());
    }
    let report = capture(
        context,
        &options.path("--baseline")?,
        &options.path("--candidate")?,
        &options.path("--output")?,
        &options.path("--isolation-cgroup")?,
        options.has("--stream-views"),
        options.number("--iterations", 2, u64::MAX)?,
    )?;
    println!(
        "{}",
        json!({"inventory_collected":true,"layout_mismatches":report["layout_mismatches"].as_array().map_or(0,Vec::len),"release_approval":false,"candidate_attribution":null})
    );
    Ok(())
}
