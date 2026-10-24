//! Thread-owned CPU changes, bounded handshake IO, and fixed-count Slot captures.
use super::{
    environment as env,
    io::{Context, Environment},
    overlay::Overlay,
    release, sampling, *,
};
use crate::diagnostics::owned::Process;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};
const CPU_CASE: &str = "encode:10:4:524288";
const SLOT_CASE: &str = "reconstruct_shard_slot:4:2:65536";
#[derive(Default)]
struct Handshake {
    ready: usize,
    ended: usize,
    tid: Option<u64>,
    pending: Option<Value>,
}
impl Handshake {
    fn ready(
        &mut self,
        event: &Value,
        sequence: &[u64],
        cpus: &[u64],
        owned_group: bool,
        affinity: &[u64],
    ) -> Result<()> {
        if ["epoch", "cpu", "tid", "budget_before", "budget_after"]
            .into_iter()
            .any(|k| event[k].as_u64().is_none())
            || event["tid"].as_u64() == Some(0)
            || event["epoch"].as_u64() != Some(self.ready as u64)
            || self.ended != self.ready
            || self.ready >= sequence.len()
            || event["cpu"].as_u64() != Some(sequence[self.ready])
            || event["budget_before"].as_u64() != Some(cpus.len() as u64)
            || event["budget_after"].as_u64() != Some(cpus.len() as u64)
        {
            return Err("incorrect construction budget or placement handshake".into());
        }
        let tid = event["tid"].as_u64().ok_or("missing owned measuring TID")?;
        if !owned_group || self.tid.is_some_and(|prior| prior != tid) {
            return Err("thread is outside the owned worker session".into());
        }
        let previous = if self.ready == 0 {
            cpus.to_vec()
        } else {
            vec![sequence[self.ready - 1]]
        };
        if affinity != previous {
            return Err("measuring-thread affinity changed before handshake".into());
        }
        self.tid = Some(tid);
        let mut gate = event.clone();
        gate.as_object_mut().ok_or("invalid READY")?.remove("tid");
        self.pending = Some(gate);
        self.ready += 1;
        Ok(())
    }
    fn end(
        &mut self,
        event: &Value,
        sequence: &[u64],
        records: usize,
        owned_group: bool,
        affinity: &[u64],
    ) -> Result<Value> {
        if self.ended >= sequence.len()
            || event.as_object().is_none_or(|o| o.len() != 4)
            || event["epoch"].as_u64() != Some(self.ended as u64)
            || event["cpu"].as_u64() != Some(sequence[self.ended])
            || event["cached_policy_unchanged"].as_bool() != Some(true)
            || event["available_parallelism_while_pinned"].as_u64() != Some(1)
            || self.ready != self.ended + 1
            || records != self.ready * 12
            || !owned_group
            || affinity != [sequence[self.ended]]
        {
            return Err("affinity/policy changed or incomplete epoch".into());
        }
        let mut gate = self.pending.take().ok_or("missing READY gate")?;
        gate["actual_affinity"] = json!(affinity);
        self.ended += 1;
        Ok(gate)
    }
}
#[cfg(unix)]
fn cpu_worker(
    context: &Context<'_>,
    root: &Path,
    environment: &Environment,
    sequence: &[u64],
    iterations: u64,
    output: &Path,
    cpus: &[u64],
) -> Result<(Vec<Value>, Vec<Value>)> {
    use std::os::fd::AsRawFd;
    if !cfg!(target_os = "linux") {
        return Err("CPU placement capture requires Linux".into());
    }
    let mut settings = environment.clone();
    settings.extend([
        ("RSE_CPU_ITERATIONS".into(), iterations.to_string()),
        (
            "RSE_CPU_SEQUENCE".into(),
            sequence
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        (
            "RSE_SAMPLER_PAGE_SIZE".into(),
            env::page_size()?.to_string(),
        ),
    ]);
    let mut command = sampling::command(
        root,
        &settings,
        &[
            "release_cpu_placement",
            "--",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ],
    )?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut process = Process::spawn(command)?;
    let group: libc::pid_t = process.child.id().try_into()?;
    let mut samples = Vec::new();
    let mut gates = Vec::new();
    let result = (|| -> Result<()> {
        let mut stdout = process
            .child
            .stdout
            .take()
            .ok_or("missing placement output")?;
        let mut stdin = process
            .child
            .stdin
            .take()
            .ok_or("missing placement input")?;
        let mut log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        let fd = stdout.as_raw_fd();
        // SAFETY: stdout is an owned open descriptor. F_GETFL reads only descriptor flags.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: this descriptor belongs only to this worker; enable bounded, cancellable reads.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut pending = Vec::new();
        let mut state = Handshake::default();
        let mut buffer = [0u8; 65536];
        loop {
            if context.cancelled.load(Ordering::Relaxed) {
                return Err("placement worker cancelled; preserving completed observations".into());
            }
            if Instant::now() > deadline {
                return Err("placement worker exceeded its bounded runtime".into());
            }
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: poll receives one initialized writable descriptor record and a bounded timeout.
            let events = unsafe { libc::poll(&mut poll, 1, 100) };
            if events < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if events == 0 {
                continue;
            }
            let count = match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            log.write_all(&buffer[..count])?;
            log.flush()?;
            pending.extend_from_slice(&buffer[..count]);
            if pending.len() > 1024 * 1024 {
                return Err("unexpected unbounded placement output".into());
            }
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let line = String::from_utf8(pending.drain(..=end).collect())?;
                if let Some(text) = line.strip_prefix("RSE_CPU_READY ") {
                    let event = crate::artifact::python_json::parse(text)?;
                    let tid: libc::pid_t = event["tid"]
                        .as_u64()
                        .ok_or("invalid measuring TID")?
                        .try_into()?;
                    // SAFETY: getpgid is read-only; the unreaped leader anchors the owned group.
                    let owned = unsafe { libc::getpgid(tid) } == group;
                    state.ready(&event, sequence, cpus, owned, &env::affinity(tid)?)?;
                    stdin.write_all(b"G")?;
                    stdin.flush()?;
                } else if let Some(text) = line.strip_prefix("RSE_CPU_SAMPLE ") {
                    samples.push(crate::artifact::python_json::parse(text)?);
                } else if let Some(text) = line.strip_prefix("RSE_CPU_END ") {
                    let event = crate::artifact::python_json::parse(text)?;
                    let tid: libc::pid_t = state.tid.ok_or("END before READY")?.try_into()?;
                    // SAFETY: the exact ready thread remains blocked on the END acknowledgement; read only its group.
                    let owned = unsafe { libc::getpgid(tid) } == group;
                    gates.push(state.end(
                        &event,
                        sequence,
                        samples.len(),
                        owned,
                        &env::affinity(tid)?,
                    )?);
                    stdin.write_all(b"E")?;
                    stdin.flush()?;
                }
            }
        }
        if state.ready != sequence.len() || state.ended != state.ready {
            return Err("placement worker ended before its full schedule".into());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = process.poll()? {
                if !status.success() {
                    return Err(format!("placement worker failed: {status}").into());
                }
                break;
            }
            if Instant::now() > deadline {
                return Err("placement worker did not exit".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    })();
    if let Err(error) = process.stop() {
        context.unreaped.set(true);
        io::write_json(
            &output.with_extension("samples.partial.json"),
            &samples,
            false,
        )?;
        io::write_json(&output.with_extension("gates.partial.json"), &gates, false)?;
        return Err(error);
    }
    io::write_json(
        &output.with_extension("samples.partial.json"),
        &samples,
        false,
    )?;
    io::write_json(&output.with_extension("gates.partial.json"), &gates, false)?;
    result?;
    Ok((samples, gates))
}
#[cfg(not(unix))]
fn cpu_worker(
    _context: &Context<'_>,
    _root: &Path,
    _environment: &Environment,
    _sequence: &[u64],
    _iterations: u64,
    _output: &Path,
    _cpus: &[u64],
) -> Result<(Vec<Value>, Vec<Value>)> {
    Err("CPU placement requires Linux".into())
}
fn cpu_plan(cpus: &[u64]) -> Result<Vec<(String, Vec<u64>)>> {
    Ok(serde_json::from_value(model(
        "diagnostic-layout",
        json!({"operation":"cpu-plan","cpus":cpus}),
    )?)?)
}
pub(super) fn capture_cpu(
    context: &Context<'_>,
    root: &Path,
    output: &Path,
    group: &Path,
    iterations: u64,
) -> Result<Value> {
    if !(1..=1_000_000).contains(&iterations) {
        return Err("invalid diagnostic iteration count".into());
    }
    let source = env::source(root)?;
    if source["commit"] != BASELINE {
        return Err("diagnose the published 9.0.0 baseline first".into());
    }
    let tool_source = env::source(tool())?;
    let environment = env::capture();
    let resource = env::context(root, &environment, group)?;
    let cpus: Vec<u64> = serde_json::from_value(resource["isolation"]["cpu_affinity"].clone())?;
    if cpus.len() > 64 {
        return Err("placement supports at most 64 reserved CPUs".into());
    }
    io::directory(output)?;
    let files = sampling::render(root, Some("cpu-placement"), false)?;
    let plan = cpu_plan(&cpus)?;
    let mut metadata = json!({"schema_version":"cpu-placement-diagnostic-v1","source":source,"tool":tool_source,"context":resource,"protocol":crate::sampling_contract::protocol_value(),"iterations":iterations,"case":CPU_CASE,"worker_plan":plan.iter().map(|(name,sequence)|json!({"name":name,"sequence":sequence})).collect::<Vec<_>>(),"controller_sha256":controller_digest("placement")?,"template_sha256":io::digest(&tool().join("scripts/diagnostics/cpu_placement.rs"))?,"overlay_sha256":crate::fingerprint::overlay(&files,root)?,"purpose":"ordinary serial encode placement diagnosis; cached parallel decision is hypothetical, not actual dispatch","release_approval":false,"candidate_attribution":null});
    io::write_json(&output.join("provenance.json"), &metadata, false)?;
    let mut workers = Vec::new();
    {
        let overlay = Overlay::generated(context, root, files)?;
        context.logged(
            sampling::command(root, &environment, &["--no-run"])?,
            &output.join("build.log"),
            true,
        )?;
        for (name, sequence) in plan {
            let (records, gates) = cpu_worker(
                context,
                root,
                &environment,
                &sequence,
                iterations,
                &output.join(format!("{name}.log")),
                &cpus,
            )?;
            let mut detail = model(
                "diagnostic-layout",
                json!({"operation":"cpu","records":records,"sequence":sequence,"iterations":iterations,"page_size":resource["runtime"]["page_size"]}),
            )?;
            detail.as_object_mut().ok_or("invalid CPU report")?.extend(
                json!({"name":name,"sequence":sequence,"gates":gates})
                    .as_object()
                    .ok_or("invalid CPU metadata")?
                    .clone(),
            );
            workers.push(detail);
            let mut partial = metadata.clone();
            partial["workers"] = json!(workers);
            io::write_json(&output.join("diagnosis.partial.json"), &partial, false)?;
            if env::source(root)? != source
                || env::source(tool())? != tool_source
                || env::context(root, &environment, group)? != resource
            {
                return Err("source or resource context changed".into());
            }
        }
        overlay.finish()?;
    }
    let mut fresh = Vec::new();
    for cpu in cpus {
        let pair: Vec<_> = workers
            .iter()
            .filter(|w| {
                w["name"].as_str().is_some_and(|n| n.starts_with("fresh-"))
                    && w["sequence"] == json!([cpu])
            })
            .map(|w| &w["groups"][0]["median_ns_per_iter"])
            .collect();
        if pair.len() != 2 {
            return Err("both fresh-process waves are required".into());
        }
        let ratio = crate::statistics::ratio(pair[1], pair[0])?;
        fresh.push(json!({"cpu":cpu,"absolute_drift":(ratio-1.0).abs()}));
    }
    metadata["workers"] = workers.into();
    metadata["fresh_process_repeats"] = fresh.into();
    io::write_json(&output.join("diagnosis.json"), &metadata, false)?;
    Ok(metadata)
}
pub(super) fn capture_slot(
    context: &Context<'_>,
    root: &Path,
    output: &Path,
    group: &Path,
    iterations: u64,
) -> Result<Value> {
    if !(1..=1_000_000).contains(&iterations) {
        return Err("invalid bounded diagnostic iteration count".into());
    }
    io::directory(output)?;
    let environment = env::capture();
    let source = env::source(root)?;
    let tool_source = env::source(tool())?;
    if source["commit"] != BASELINE {
        return Err("diagnose the published 9.0.0 baseline first".into());
    }
    let resource = env::context(root, &environment, group)?;
    let page = resource["runtime"]["page_size"]
        .as_u64()
        .ok_or("missing page size")?;
    if page < 4096 {
        return Err("Slot diagnosis requires base pages of at least 4096 bytes".into());
    }
    let cpu = resource["isolation"]["cpu_affinity"][0]
        .as_u64()
        .ok_or("missing measuring CPU")?;
    let mut metadata = json!({"schema_version":"shard-slot-diagnostic-v3","source":source,"tool":tool_source,"context":resource,"measuring_cpu":cpu,"iterations":iterations,"protocol":crate::sampling_contract::protocol_value(),"renderer_sha256":release::implementation()?,"controller_sha256":controller_digest("placement")?,"trace_sha256":io::digest(&tool().join("scripts/diagnostics/shard_slot.rs"))?,"purpose":"within-version fixed-count diagnosis; instrumented timings are perturbed","release_approval":false,"candidate_attribution":null});
    io::write_json(&output.join("provenance.json"), &metadata, false)?;
    let mut reports = Vec::new();
    for traced in [false, true] {
        let arm = if traced { "traced" } else { "plain" };
        let files = sampling::render(root, if traced { Some("shard-slot") } else { None }, false)?;
        io::write_json(
            &output.join(format!("{arm}-overlay.json")),
            &json!({"sha256":crate::fingerprint::overlay(&files,root)?}),
            false,
        )?;
        let overlay = Overlay::generated(context, root, files)?;
        context.logged(
            sampling::command(root, &environment, &["--no-run"])?,
            &output.join(format!("{arm}-build.log")),
            true,
        )?;
        for (worker, mode) in ["calibrate", "sample", "sample", "calibrate"]
            .into_iter()
            .enumerate()
        {
            let name = format!("{arm}-{worker}-{mode}");
            let mut settings = env::worker(&environment)?;
            settings.extend([
                ("RSE_SAMPLER_CASE".into(), SLOT_CASE.into()),
                ("RSE_SAMPLER_PROFILE".into(), "small_file".into()),
                ("RSE_SAMPLER_MODE".into(), mode.into()),
                ("RSE_SAMPLER_ITERATIONS".into(), iterations.to_string()),
                (
                    "RSE_SAMPLER_BATCH".into(),
                    if mode == "calibrate" { "1" } else { "0" }.into(),
                ),
                ("RSE_SAMPLER_PAGE_SIZE".into(), page.to_string()),
            ]);
            let mut args = vec!["--cpu-list".into(), cpu.to_string(), "cargo".into()];
            args.extend(sampling::TEST_ARGS.iter().map(|s| (*s).into()));
            args.extend(
                [
                    "release_controlled_profile",
                    "--",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ]
                .map(str::to_owned),
            );
            let path = output.join(format!("{name}.log"));
            let outcome = context.logged(
                io::command(root, &env::build(root, &settings)?, "taskset", args),
                &path,
                true,
            );
            let text = fs::read_to_string(&path)?;
            let rows = sampling::parsed(&text, "RSE_SAMPLE ")?;
            let traces = sampling::parsed(&text, "RSE_SLOT_LAYOUT ")?;
            io::write_json(
                &output.join(format!("{name}-samples.partial.json")),
                &rows,
                false,
            )?;
            io::write_json(
                &output.join(format!("{name}-timing.partial.json")),
                &json!(sampling::parsed(&text, "RSE_TIMING ")?),
                false,
            )?;
            io::write_json(
                &output.join(format!("{name}-layouts.partial.json")),
                &traces,
                false,
            )?;
            outcome?;
            if mode == "calibrate" {
                sampling_model(
                    "calibration-batch",
                    json!({"records":rows,"count":iterations,"batch":1}),
                )?;
                for row in &rows {
                    sampling_model(
                        "validate-telemetry",
                        json!({"record":row,"allowed_cpus":[cpu]}),
                    )?;
                    sampling_model(
                        "validate-timing",
                        json!({"record":row,"profile":"small_file","count":iterations}),
                    )?;
                    sampling_model(
                        "validate-fixture",
                        json!({"record":row,"profile":"small_file","expected_page_size":page}),
                    )?;
                    crate::release_rows::Request {
                        profile: "small_file".into(),
                        rows: json!([row["rows"][0]]),
                        revision: BASELINE.into(),
                        iteration_counts: Some(BTreeMap::from([(SLOT_CASE.into(), iterations)])),
                        default_iterations: None,
                        complete: false,
                    }
                    .records()?;
                }
            } else {
                sampling_model(
                    "batch",
                    json!({"request":{"profile":"small_file","revision":BASELINE,"records":rows,"expected":[SLOT_CASE],"counts":{SLOT_CASE:iterations},"allowed_cpus":[cpu],"expected_page_size":page,"output":{"kind":"groups"}}}),
                )?;
            }
            let mut detail = if traced {
                model(
                    "diagnostic-layout",
                    json!({"operation":"slots","records":traces,"iterations":iterations,"page_size":page}),
                )?
            } else {
                if !traces.is_empty() {
                    return Err("plain arm was instrumented".into());
                }
                json!({})
            };
            if traced {
                for (trace, row) in traces.iter().zip(&rows) {
                    let total = trace["phase_ns"]
                        .as_object()
                        .ok_or("missing phase intervals")?
                        .values()
                        .map(|v| v.as_f64().ok_or("invalid phase interval"))
                        .collect::<std::result::Result<Vec<_>, _>>()?
                        .iter()
                        .sum::<f64>();
                    if total > row["timed_ns"].as_f64().ok_or("missing enclosing timer")? + 1.0 {
                        return Err("phase intervals exceed the enclosing diagnostic timer".into());
                    }
                }
            }
            detail.as_object_mut().ok_or("invalid Slot report")?.extend(json!({"arm":arm,"worker":worker,"mode":mode,"wall_ns":rows.iter().map(|r|r["wall_ns"].clone()).collect::<Vec<_>>(),"timed_ns":rows.iter().map(|r|r["timed_ns"].clone()).collect::<Vec<_>>()}).as_object().ok_or("invalid Slot metadata")?.clone());
            reports.push(detail);
            if env::source(root)? != source
                || env::source(tool())? != tool_source
                || env::context(root, &environment, group)? != resource
            {
                return Err("source or resource context changed".into());
            }
        }
        overlay.finish()?;
    }
    metadata["workers"] = reports.into();
    io::write_json(&output.join("diagnosis.json"), &metadata, false)?;
    Ok(metadata)
}
pub(super) fn cli(context: &Context<'_>, cpu: bool, args: Vec<String>) -> Result<()> {
    let options = Options::command(
        if cpu {
            "diagnose-cpu-placement"
        } else {
            "diagnose-shard-slot"
        },
        args,
    )?;
    if options.has("--check") {
        let variant = if cpu { "cpu-placement" } else { "shard-slot" };
        let overlay = Overlay::generated(
            context,
            tool(),
            sampling::render(tool(), Some(variant), cpu)?,
        )?;
        context.run(sampling::command(
            tool(),
            &env::capture(),
            if cpu {
                &[
                    "--",
                    "--skip",
                    "release_controlled_profile",
                    "--skip",
                    "release_cpu_placement",
                ]
            } else {
                &["slot_trace", "--", "--nocapture"]
            },
        )?)?;
        return overlay.finish();
    }
    if options.path("--tool")? != fs::canonicalize(tool())? {
        return Err("tool path differs from the native diagnostic".into());
    }
    let iterations = options.number("--iterations", if cpu { 384 } else { 4096 }, 1_000_000)?;
    let report = if cpu {
        capture_cpu(
            context,
            &options.path("--repository")?,
            &options.path("--output")?,
            &options.path("--isolation-cgroup")?,
            iterations,
        )?
    } else {
        capture_slot(
            context,
            &options.path("--repository")?,
            &options.path("--output")?,
            &options.path("--isolation-cgroup")?,
            iterations,
        )?
    };
    println!("{report}");
    Ok(())
}
#[cfg(test)]
mod tests;
