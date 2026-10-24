//! One source-copy transaction and original-member replay for three observer families.
use super::{
    environment as env,
    io::{Context, Environment},
    overlay::Overlay,
    release, sampling, *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    path::Path,
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Verify,
    Stream,
    Dynamic,
}
impl Kind {
    fn variant(self) -> &'static str {
        match self {
            Self::Verify => "verify",
            Self::Stream => "stream",
            Self::Dynamic => "dynamic",
        }
    }
    fn schema(self) -> &'static str {
        match self {
            Self::Verify => "verify-scratch",
            Self::Stream => "stream-core",
            Self::Dynamic => "dynamic-buffer",
        }
    }
    fn plan(self) -> &'static str {
        match self {
            Self::Verify => "verify-scratch",
            Self::Stream => "stream-core",
            Self::Dynamic => "dynamic",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Verify => "RSE_VERIFY_SCRATCH ",
            Self::Stream => "RSE_STREAM_CORE ",
            Self::Dynamic => "RSE_DYNAMIC_BUFFERS ",
        }
    }
    fn names(self) -> Vec<&'static str> {
        match self {
            Self::Verify => vec!["src/lib.rs", "src/core/verify.rs"],
            Self::Stream => vec!["src/lib.rs", "src/core/stream.rs", "src/core/verify.rs"],
            Self::Dynamic => vec![
                "src/lib.rs",
                "src/matrix.rs",
                "src/core/reconstruct.rs",
                "src/galois_8/policy.rs",
                "src/core/leopard_gf8/work.rs",
                "src/core/leopard_gf8/decode.rs",
                "src/core/leopard_gf16/work.rs",
                "src/core/leopard_gf16/decode.rs",
            ],
        }
    }
    fn native(self) -> Result<String> {
        crate::diagnostics::templates::native(self.variant(), tool())
    }
    fn steps(self) -> Result<Vec<Value>> {
        if self == Self::Dynamic {
            return Ok(serde_json::from_value(
                model("diagnostic-ledger", json!({"kind":"dynamic-catalog"}))?["steps"].clone(),
            )?);
        }
        let mut steps = Vec::new();
        for role in ["baseline", "candidate"] {
            for profile in if self == Self::Stream {
                vec!["stream"]
            } else {
                vec!["smoke", "small_file"]
            } {
                for case in sampling::catalog(profile)? {
                    if self == Self::Verify && !case.starts_with("verify:") {
                        continue;
                    }
                    steps.push(json!({"role":role,"mode":"sample","profile":profile,"case":case}));
                }
            }
        }
        Ok(steps)
    }
    fn data_key(self) -> &'static str {
        match self {
            Self::Verify => "scratch",
            Self::Stream => "core",
            Self::Dynamic => "dynamic",
        }
    }
}
pub(super) fn original(
    kind: Kind,
    repository: &Path,
    source: &Value,
) -> Result<BTreeMap<String, String>> {
    kind.names()
        .into_iter()
        .map(|name| {
            Ok((
                name.into(),
                String::from_utf8(io::git_bytes(
                    repository,
                    &format!(
                        "{}:{name}",
                        source["commit"].as_str().ok_or("missing original commit")?
                    ),
                )?)?
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
            ))
        })
        .collect()
}
pub(super) fn source_plan(kind: Kind, original: &BTreeMap<String, String>) -> Result<Value> {
    crate::bridge::evaluate(
        json!({"kind":"source-plan","request":{"variant":kind.plan(),"original":original,"tool":tool()}}),
    )
}
pub(super) fn native_digest() -> Result<String> {
    crate::fingerprint::source(tool(), crate::fingerprint::Kind::Native, None)
}
pub(super) fn dynamic_context(root: &Path, environment: &Environment) -> Result<Value> {
    let cpus = if cfg!(target_os = "linux") {
        Some(env::affinity(0)?)
    } else {
        None
    };
    let machine = io::output(root, "uname", &["-m"])?;
    let (identity, policy, libc) = if let Some(cpus) = &cpus {
        let runtime = env::runtime_context(environment)?;
        (
            crate::runtime::cpu_identity(&fs::read_to_string("/proc/cpuinfo")?, &machine, cpus)?,
            runtime["kernel_memory_policy"].clone(),
            json!([runtime["libc"]["name"], runtime["libc"]["version"]]),
        )
    } else {
        (machine.clone(), Value::Null, json!(["", ""]))
    };
    let allocator: BTreeMap<_, _> = environment
        .iter()
        .filter(|(k, _)| crate::runtime::allocator_key(k))
        .collect();
    Ok(
        json!({"compiler":env::compiler(root)?,"machine":machine,"cpu_identity":identity,"cpu_affinity":cpus,"page_size":env::page_size()?,"kernel_memory_policy":policy,"libc":libc,"allocator_environment":allocator,"settings":crate::sampling_contract::protocol::sampling_environment()}),
    )
}
fn name(step: &Value) -> Result<String> {
    let role = step["role"].as_str().ok_or("missing role")?;
    let profile = step["profile"].as_str().ok_or("missing profile")?;
    let case = step["case"]
        .as_str()
        .ok_or("missing case")?
        .replace(':', "_");
    Ok(
        if step["mode"] == "structure"
            || step
                .get("mode")
                .is_some_and(|_| step.get("dynamic") == Some(&Value::Bool(true)))
        {
            format!(
                "{role}-{}-{profile}-{case}",
                step["mode"].as_str().ok_or("missing mode")?
            )
        } else {
            format!("{role}-{profile}-{case}")
        },
    )
}
fn key(kind: Kind, step: &Value) -> Result<String> {
    let profile = step["profile"].as_str().ok_or("missing profile")?;
    let case = step["case"].as_str().ok_or("missing case")?;
    Ok(match kind {
        Kind::Verify => format!("{profile}/{case}"),
        Kind::Stream => case.into(),
        Kind::Dynamic => format!(
            "{}/{profile}/{case}",
            step["mode"].as_str().ok_or("missing mode")?
        ),
    })
}
fn worker_name(kind: Kind, step: &Value) -> Result<String> {
    if kind == Kind::Dynamic {
        Ok(format!(
            "{}-{}-{}-{}",
            step["role"].as_str().ok_or("missing role")?,
            step["mode"].as_str().ok_or("missing mode")?,
            step["profile"].as_str().ok_or("missing profile")?,
            step["case"]
                .as_str()
                .ok_or("missing case")?
                .replace(':', "_")
        ))
    } else {
        name(step)
    }
}
struct Observation<'a> {
    kind: Kind,
    data: &'a Value,
    step: &'a Value,
    source: &'a Value,
    page: &'a Value,
    cpus: &'a Value,
    current: bool,
}
fn worker(
    observation: Observation<'_>,
    journal: Option<Value>,
    reference: Option<&Value>,
) -> Result<Value> {
    let Observation {
        kind,
        data,
        step,
        source,
        page,
        cpus,
        current,
    } = observation;
    let case = &step["case"];
    if kind == Kind::Dynamic && step["profile"] == "decoder" {
        return model(
            "diagnostic-ledger",
            json!({"kind":"dynamic-supplement","header":data["supplement"],"data":data["dynamic"],"case":case,"page":page,"cpus":cpus,"legacy":source["commit"]==BASELINE,"journal":journal}),
        );
    }
    let payload = match kind {
        Kind::Verify => json!({"kind":"verify-scratch","ledger":data["scratch"]}),
        Kind::Stream => {
            json!({"kind":"stream-core","ledger":data["core"],"views":data["views"],"current":current})
        }
        Kind::Dynamic => {
            json!({"kind":"dynamic","ledger":data["dynamic"],"legacy":source["commit"]==BASELINE})
        }
    };
    model(
        "diagnostic-worker",
        json!({"rows":data["samples"],"profile":step["profile"],"case":case,"revision":source["commit"],"page":page,"cpus":cpus,"mode":step["mode"],"journal":journal,"reference":reference,"data":payload}),
    )
}
fn numerical(kind: Kind, raw: &str, step: &Value) -> Result<Value> {
    let ledgers = sampling::parsed(raw, kind.prefix())?;
    if ledgers.len() != 1 {
        return Err("missing complete original observer ledger".into());
    }
    let rows = sampling::parsed(raw, "RSE_SAMPLE ")?;
    let mut data = json!({"samples":rows});
    data[kind.data_key()] = ledgers.into_iter().next().ok_or("missing observer")?;
    if kind == Kind::Stream {
        data["views"] = sampling::parsed(raw, "RSE_STREAM_VIEWS ")?.into();
    }
    if kind == Kind::Dynamic && step["profile"] == "decoder" {
        let headers = sampling::parsed(raw, "RSE_DYNAMIC_SUPPLEMENT ")?;
        if headers.len() != 1 {
            return Err("missing decoder supplement header".into());
        }
        data.as_object_mut()
            .ok_or("invalid supplement")?
            .remove("samples");
        data["supplement"] = headers.into_iter().next().ok_or("missing supplement")?;
    }
    Ok(data)
}
fn numeric_log(kind: Kind, raw: &str) -> String {
    raw.split_inclusive('\n')
        .filter(|line| {
            [
                "RSE_TIMING ",
                "RSE_SAMPLE ",
                "RSE_STREAM_VIEWS ",
                "RSE_DYNAMIC_SUPPLEMENT ",
                kind.prefix(),
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        })
        .collect()
}
fn same_summary(entry: &Value, summary: &Value) -> Result<()> {
    if summary
        .as_object()
        .ok_or("invalid worker summary")?
        .iter()
        .any(|(k, v)| {
            !entry
                .get(k)
                .is_some_and(|actual| crate::artifact::python_json::equivalent(actual, v))
        })
    {
        return Err("diagnostic summary differs from original rows".into());
    }
    Ok(())
}
fn shapes(kind: Kind, data: &Value) -> Result<BTreeSet<String>> {
    let events = data[kind.data_key()]["events"]
        .as_array()
        .ok_or("missing raw source events")?;
    let mut shapes = BTreeSet::new();
    for event in events {
        if kind == Kind::Dynamic {
            if event["kind"] != "view" {
                continue;
            }
            let shape = if event["shape"].is_null() {
                json!([])
            } else {
                event["shape"].clone()
            };
            let offsets = if event["offsets"].is_null() {
                json!([])
            } else {
                event["offsets"].clone()
            };
            shapes.insert(serde_json::to_string(&json!([
                event["role"],
                event["stage"],
                event["index"],
                event["len"],
                event["capacity"],
                event["storage"],
                event["element_size"],
                shape,
                event["occupied"],
                event["present"],
                offsets
            ]))?);
        } else if kind == Kind::Verify {
            shapes.insert(serde_json::to_string(&json!([
                event["phase"],
                event["len"],
                event["capacity"],
                event["offsets"],
                event["inline"]
            ]))?);
        }
    }
    Ok(shapes)
}
fn source_unchanged(root: &Path, plan: &Value, names: &[&str]) -> Result<()> {
    let files = plan["files"].as_object().ok_or("invalid source plan")?;
    for (name, text) in files {
        if !fs::symlink_metadata(root.join(name))?.is_file()
            || fs::read_to_string(root.join(name))?
                != text.as_str().ok_or("invalid planned source")?
        {
            return Err("diagnostic source changed during observation".into());
        }
    }
    let names: BTreeSet<_> = names.iter().copied().collect();
    let changed = io::output(root, "git", &["diff", "--name-only"])?;
    if changed.lines().collect::<BTreeSet<_>>() != names {
        return Err("foreign tracked diagnostic source change".into());
    }
    Ok(())
}
pub(super) fn check(
    context: &Context<'_>,
    kind: Kind,
    repository: &Path,
    output: &Path,
    baseline: &str,
) -> Result<Value> {
    if kind == Kind::Dynamic && output.starts_with(repository) {
        return Err("keep dynamic raw evidence outside the source repository".into());
    }
    io::directory(output)?;
    let result = (|| -> Result<Value> {
        let tool_source = env::source(repository)?;
        let old = env::source_at(repository, baseline)?;
        if old["commit"] != BASELINE || tool_source["commit"] == BASELINE {
            return Err("require published 9.0.0 and a distinct candidate before any build".into());
        }
        let environment = env::capture();
        let dynamic = dynamic_context(repository, &environment)?;
        let page = dynamic["page_size"].clone();
        let cpus = dynamic["cpu_affinity"].clone();
        let mut report = json!({"schema":format!("{}-check-v1",kind.schema()),"release_approval":false,"candidate_attribution":null,"formal_performance_geometry_equivalence":false,"sources":{},"cases":{},"source_overlays":{},"harness_sha256":{},"overlay_sha256":{},"sampler_implementation_sha256":release::implementation()?,"controller_sha256":controller_digest("audits")?,"native_sha256":crate::artifact::snapshot::digest(kind.native()?.as_bytes()),"page_size":page,"cpu_affinity":cpus});
        if kind == Kind::Dynamic {
            report["tool"] = tool_source.clone();
            report["context"] = dynamic.clone();
        } else {
            report["source_copies"] = true.into();
            if kind == Kind::Stream {
                report["source_patcher_sha256"] = native_digest()?.into();
            }
        }
        let mut roots = BTreeMap::new();
        let mut plans = BTreeMap::new();
        for (role, source) in [("candidate", &tool_source), ("baseline", &old)] {
            let root = output.join(role);
            sampling::checkout(
                context,
                repository,
                &root,
                source["commit"].as_str().ok_or("missing source revision")?,
            )?;
            let actual = env::source(&root)?;
            report["sources"][role] = actual.clone();
            report["cases"][role] = json!({});
            let original = original(kind, repository, &actual)?;
            let plan = source_plan(kind, &original)?;
            report["source_overlays"][role] = plan["bindings"].clone();
            report["harness_sha256"][role] = release::harness(&root)?.into();
            report["overlay_sha256"][role] =
                sampling::overlay_digest(&root, Some(kind.variant()))?.into();
            plans.insert(role, (original, plan));
            roots.insert(role, root);
        }
        if report["harness_sha256"]["baseline"] != report["harness_sha256"]["candidate"]
            || report["overlay_sha256"]["baseline"] != report["overlay_sha256"]["candidate"]
        {
            return Err(
                "source copies require identical original harness and diagnostic workers".into(),
            );
        }
        let steps = kind.steps()?;
        if kind == Kind::Dynamic {
            let mut plan = report.clone();
            plan["steps"] = json!(steps);
            io::write_json(&output.join("plan.json"), &plan, false)?;
        }
        let mut execution = if kind == Kind::Dynamic {
            Some(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(output.join("execution.jsonl"))?,
            )
        } else {
            None
        };
        for role in ["candidate", "baseline"] {
            let root = roots.remove(role).ok_or("missing private source copy")?;
            let source = report["sources"][role].clone();
            let (original, plan) = plans.remove(role).ok_or("missing source plan")?;
            let current = original
                .get("src/core/stream.rs")
                .is_some_and(|text| text.contains("struct StreamReadBuffer {"));
            let files = sampling::render(&root, Some(kind.variant()), false)?;
            let patch = Overlay::source(
                context,
                &root,
                original,
                serde_json::from_value(plan["files"].clone())?,
            )?;
            let overlay = Overlay::generated(context, &root, files)?;
            if kind == Kind::Dynamic {
                source_unchanged(&root, &plan, &kind.names())?;
            }
            context.logged(
                sampling::command(&root, &environment, &["--no-run"])?,
                &output.join(format!("{role}-build.log")),
                true,
            )?;
            if kind == Kind::Dynamic {
                let mut controls = environment.clone();
                controls.insert(
                    "RSE_DYNAMIC_CURRENT_SOURCE".into(),
                    if role == "baseline" { "0" } else { "1" }.into(),
                );
                context.logged(sampling::command(&root,&controls,&["dynamic_semantics::active_observer_preserves_normal_selective_invalid_and_late_failure_semantics","--","--exact","--nocapture","--test-threads=1"])?,&output.join(format!("{role}-semantics.log")),true)?;
            }
            for step in steps.iter().filter(|s| s["role"] == role) {
                if kind == Kind::Dynamic {
                    source_unchanged(&root, &plan, &kind.names())?;
                    if env::source(repository)? != tool_source
                        || dynamic_context(repository, &environment)? != dynamic
                    {
                        return Err("dynamic tool or runtime context changed".into());
                    }
                }
                if let Some(execution) = &mut execution {
                    io::append_json(
                        execution,
                        &json!({"stage":"begin","step":step,"context":dynamic}),
                    )?;
                }
                let name = worker_name(kind, step)?;
                let log = output.join(format!("{name}.log"));
                let numeric = output.join(format!("{name}-journal.jsonl"));
                let mut settings = env::worker(&environment)?;
                settings.extend([
                    (
                        "RSE_SAMPLER_PROFILE".into(),
                        step["profile"].as_str().ok_or("missing profile")?.into(),
                    ),
                    (
                        "RSE_SAMPLER_MODE".into(),
                        step["mode"].as_str().ok_or("missing mode")?.into(),
                    ),
                    (
                        "RSE_SAMPLER_CASE".into(),
                        step["case"].as_str().ok_or("missing case")?.into(),
                    ),
                    ("RSE_SAMPLER_ITERATIONS".into(), "2".into()),
                    (
                        "RSE_SAMPLER_BATCH".into(),
                        if step["mode"] == "calibrate" {
                            "1"
                        } else {
                            "0"
                        }
                        .into(),
                    ),
                    (
                        "RSE_SAMPLER_PAGE_SIZE".into(),
                        page.as_u64().ok_or("missing page size")?.to_string(),
                    ),
                ]);
                if step["profile"] == "decoder" {
                    settings.insert(
                        "RSE_DYNAMIC_DECODER_CASE".into(),
                        step["case"].as_str().ok_or("missing decoder case")?.into(),
                    );
                }
                let test = if step["profile"] == "decoder" {
                    "release_dynamic_decoder_profile"
                } else {
                    "release_controlled_profile"
                };
                let outcome = context.logged(
                    sampling::command(
                        &root,
                        &settings,
                        &[
                            test,
                            "--",
                            "--exact",
                            "--ignored",
                            "--nocapture",
                            "--test-threads=1",
                        ],
                    )?,
                    &log,
                    true,
                );
                if io::present(&log) {
                    fs::write(&numeric, numeric_log(kind, &fs::read_to_string(&log)?))?;
                }
                outcome?;
                let raw = fs::read_to_string(&numeric)?;
                let data = match numerical(kind, &raw, step) {
                    Ok(data) => data,
                    Err(error) => {
                        io::write_json(
                            &output.join(format!("{name}.partial.json")),
                            &json!({"samples":sampling::parsed(&raw,"RSE_SAMPLE ")?,"observations":sampling::parsed(&raw,kind.prefix())?}),
                            false,
                        )?;
                        return Err(error);
                    }
                };
                let member = output.join(format!("{name}.json"));
                io::write_json(&member, &data, false)?;
                let analysis = worker(
                    Observation {
                        kind,
                        data: &data,
                        step,
                        source: &source,
                        page: &page,
                        cpus: &cpus,
                        current,
                    },
                    None,
                    None,
                )?;
                let mut entry = if step["profile"] == "decoder" {
                    analysis
                } else {
                    analysis["summary"].clone()
                };
                entry
                    .as_object_mut()
                    .ok_or("invalid worker summary")?
                    .extend(
                        declaration(&name, &member, &numeric, &log)?
                            .as_object()
                            .ok_or("invalid worker declaration")?
                            .clone(),
                    );
                let key = key(kind, step)?;
                report["cases"][role][key] = entry.clone();
                if let Some(execution) = &mut execution {
                    io::append_json(
                        execution,
                        &json!({"stage":"end","step":step,"context":dynamic,"entry":entry}),
                    )?;
                }
            }
            overlay.finish()?;
            patch.finish()?;
            if env::source(&root)? != report["sources"][role] {
                return Err("original diagnostic source/lock was not restored".into());
            }
        }
        if kind == Kind::Dynamic
            && (env::source(repository)? != tool_source
                || dynamic_context(repository, &environment)? != dynamic)
        {
            return Err("dynamic source/context changed before replay".into());
        }
        io::write_json(&output.join("result.json"), &report, false)?;
        let replay = verify(kind, output, repository)?;
        io::write_json(&output.join("replay.json"), &replay, false)?;
        Ok(if kind == Kind::Dynamic {
            replay
        } else {
            report
        })
    })();
    if let Err(error) = &result {
        io::failure(output, error)?;
    }
    result
}
pub(super) fn verify(kind: Kind, output: &Path, repository: &Path) -> Result<Value> {
    if ["failure.json", "diagnostic-status.json"]
        .iter()
        .any(|n| io::present(&output.join(n)))
    {
        return Err("failed diagnostic is preserved, not accepted".into());
    }
    let report = io::json(&output.join("result.json"), None)?;
    if report["schema"] != format!("{}-check-v1", kind.schema())
        || report["release_approval"].as_bool() != Some(false)
        || !report["candidate_attribution"].is_null()
        || report["formal_performance_geometry_equivalence"].as_bool() != Some(false)
        || (kind != Kind::Dynamic && report["source_copies"].as_bool() != Some(true))
    {
        return Err("invalid diagnostic-only source report".into());
    }
    if report["native_sha256"] != crate::artifact::snapshot::digest(kind.native()?.as_bytes())
        || report["sampler_implementation_sha256"] != release::implementation()?
        || !report["controller_sha256"]
            .as_str()
            .is_some_and(io::digest_valid)
        || (kind == Kind::Dynamic && report["controller_sha256"] != controller_digest("audits")?)
        || (kind == Kind::Stream && report["source_patcher_sha256"] != native_digest()?)
    {
        return Err("diagnostic implementation differs".into());
    }
    let page = &report["page_size"];
    if !page
        .as_u64()
        .is_some_and(crate::sampling_contract::valid_page_size)
    {
        return Err("invalid diagnostic page size".into());
    }
    let cpus = &report["cpu_affinity"];
    if !cpus.is_null() {
        let cpus: Vec<u64> = serde_json::from_value(cpus.clone())?;
        if cpus.is_empty() || cpus.windows(2).any(|p| p[0] >= p[1]) {
            return Err("invalid native affinity".into());
        }
    }
    if kind == Kind::Dynamic {
        let sources = model(
            "diagnostic-ledger",
            json!({"kind":"dynamic-envelope","request":{"report":report,"plan":io::json(&output.join("plan.json"),None)?,"execution":io::text(&output.join("execution.jsonl"),None)?}}),
        )?;
        for source in sources
            .as_object()
            .ok_or("invalid source envelope")?
            .values()
        {
            if env::source_at(
                repository,
                source["commit"].as_str().ok_or("missing source")?,
            )? != *source
            {
                return Err("original dynamic source/tool differs".into());
            }
        }
    }
    let expected_roles: BTreeSet<_> = ["baseline", "candidate"].into();
    for field in ["sources", "cases"] {
        if report[field]
            .as_object()
            .ok_or("missing both sources")?
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected_roles
        {
            return Err("both original diagnostic sources required".into());
        }
    }
    if report["sources"]["baseline"]["commit"] != BASELINE
        || report["sources"]["candidate"]["commit"] == BASELINE
    {
        return Err("wrong published baseline or candidate".into());
    }
    let steps = kind.steps()?;
    let expected: BTreeSet<_> = steps
        .iter()
        .filter(|s| s["role"] == "candidate")
        .map(|s| key(kind, s))
        .collect::<Result<_>>()?;
    let mut contracts = BTreeMap::new();
    let mut workloads = BTreeMap::new();
    let mut traces = BTreeMap::new();
    let mut members = BTreeMap::new();
    let mut geometry: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let (mut workers, mut observations, mut iterations, mut events) = (0_u64, 0_u64, 0_u64, 0_u64);
    for role in ["baseline", "candidate"] {
        let source = &report["sources"][role];
        if env::source_at(
            repository,
            source["commit"].as_str().ok_or("missing original source")?,
        )? != *source
        {
            return Err("original source identity differs".into());
        }
        let original = original(kind, repository, source)?;
        let current = original
            .get("src/core/stream.rs")
            .is_some_and(|s| s.contains("struct StreamReadBuffer {"));
        if report["source_overlays"][role] != source_plan(kind, &original)?["bindings"]
            || report["harness_sha256"][role] != release::harness(repository)?
            || report["overlay_sha256"][role]
                != sampling::overlay_digest(repository, Some(kind.variant()))?
        {
            return Err("original diagnostic source overlay, worker or harness differs".into());
        }
        let cases = report["cases"][role]
            .as_object()
            .ok_or("missing original diagnostic catalog")?;
        if cases.keys().cloned().collect::<BTreeSet<_>>() != expected {
            return Err("incomplete original diagnostic catalog".into());
        }
        for step in steps.iter().filter(|s| s["role"] == role) {
            let key = key(kind, step)?;
            let entry = &cases[&key];
            let name = worker_name(kind, step)?;
            let member_name = format!("{name}.json");
            let numeric_name = format!("{name}-journal.jsonl");
            if entry["path"] != member_name || entry["numeric_journal_path"] != numeric_name {
                return Err("invalid diagnostic member path".into());
            }
            let data = io::json(
                &io::member(output, &member_name)?,
                Some(entry["sha256"].as_str().ok_or("missing member digest")?),
            )?;
            let numeric = io::member(output, &numeric_name)?;
            let log = output.join(format!("{name}.log"));
            let log_sha = entry["raw_log_sha256"]
                .as_str()
                .filter(|s| io::digest_valid(s))
                .ok_or("missing private log binding")?;
            if io::present(&log)
                && (!fs::symlink_metadata(&log)?.is_file() || io::digest(&log)? != log_sha)
            {
                return Err("private original log differs".into());
            }
            let expected_keys: BTreeSet<_> =
                if kind == Kind::Dynamic && step["profile"] == "decoder" {
                    vec!["supplement", "dynamic"]
                } else if kind == Kind::Stream {
                    vec!["samples", "views", "core"]
                } else if kind == Kind::Dynamic {
                    vec!["samples", "dynamic"]
                } else {
                    vec!["samples", "scratch"]
                }
                .into_iter()
                .collect();
            let actual_keys = data
                .as_object()
                .ok_or("invalid numerical member")?
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if kind != Kind::Verify && actual_keys != expected_keys
                || !expected_keys.is_subset(&actual_keys)
            {
                return Err("invalid original numerical member".into());
            }
            let contract_key = format!(
                "{}/{}",
                step["profile"].as_str().ok_or("missing profile")?,
                step["case"].as_str().ok_or("missing case")?
            );
            let analysis = worker(
                Observation {
                    kind,
                    data: &data,
                    step,
                    source,
                    page,
                    cpus,
                    current,
                },
                Some(json!({"path":numeric,"sha256":entry["numeric_journal_sha256"]})),
                contracts.get(&contract_key),
            )?;
            let summary = if step["profile"] == "decoder" {
                &analysis
            } else {
                let workload = analysis["preparation"]["workloads"].clone();
                if workloads.get(&contract_key).is_some_and(|v| v != &workload) {
                    return Err("diagnostic preparation differs across sources".into());
                }
                workloads.insert(contract_key.clone(), workload);
                if kind == Kind::Dynamic {
                    let trace = analysis["workload_trace"].clone();
                    if traces.get(&contract_key).is_some_and(|v| v != &trace) {
                        return Err("dynamic calibration/measurement input differs".into());
                    }
                    traces.insert(contract_key.clone(), trace);
                }
                contracts.insert(contract_key, analysis["contract"].clone());
                &analysis["summary"]
            };
            same_summary(entry, summary)?;
            if kind != Kind::Stream {
                geometry.insert((role.into(), key.clone()), shapes(kind, &data)?);
            }
            members.insert(member_name, entry["sha256"].clone());
            if kind == Kind::Dynamic {
                members.insert(numeric_name, entry["numeric_journal_sha256"].clone());
            }
            workers += 1;
            observations += 12;
            iterations += summary["iterations"].as_u64().unwrap_or(0);
            events += summary["events"].as_u64().unwrap_or(0);
        }
    }
    let mut result = json!({"schema":format!("{}-replay-v1",kind.schema()),"release_approval":false,"candidate_attribution":null,"formal_performance_geometry_equivalence":false,"source_count":2,"case_count":expected.len(),"members":members});
    if kind == Kind::Verify {
        result["auditor_sha256"] = controller_digest("audits")?.into();
        result["layout_differences"]=expected.into_iter().filter(|k|geometry[&("baseline".into(),k.clone())]!=geometry[&("candidate".into(),k.clone())]).map(|k|json!({"case":k,"different_shapes":geometry[&("baseline".into(),k.clone())].symmetric_difference(&geometry[&("candidate".into(),k.clone())]).count()})).collect::<Vec<_>>().into();
    } else if kind == Kind::Stream {
        result["auditor_sha256"] = controller_digest("audits")?.into();
        result["events"] = events.into();
        result["coverage"] = json!({"read_owners":true,"encode_parity_owners":true,"stream_verify_scratch":true,"stream_recovery_owners":true,"codec_internal_decode_scratch":false,"physical_allocation_identity":false,"first_fault_instruction":false});
    } else {
        result["case_count"] = 132.into();
        result["original_case_count"] = 126.into();
        result["decoder_supplement_case_count"] = 6.into();
        result["modes"] = json!(["calibrate", "sample"]);
        result["workers"] = workers.into();
        result["observations"] = observations.into();
        result["iterations"] = iterations.into();
        result["events"] = events.into();
        result["diagnostic_geometry_differences"] = expected
            .into_iter()
            .filter(|k| {
                geometry[&("baseline".into(), k.clone())]
                    != geometry[&("candidate".into(), k.clone())]
            })
            .collect::<Vec<_>>()
            .into();
        result["coverage"] = json!({"classic_decoder_matrix_storage":true,"decoder_heap_request_generations":true,"non_stream_dynamic_payloads":true,"smoke_hotspot_source_positions":true,"calibration_and_measurement_lifetimes":true,"leopard_decode_work_leases":true,"leopard_locator_conversion_output_scratch":true,"physical_allocation_identity":false,"first_fault_instruction":false,"formal_performance_acceptance":false});
    }
    Ok(result)
}
pub(super) fn cli(context: &Context<'_>, kind: Kind, args: Vec<String>) -> Result<()> {
    let options = Options::command(
        match kind {
            Kind::Verify => "verify-scratch-audit",
            Kind::Stream => "stream-core-audit",
            Kind::Dynamic => "dynamic-buffer-audit",
        },
        args,
    )?;
    if options.has("--check") == options.get("--replay").is_some() {
        return Err("choose --check or --replay".into());
    }
    if options.get("--replay").is_some() {
        if options.get("--output").is_some() {
            return Err("output is only for a new check".into());
        }
        println!("{}", verify(kind, &options.path("--replay")?, tool())?);
    } else {
        let destination = if options.get("--output").is_some() {
            options.path("--output")?
        } else {
            std::env::temp_dir().join(format!("rse-{}-{}", kind.schema(), io::utc_ns()?))
        };
        let report = check(
            context,
            kind,
            tool(),
            &destination,
            options.get("--baseline-ref").unwrap_or(BASELINE),
        )?;
        println!("{report}");
    }
    Ok(())
}

fn declaration(name: &str, member: &Path, numeric: &Path, log: &Path) -> Result<Value> {
    Ok(
        json!({"path":format!("{name}.json"),"sha256":io::digest(member)?,"raw_log_sha256":io::digest(log)?,"numeric_journal_path":format!("{name}-journal.jsonl"),"numeric_journal_sha256":io::digest(numeric)?}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::snapshot::tests::Directory;
    #[test]
    fn diagnostic_declarations_use_portable_relative_strings() {
        let directory = Directory::new();
        let member = directory.0.join("case.json");
        let numeric = directory.0.join("case-journal.jsonl");
        let log = directory.0.join("case.log");
        for path in [&member, &numeric, &log] {
            fs::write(path, "original").unwrap();
        }
        let entry = declaration("case", &member, &numeric, &log).unwrap();
        assert_eq!(entry["path"].as_str(), Some("case.json"));
        assert_eq!(
            entry["numeric_journal_path"].as_str(),
            Some("case-journal.jsonl")
        );
        assert_eq!(
            entry["sha256"],
            crate::artifact::snapshot::digest(b"original")
        );
    }
}
