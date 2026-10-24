//! Original-byte adjacent replay, source-bound Rust models, and descriptive analysis.
use super::{capture, environment as env, frozen::Models, io::Context, release, sampling, *};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
const SCHEMA: &str = "adjacent-baseline-diagnostic-v1";
const GAP: u64 = 10_000_000_000;
fn same(a: &Value, b: &Value) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}
pub(super) fn compare_context(context: &Value, prior: &Value) -> Result<()> {
    crate::runtime::validate_runtime(&context["runtime"])?;
    let mut compared = context.clone();
    if prior["runtime"].get("kernel_memory_policy").is_some() {
        crate::runtime::validate_runtime(&prior["runtime"])?;
    } else {
        compared["runtime"]
            .as_object_mut()
            .ok_or("invalid runtime")?
            .remove("kernel_memory_policy");
    }
    if !same(&compared, prior) {
        return Err("recorded environment or kernel policy differs".into());
    }
    Ok(())
}
struct Originals {
    paths: BTreeMap<String, (PathBuf, String)>,
    bytes: BTreeMap<String, Vec<u8>>,
}
impl Originals {
    fn new(capture: &Path, prior: &Path) -> Result<Self> {
        let adjacent = io::member(capture, "adjacent")?;
        let mut paths = BTreeMap::new();
        for name in ["reservation/status.json", "reservation/cleanup.json"] {
            paths.insert(name.into(), (capture.to_owned(), name.into()));
        }
        for name in ["plan.json", "result.json", "execution.jsonl"] {
            paths.insert(format!("adjacent/{name}"), (adjacent.clone(), name.into()));
        }
        for ordinal in 0..252 {
            for suffix in ["json", "log"] {
                let name = format!("workers/{ordinal:03}.{suffix}");
                paths.insert(format!("adjacent/{name}"), (adjacent.clone(), name));
            }
        }
        for name in ["provenance.json", "qualification.partial.json"] {
            paths.insert(format!("prior/{name}"), (prior.to_owned(), name.into()));
        }
        for (profile, _) in release::FILES {
            let name = format!("calibration/{profile}-samples.json");
            paths.insert(format!("prior/{name}"), (prior.to_owned(), name));
        }
        let mut total = 0_u64;
        for (root, name) in paths.values() {
            let path = io::member(root, name)?;
            let metadata = fs::symlink_metadata(path)?;
            total = total
                .checked_add(metadata.len())
                .ok_or("snapshot size overflow")?;
            if !metadata.is_file()
                || metadata.len() == 0
                || metadata.len() > crate::artifact::snapshot::MEMBER_LIMIT
                || total > 256 * 1024 * 1024
            {
                return Err(
                    "bounded regular original members and aggregate byte budget required".into(),
                );
            }
        }
        let mut bytes = BTreeMap::new();
        let mut total = 0_usize;
        for (key, (root, name)) in &paths {
            let raw = io::bytes(&io::member(root, name)?, None)?;
            total = total
                .checked_add(raw.len())
                .ok_or("snapshot size overflow")?;
            if total > 256 * 1024 * 1024 {
                return Err("adjacent snapshot changed beyond byte budget".into());
            }
            bytes.insert(key.clone(), raw);
        }
        Ok(Self { paths, bytes })
    }
    fn finish(&self) -> Result<()> {
        for (key, (root, name)) in &self.paths {
            if io::bytes(&io::member(root, name)?, None)? != self.bytes[key] {
                return Err("original members changed during frozen replay".into());
            }
        }
        Ok(())
    }
    fn root<'a>(&'a self, path: &'a Path, prefix: &'a str) -> Archive<'a> {
        Archive {
            root: path,
            cached: Some((&self.bytes, prefix)),
        }
    }
}
pub(super) struct Archive<'a> {
    pub root: &'a Path,
    cached: Option<(&'a BTreeMap<String, Vec<u8>>, &'a str)>,
}
impl<'a> Archive<'a> {
    pub fn disk(root: &'a Path) -> Self {
        Self { root, cached: None }
    }
    fn bytes(&self, name: &str, sha: Option<&str>) -> Result<Vec<u8>> {
        if let Some((cache, prefix)) = self.cached {
            let bytes = cache
                .get(&format!("{prefix}/{name}"))
                .ok_or("missing original snapshot member")?;
            if let Some(sha) = sha
                && (!io::digest_valid(sha) || crate::artifact::snapshot::digest(bytes) != sha)
            {
                return Err("original member digest mismatch".into());
            }
            Ok(bytes.clone())
        } else {
            io::bytes(&io::member(self.root, name)?, sha)
        }
    }
    pub fn text(&self, name: &str, sha: Option<&str>) -> Result<String> {
        Ok(String::from_utf8(self.bytes(name, sha)?)?
            .replace("\r\n", "\n")
            .replace('\r', "\n"))
    }
    pub fn json(&self, name: &str, sha: Option<&str>) -> Result<Value> {
        crate::artifact::python_json::parse(&self.text(name, sha)?)
    }
    fn bound(&self, name: &str) -> Result<(Value, String)> {
        let bytes = self.bytes(name, None)?;
        let sha = crate::artifact::snapshot::digest(&bytes);
        Ok((
            crate::artifact::python_json::parse(
                &String::from_utf8(bytes)?
                    .replace("\r\n", "\n")
                    .replace('\r', "\n"),
            )?,
            sha,
        ))
    }
}
pub(super) struct Inputs {
    pub provenance: Value,
    pub counts: Value,
    pub calibration: Value,
    pub digests: Value,
}
pub(super) fn inputs(
    prior: &Archive<'_>,
    repository: &Path,
    models: &mut Models<'_, '_>,
) -> Result<Inputs> {
    let (provenance, prov_sha) = prior.bound("provenance.json")?;
    let (document, doc_sha) = prior.bound("qualification.partial.json")?;
    let schema = provenance["schema_version"]
        .as_str()
        .ok_or("missing baseline qualification schema")?;
    if !["baseline-qualification-v1", "baseline-qualification-v2"].contains(&schema)
        || provenance["release_approval"].as_bool() != Some(false)
        || !provenance["candidate_attribution"].is_null()
    {
        return Err("require original diagnostic baseline provenance".into());
    }
    if schema == "baseline-qualification-v2" {
        crate::runtime::validate_runtime(&provenance["context"]["runtime"])?;
    }
    if provenance["source"] != env::baseline(repository)?
        || provenance["tool"]
            != env::source_at(
                repository,
                provenance["tool"]["commit"]
                    .as_str()
                    .ok_or("missing prior tool source")?,
            )?
    {
        return Err("original baseline/tool identity differs".into());
    }
    for field in ["source", "context", "tool"] {
        if document[field] != provenance[field] {
            return Err("prior control metadata differs".into());
        }
    }
    let protocol = crate::sampling_contract::protocol_value();
    if protocol
        .as_object()
        .ok_or("invalid protocol")?
        .iter()
        .any(|(k, v)| {
            !provenance["sampling"]
                .get(k)
                .is_some_and(|r| crate::artifact::python_json::equivalent(r, v))
        })
        || provenance["sampling"]["implementation_sha256"] != models.implementation()?
        || provenance["sampling"]["overlay_sha256"] != models.overlay()?
        || provenance["harness_sha256"] != release::harness(models.assets())?
    {
        return Err("frozen worker protocol/harness differs".into());
    }
    let mut digests = json!({"provenance.json":prov_sha,"qualification.partial.json":doc_sha});
    let mut counts = json!({});
    let mut calibration = json!({});
    for (profile, _) in release::FILES {
        let name = format!("calibration/{profile}-samples.json");
        let entry = &document["calibration"][profile];
        if entry["path"] != name {
            return Err("prior calibration member differs".into());
        }
        let rows = prior.json(
            &name,
            Some(
                entry["sha256"]
                    .as_str()
                    .ok_or("missing prior calibration digest")?,
            ),
        )?;
        digests[&name] = entry["sha256"].clone();
        let prepared=models.model("sampling",json!({"operation":"calibration-profile","profile":profile,"records":rows,"revision":BASELINE,"expected_page_size":provenance["context"]["runtime"]["page_size"],"allowed_cpus":provenance["context"]["isolation"]["cpu_affinity"]}))?;
        let profile_counts: BTreeMap<String, u64> =
            serde_json::from_value(prepared["counts"].clone())?;
        counts[profile] = serde_json::to_value(profile_counts)?;
        let rows = rows.as_array().ok_or("missing calibration rows")?;
        let completed: Vec<_> = rows
            .iter()
            .filter(|r| r["complete"].as_bool() == Some(true))
            .cloned()
            .collect();
        let reference = capture::observations(profile, &completed, None)?;
        calibration[profile] = json!({"reference":reference,"fixtures":prepared["preparation"]["fixtures"],"workloads":prepared["preparation"]["workloads"],"traces":prepared["preparation"]["traces"]});
    }
    if document["sampling"]["common_iterations"] != counts {
        return Err("prior frozen counts differ".into());
    }
    Ok(Inputs {
        provenance,
        counts,
        calibration,
        digests,
    })
}
pub(super) fn schedule(counts: &Value, models: &mut Models<'_, '_>) -> Result<Vec<Value>> {
    Ok(serde_json::from_value(models.model(
        "baseline-analysis",
        json!({"operation":"schedule","counts":counts}),
    )?)?)
}
pub(super) fn worker_request(
    rows: Value,
    raw: String,
    step: &Value,
    calibration: &Value,
    context: &Value,
) -> Result<Value> {
    let profile = step["profile"].as_str().ok_or("missing worker profile")?;
    let case = step["case"].as_str().ok_or("missing worker case")?;
    let reference = &calibration[profile];
    Ok(
        json!({"rows":rows,"raw":raw,"profile":profile,"case":case,"iterations":step["iterations"],"revision":BASELINE,"calibration":{"reference":reference["reference"][case],"fixture":reference["fixtures"][case],"workload":reference["workloads"][case],"trace":reference["traces"][case]},"allowed_cpus":context["isolation"]["cpu_affinity"],"expected_page_size":context["runtime"]["page_size"]}),
    )
}
fn worker(models: &mut Models<'_, '_>, request: Value) -> Result<Value> {
    models.model(
        "sampling",
        json!({"operation":"replay-worker","request":request}),
    )
}
fn summarize(
    archive: &Archive<'_>,
    input: &Inputs,
    mut summaries: BTreeMap<String, Value>,
    gaps: Vec<u64>,
    models: &mut Models<'_, '_>,
) -> Result<Value> {
    let mut ordered = Vec::with_capacity(summaries.len());
    for (profile, _) in release::FILES {
        let prefix = format!("{profile}/");
        let keys: Vec<_> = summaries
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in keys {
            ordered.push((
                key.clone(),
                summaries.remove(&key).ok_or("missing ordered case")?,
            ));
        }
    }
    if !summaries.is_empty() {
        return Err("unknown adjacent profile".into());
    }
    let checks = models.model(
        "baseline-analysis",
        json!({"operation":"adjacent-checks","summaries":ordered,"gaps":gaps}),
    )?;
    let mut result = json!({"schema":SCHEMA,"release_approval":false,"candidate_attribution":null,"formal_qualification":false,"baseline":input.provenance["source"],"cases":126,"observations":3024,"execution_sha256":crate::artifact::snapshot::digest(&archive.bytes("execution.jsonl",None)?),"plan_sha256":crate::artifact::snapshot::digest(&archive.bytes("plan.json",None)?)});
    result
        .as_object_mut()
        .ok_or("invalid adjacent result")?
        .extend(checks.as_object().ok_or("invalid adjacent checks")?.clone());
    Ok(result)
}
fn frozen_controller(repository: &Path, plan: &Value) -> Result<Value> {
    let source = env::source(repository)?;
    let recorded = &plan["tool"];
    if env::source_at(
        repository,
        recorded["commit"]
            .as_str()
            .ok_or("missing frozen identity")?,
    )? != *recorded
        || ["tree", "lock_sha256"]
            .iter()
            .any(|k| source[k] != recorded[k])
    {
        return Err("frozen tool tree/lock differs".into());
    }
    let native = repository.join("xtask/src/control/baseline.rs");
    let relative = if native.is_file() {
        "xtask/src/control/baseline.rs"
    } else {
        "scripts/diagnose_adjacent_baseline.py"
    };
    let path = io::member(repository, relative)?;
    let committed = io::git_bytes(
        repository,
        &format!(
            "{}:{relative}",
            source["commit"].as_str().ok_or("missing frozen commit")?
        ),
    )?;
    if io::bytes(&path, None)? != committed
        || crate::artifact::snapshot::digest(&committed) != plan["controller_sha256"]
    {
        return Err("frozen controller bytes differ".into());
    }
    Ok(source)
}
pub(super) fn replay(
    archive: &Archive<'_>,
    input: &Inputs,
    repository: &Path,
    models: &mut Models<'_, '_>,
) -> Result<Value> {
    for name in ["failure.json", "diagnostic-status.json"] {
        if io::present(&archive.root.join(name)) {
            return Err("failed adjacent capture remains unaccepted".into());
        }
    }
    let result = archive.json("result.json", None)?;
    let plan = archive.json(
        "plan.json",
        Some(
            result["plan_sha256"]
                .as_str()
                .ok_or("missing plan digest")?,
        ),
    )?;
    let expected = schedule(&input.counts, models)?;
    if plan["schema"] != SCHEMA
        || plan["release_approval"].as_bool() != Some(false)
        || !plan["candidate_attribution"].is_null()
        || !same(&plan["steps"], &json!(expected))
        || plan["prior_inputs"] != input.digests
        || plan["baseline"] != input.provenance["source"]
        || plan["max_pair_gap_ns"].as_u64() != Some(GAP)
    {
        return Err("predeclared adjacent plan differs".into());
    }
    frozen_controller(repository, &plan)?;
    let context = &plan["context"];
    compare_context(context, &input.provenance["context"])?;
    let execution = archive.text(
        "execution.jsonl",
        Some(
            result["execution_sha256"]
                .as_str()
                .ok_or("missing execution digest")?,
        ),
    )?;
    let entries: Vec<Value> = execution
        .lines()
        .map(crate::artifact::python_json::parse)
        .collect::<Result<_>>()?;
    if entries.len() != 252 {
        return Err("incomplete original adjacent case catalog".into());
    }
    let mut summaries: BTreeMap<String, Value> = BTreeMap::new();
    let mut last = 0;
    let mut gaps = Vec::new();
    for (step, entry) in expected.iter().zip(&entries) {
        if entry.as_object().is_none_or(|o| o.len() != 7)
            || !same(&entry["step"], step)
            || !same(&entry["before"], context)
            || !same(&entry["after"], context)
        {
            return Err("execution order or runtime context differs".into());
        }
        let start = entry["start_ns"]
            .as_u64()
            .ok_or("invalid worker interval")?;
        let end = entry["end_ns"]
            .as_u64()
            .filter(|end| last <= start && start < *end)
            .ok_or("invalid worker interval")?;
        let gap = start - last;
        if step["phase"] == "A2" {
            if gap > GAP {
                return Err("paired workers are not temporally adjacent".into());
            }
            gaps.push(gap);
        }
        last = end;
        let ordinal = step["ordinal"].as_u64().ok_or("invalid worker ordinal")?;
        let rows = archive.json(
            &format!("workers/{ordinal:03}.json"),
            Some(
                entry["samples_sha256"]
                    .as_str()
                    .ok_or("missing sample digest")?,
            ),
        )?;
        let raw = archive.text(
            &format!("workers/{ordinal:03}.log"),
            Some(entry["log_sha256"].as_str().ok_or("missing log digest")?),
        )?;
        let wall = rows
            .as_array()
            .ok_or("invalid original rows")?
            .iter()
            .try_fold(0_u64, |sum, r| {
                sum.checked_add(
                    r["wall_ns"]
                        .as_u64()
                        .ok_or("invalid worker wall interval")?,
                )
                .ok_or("worker wall overflow")
            })?;
        if end - start < wall {
            return Err("worker interval cannot contain its original observations".into());
        }
        let detail = worker(
            models,
            worker_request(rows, raw, step, &input.calibration, context)?,
        )?;
        let key = format!(
            "{}/{}",
            step["profile"].as_str().ok_or("missing profile")?,
            step["case"].as_str().ok_or("missing case")?
        );
        summaries.entry(key).or_insert_with(|| json!({}))
            [step["phase"].as_str().ok_or("missing phase")?] = detail;
    }
    let expected = summarize(archive, input, summaries, gaps, models)?;
    if !same(&result, &expected) {
        return Err("diagnostic summary differs from original workers".into());
    }
    Ok(expected)
}
pub(super) fn capture_adjacent(
    context: &Context<'_>,
    baseline: &Path,
    prior: &Path,
    output: &Path,
    group: &Path,
) -> Result<Value> {
    io::directory(output)?;
    let result = (|| -> Result<Value> {
        let mut models = Models::current();
        let input = inputs(&Archive::disk(prior), tool(), &mut models)?;
        if env::source(baseline)? != input.provenance["source"]
            || release::harness(baseline)? != release::harness(tool())?
            || sampling::overlay_digest(baseline, None)? != sampling::overlay_digest(tool(), None)?
        {
            return Err("require unchanged original baseline/worker/harness".into());
        }
        let environment = env::capture();
        let resource = env::context(baseline, &environment, group)?;
        compare_context(&resource, &input.provenance["context"])?;
        let tool_source = env::source(tool())?;
        let steps = schedule(&input.counts, &mut models)?;
        let plan = json!({"schema":SCHEMA,"release_approval":false,"candidate_attribution":null,"baseline":input.provenance["source"],"tool":tool_source,"controller_sha256":controller_digest("baseline")?,"prior_inputs":input.digests,"context":resource,"max_pair_gap_ns":GAP,"steps":steps});
        io::write_json(&output.join("plan.json"), &plan, true)?;
        fs::create_dir(output.join("workers"))?;
        fs::create_dir(output.join("counts"))?;
        let mut summaries: BTreeMap<String, Value> = BTreeMap::new();
        let mut gaps = Vec::new();
        let mut last = None;
        {
            let collector = capture::Collector {
                context,
                environment: environment.clone(),
                output,
                group,
            };
            let overlay = collector.prepare(baseline, &output.join("build.log"), None)?;
            let mut ledger = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output.join("execution.jsonl"))?;
            for step in &steps {
                if env::source(tool())? != tool_source
                    || env::source(baseline)? != input.provenance["source"]
                {
                    return Err("diagnostic source changed".into());
                }
                let before = env::context(baseline, &environment, group)?;
                if before != resource {
                    return Err("runtime context changed".into());
                }
                let ordinal = step["ordinal"].as_u64().ok_or("invalid ordinal")?;
                let case = step["case"].as_str().ok_or("missing case")?;
                let profile = step["profile"].as_str().ok_or("missing profile")?;
                let count = step["iterations"].as_u64().ok_or("missing frozen count")?;
                fs::write(
                    output.join("counts").join(format!("{ordinal:03}.tsv")),
                    format!("{case}\t{count}\n"),
                )?;
                let log = output.join("workers").join(format!("{ordinal:03}.log"));
                let start = context.now_ns()?;
                if step["phase"] == "A2" && last.is_some_and(|end| start - end > GAP) {
                    return Err("paired worker start gap exceeded; no retry".into());
                }
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
                let end = context.now_ns()?;
                let after = env::context(baseline, &environment, group)?;
                let member = output.join("workers").join(format!("{ordinal:03}.json"));
                io::write_json(&member, &rows, true)?;
                io::append_json(
                    &mut ledger,
                    &json!({"step":step,"before":before,"after":after,"start_ns":start,"end_ns":end,"samples_sha256":io::digest(&member)?,"log_sha256":io::digest(&log)?}),
                )?;
                if after != resource {
                    return Err("runtime context changed; original worker retained".into());
                }
                let summary = worker(
                    &mut models,
                    worker_request(
                        rows.into(),
                        io::text(&log, None)?,
                        step,
                        &input.calibration,
                        &resource,
                    )?,
                )?;
                let key = format!("{profile}/{case}");
                summaries.entry(key).or_insert_with(|| json!({}))
                    [step["phase"].as_str().ok_or("missing phase")?] = summary;
                if step["phase"] == "A2" {
                    gaps.push(start - last.ok_or("A2 precedes A1")?);
                }
                last = Some(end);
                io::write_json(
                    &output.join("progress.tmp"),
                    &json!({"completed_steps":ordinal+1,"total_steps":252,"release_approval":false}),
                    false,
                )?;
                fs::rename(output.join("progress.tmp"), output.join("progress.json"))?;
            }
            overlay.finish()?;
        }
        if env::source(baseline)? != input.provenance["source"]
            || env::source(tool())? != tool_source
            || env::context(baseline, &environment, group)? != resource
        {
            return Err("source/context not restored".into());
        }
        let archive = Archive::disk(output);
        let result = summarize(&archive, &input, summaries, gaps, &mut models)?;
        io::write_json(&output.join("result.json"), &result, true)?;
        let validated = replay(&archive, &input, tool(), &mut models)?;
        io::write_json(&output.join("replay.json"), &validated, true)?;
        Ok(result)
    })();
    if let Err(error) = &result {
        io::failure(output, error)?;
    }
    result
}
fn resources(status: &Value, cleanup: &Value) -> Result<()> {
    if status["status"] != "collected-diagnostic-only"
        || !status["error"].is_null()
        || status["release_approval"].as_bool() != Some(false)
        || !status["isolation_checks"].as_u64().is_some_and(|n| n > 0)
        || [
            "child_removed",
            "controller_restored",
            "parent_affinities_restored",
        ]
        .iter()
        .any(|k| cleanup[*k].as_bool() != Some(true))
        || !cleanup["error"].is_null()
    {
        return Err("complete capture and restored resources required".into());
    }
    Ok(())
}
fn analyze_adjacent_inputs(
    context: &Context<'_>,
    capture: &Path,
    prior: &Path,
    frozen: &Path,
) -> Result<(Value, Inputs, Value)> {
    for name in ["failure.json", "diagnostic-status.json"] {
        if io::present(&capture.join("adjacent").join(name)) {
            return Err("failed adjacent capture remains unaccepted".into());
        }
    }
    let original = Originals::new(capture, prior)?;
    let status = crate::artifact::python_json::parse(&String::from_utf8(
        original.bytes["reservation/status.json"].clone(),
    )?)?;
    let cleanup = crate::artifact::python_json::parse(&String::from_utf8(
        original.bytes["reservation/cleanup.json"].clone(),
    )?)?;
    resources(&status, &cleanup)?;
    let adjacent_root = io::member(capture, "adjacent")?;
    let adjacent = original.root(&adjacent_root, "adjacent");
    let prior_archive = original.root(prior, "prior");
    let plan = adjacent.json("plan.json", None)?;
    let source = frozen_controller(frozen, &plan)?;
    let mut models = Models::frozen(context, frozen, &source)?;
    let input = inputs(&prior_archive, frozen, &mut models)?;
    compare_context(&plan["context"], &input.provenance["context"])?;
    let validated = replay(&adjacent, &input, frozen, &mut models)?;
    let entries: Vec<Value> = adjacent
        .text("execution.jsonl", None)?
        .lines()
        .map(crate::artifact::python_json::parse)
        .collect::<Result<_>>()?;
    let mut groups = Vec::new();
    for (profile, _) in release::FILES {
        let mut source_groups = Vec::new();
        for entry in &entries {
            let step = &entry["step"];
            if step["profile"] != profile {
                continue;
            }
            let ordinal = step["ordinal"].as_u64().ok_or("missing worker ordinal")?;
            let case = step["case"].as_str().ok_or("missing case")?;
            source_groups.push(json!({"profile":profile,"case":case,"phase":step["phase"],"records":adjacent.json(&format!("workers/{ordinal:03}.json"),None)?,"fixture":input.calibration[profile]["fixtures"][case],"entry":{"start_ns":entry["start_ns"],"end_ns":entry["end_ns"]}}));
        }
        let reply = models.model(
            "baseline-analysis",
            json!({"operation":"describe","groups":source_groups}),
        )?;
        groups.extend(
            reply
                .as_array()
                .ok_or("missing original descriptions")?
                .iter()
                .cloned(),
        );
    }
    let drift = validated["checks"]
        .as_array()
        .ok_or("invalid frozen checks")?
        .iter()
        .filter(|c| {
            c["drift"]
                .as_f64()
                .is_some_and(|n| n > crate::sampling_contract::REPEAT_DRIFT_LIMIT)
        })
        .count();
    let spread = validated["checks"]
        .as_array()
        .ok_or("invalid frozen checks")?
        .iter()
        .filter(|c| {
            c["widths"].as_array().is_some_and(|v| {
                v.iter().any(|v| {
                    v.as_f64()
                        .is_some_and(|n| n > crate::sampling_contract::CENTRAL_SPREAD_LIMIT)
                })
            })
        })
        .count();
    let frozen_summary = json!({"cases":126,"observations":3024,"release_approval":false,"formal_qualification":false,"drift_failures":drift,"spread_failures":spread});
    let summary=models.model("baseline-analysis",json!({"operation":"report","phases":["A1","A2"],"groups":groups,"frozen":{"drift_failures":drift,"spread_failures":spread}}))?;
    drop(models);
    if frozen_controller(frozen, &plan)? != source {
        return Err("frozen source changed during replay".into());
    }
    original.finish()?;
    let hashes: BTreeMap<_, _> = original
        .bytes
        .iter()
        .map(|(name, raw)| (name, crate::artifact::snapshot::digest(raw)))
        .collect();
    let mut result = json!({"schema_version":"baseline-adjacent-service-analysis-v1","purpose":"complete-catalog descriptive service and conservative workload-window separation; no causal or release inference","release_approval":false,"candidate_attribution":null,"formal_qualification":false,"new_acceptance_gate":false,"baseline_commit":BASELINE,"measured_tool":plan["tool"],"frozen_replay_source":source,"native_replay_controller_sha256":controller_digest("baseline")?,"protocol":crate::sampling_contract::protocol_value(),"renderer_sha256":crate::fingerprint::sampling(frozen)?,"capture_schema":plan["schema"],"kernel_memory_policy_recorded":true,"prior_kernel_memory_policy_recorded":input.provenance["context"]["runtime"].get("kernel_memory_policy").is_some(),"analyzer_sha256":controller_digest("baseline")?,"input_sha256":hashes,"resources_restored":true,"frozen_replay_summary":frozen_summary,"phase_order":["A1","A2"],"inner_api_gap_measured":false,"limitations":["Bounds are from the last A1 wall window to the first A2 warmup or measured-sample wall window, not inner API timestamps.","The pair envelope contains every workload window in both workers; median ranks are not treated as timestamps.","Both workers contribute all setup, teardown and between-observation overhead, making the bound conservative.","Observed bounds do not introduce or retroactively satisfy a new timing gate.","Historical missing kernel policy stays unknown; cross-campaign causal equivalence is not established.","Source instrumentation, dynamic/internal geometry and physical-host interference are not resolved by this timing bound."]});
    result
        .as_object_mut()
        .ok_or("invalid baseline analysis")?
        .extend(
            summary
                .as_object()
                .ok_or("invalid baseline summary")?
                .clone(),
        );
    Ok((result, input, plan["context"].clone()))
}
pub(super) fn analyze_adjacent(
    context: &Context<'_>,
    capture: &Path,
    prior: &Path,
    frozen: &Path,
) -> Result<Value> {
    Ok(analyze_adjacent_inputs(context, capture, prior, frozen)?.0)
}
pub(super) fn frequency_inputs(
    context: &Context<'_>,
    capture: &Path,
    prior: &Path,
    frozen: &Path,
) -> Result<(Value, Inputs, Value)> {
    analyze_adjacent_inputs(context, capture, prior, frozen)
}
pub(super) fn adjacent_cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("diagnose-adjacent-baseline", args)?;
    let mode = options.mode.as_deref().ok_or("choose capture or replay")?;
    if mode == "replay" {
        options.only(&["--prior", "--output"])?;
    }
    let prior = options.path("--prior")?;
    let output = options.path("--output")?;
    let result = if mode == "capture" {
        capture_adjacent(
            context,
            &options.path("--baseline")?,
            &prior,
            &output,
            &options.path("--isolation-cgroup")?,
        )?
    } else {
        let mut models = Models::current();
        let input = inputs(&Archive::disk(&prior), tool(), &mut models)?;
        replay(&Archive::disk(&output), &input, tool(), &mut models)?
    };
    let checks = result["checks"].as_array().ok_or("missing checks")?;
    println!(
        "{}",
        json!({"cases":result["cases"],"observations":result["observations"],"release_approval":false,"formal_qualification":false,"drift_failures":checks.iter().filter(|c|c["drift"].as_f64().is_some_and(|n|n>crate::sampling_contract::REPEAT_DRIFT_LIMIT)).count(),"spread_failures":checks.iter().filter(|c|c["widths"].as_array().is_some_and(|v|v.iter().any(|v|v.as_f64().is_some_and(|n|n>crate::sampling_contract::CENTRAL_SPREAD_LIMIT)))).count()})
    );
    Ok(())
}
pub(super) fn cli(context: &Context<'_>, args: Vec<String>) -> Result<()> {
    let options = Options::command("analyze-baseline-variability", args)?;
    if options.get("--capture").is_some() == options.get("--adjacent-capture").is_some() {
        return Err("choose capture or adjacent-capture".into());
    }
    let output = options.path("--output")?;
    if io::present(&output) {
        return Err("output must be a new file; original reports are preserved".into());
    }
    let result = if options.get("--capture").is_some() {
        let source = options
            .get("--frozen-tool")
            .map(PathBuf::from)
            .unwrap_or(tool().to_owned());
        let controller = source.join(if source.join("xtask/src/control/baseline.rs").is_file() {
            "xtask/src/control/baseline.rs"
        } else {
            "scripts/analyze_baseline_variability.py"
        });
        model(
            "baseline-replay",
            json!({"capture":options.path("--capture")?,"renderer_sha256":crate::fingerprint::sampling(&source)?,"analyzer_sha256":io::digest(&controller)?}),
        )?
    } else {
        analyze_adjacent(
            context,
            &options.path("--adjacent-capture")?,
            &options.path("--prior")?,
            &options.path("--frozen-tool")?,
        )?
    };
    io::write_json(&output, &result, true)?;
    println!(
        "{}",
        json!({"analysis_written":true,"case_count":result["case_count"],"phase_order":result["phase_order"],"release_approval":false})
    );
    Ok(())
}

#[cfg(test)]
mod tests;
