//! Native directory replay: one bounded snapshot and shared validation models.
use super::{Group, PROFILES};
use crate::{
    Args, Result,
    artifact::snapshot::Snapshot,
    fingerprint, release_rows,
    sampling_contract::{self, preparation::Preparation},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub(crate) const BASELINE: &str = "9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6";
const QUALIFICATION: &str = "baseline-qualification-v2";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    capture: PathBuf,
    renderer_sha256: String,
    analyzer_sha256: String,
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
// Protocol numbers are small fixed constants; preserve former JSON numeric equality.
fn protocol_matches(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| protocol_matches(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && b.iter()
                    .all(|(k, v)| a.get(k).is_some_and(|a| protocol_matches(a, v)))
        }
        _ => actual == expected,
    }
}
fn records(value: Value) -> Result<Vec<Value>> {
    match value {
        Value::Array(rows) => Ok(rows),
        _ => Err("original sample array is required".into()),
    }
}
fn has_repeat(root: &std::path::Path) -> Result<bool> {
    match fs::symlink_metadata(root.join("A2")) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(true),
        Ok(_) => Err("bounded regular original phase directory required".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        if !sha(&self.renderer_sha256) || !sha(&self.analyzer_sha256) {
            return Err("explicit source fingerprints are required".into());
        }
        let root = fs::canonicalize(&self.capture)?;
        let mut snapshot = Snapshot::new(&root)?;
        let repeat = has_repeat(&root)?;
        let (mut result, _, _) = bound(
            &mut snapshot,
            "",
            None,
            &self.renderer_sha256,
            &self.analyzer_sha256,
            repeat,
        )?;
        if has_repeat(&root)? != repeat {
            return Err("original baseline phase set changed during replay".into());
        }
        result["input_sha256"] = serde_json::to_value(snapshot.finish()?)?;
        Ok(result)
    }
}

/// The envelope and baseline models decode the same original snapshot bytes.
pub(crate) fn bound(
    snapshot: &mut Snapshot,
    prefix: &str,
    pins: Option<&std::collections::BTreeMap<String, String>>,
    renderer_sha256: &str,
    analyzer_sha256: &str,
    repeat: bool,
) -> Result<(Value, Value, Value)> {
    if !sha(renderer_sha256) || !sha(analyzer_sha256) {
        return Err("explicit source fingerprints are required".into());
    }
    let read = |snapshot: &mut Snapshot, name: &str| -> Result<Value> {
        let pin = pins
            .map(|pins| {
                pins.get(name)
                    .map(String::as_str)
                    .ok_or("missing baseline member pin")
            })
            .transpose()?;
        match pin {
            Some(pin) => snapshot.json_bound(&format!("{prefix}{name}"), Some(pin)),
            None => snapshot.json(&format!("{prefix}{name}")),
        }
    };
    let mut provenance = read(snapshot, "provenance.json")?;
    let schema = provenance["schema_version"]
        .as_str()
        .ok_or("missing baseline schema")?
        .to_owned();
    if ![
        "baseline-basic-controls-v1",
        "baseline-qualification-v1",
        QUALIFICATION,
    ]
    .contains(&schema.as_str())
        || provenance["source"]["commit"] != BASELINE
    {
        return Err("a complete published-9.0.0 baseline control is required".into());
    }
    let expected = sampling_contract::protocol_value();
    let (protocol, renderer) = if schema == "baseline-basic-controls-v1" {
        (
            provenance["protocol"].clone(),
            &provenance["renderer_sha256"],
        )
    } else {
        let contract = provenance["sampling"]
            .as_object()
            .ok_or("invalid baseline qualification provenance")?;
        if contract.len() != expected.as_object().ok_or("missing protocol")?.len() + 2
            || !expected
                .as_object()
                .ok_or("missing protocol")?
                .keys()
                .all(|k| contract.contains_key(k))
            || !contract
                .get("overlay_sha256")
                .and_then(Value::as_str)
                .is_some_and(sha)
            || provenance["release_approval"] != false
            || !provenance["candidate_attribution"].is_null()
        {
            return Err("invalid baseline qualification provenance".into());
        }
        let protocol: serde_json::Map<_, _> = expected
            .as_object()
            .ok_or("missing protocol")?
            .keys()
            .map(|k| (k.clone(), provenance["sampling"][k].clone()))
            .collect();
        (
            Value::Object(protocol),
            &provenance["sampling"]["implementation_sha256"],
        )
    };
    if !protocol_matches(&protocol, &expected) || renderer != renderer_sha256 {
        return Err("replay with the matching frozen protocol and renderer".into());
    }
    let context = &provenance["context"];
    if schema == QUALIFICATION {
        crate::runtime::validate_runtime(&context["runtime"])?;
    }
    let cpu_value = context["isolation"]
        .get("cpu_affinity")
        .ok_or("missing baseline CPU policy")?;
    let cpus: Option<Vec<u64>> = serde_json::from_value(cpu_value.clone())?;
    let page: Option<u64> = serde_json::from_value(
        context["runtime"]
            .get("page_size")
            .ok_or("missing baseline page size")?
            .clone(),
    )?;

    let phases = if repeat {
        vec!["A1".to_owned(), "A2".to_owned()]
    } else {
        vec!["A1".to_owned()]
    };
    let mut descriptions = Vec::new();
    let catalog = sampling_contract::case_catalog();
    let mut common_counts = json!({});
    for profile in PROFILES {
        let calibration = records(read(
            snapshot,
            &format!("calibration/{profile}-samples.json"),
        )?)?;
        let counts = sampling_contract::calibration_profile::counts(
            &calibration,
            profile,
            BASELINE,
            page,
            cpus.as_deref(),
        )?;
        let counts: std::collections::BTreeMap<_, _> =
            counts.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        common_counts[profile] = serde_json::to_value(&counts)?;
        let contract = release_rows::Request {
            profile: profile.into(),
            revision: BASELINE.into(),
            rows: Value::Null,
            iteration_counts: Some(counts),
            default_iterations: None,
            complete: false,
        };
        let prepared = Preparation::new(&calibration, true)?;
        let expected: BTreeSet<_> = catalog[profile].iter().cloned().collect();
        for phase in &phases {
            let rows = records(read(snapshot, &format!("{phase}/{profile}-samples.json"))?)?;
            let groups = sampling_contract::batch::validate_groups(
                &rows,
                &contract,
                &expected,
                cpus.as_deref(),
                page,
            )?;
            prepared.validate(profile, &rows)?;
            for (case, indices) in groups {
                let observations: Vec<_> = indices.iter().map(|i| &rows[*i]).collect();
                descriptions.push(Group {
                    profile: profile.into(),
                    case: case.into(),
                    phase: phase.clone(),
                    detail: super::describe(
                        &observations,
                        release_rows::metric(profile)?,
                        prepared.fixtures.get(case).copied(),
                    )?,
                    window: None,
                });
            }
        }
    }
    let Value::Object(mut result) = super::report(&phases, descriptions, None)? else {
        return Err("invalid baseline summary".into());
    };
    let binding = provenance.clone();
    let measured = provenance
        .get_mut("tool")
        .ok_or("missing measured tool")?
        .take();
    let Value::Object(metadata) = json!({"schema_version":"baseline-variability-analysis-v1",
            "purpose":"descriptive replay of every baseline case; no causal or release inference",
            "release_approval":false,"candidate_attribution":null,"baseline_commit":BASELINE,"measured_tool":measured,
            "protocol":protocol,"renderer_sha256":renderer_sha256,"capture_schema":schema,
            "kernel_memory_policy_recorded":schema==QUALIFICATION,"analyzer_sha256":analyzer_sha256,
            "phase_order":phases,
            "limitations":["Equal CPU endpoints do not exclude intermediate migration or prove affinity pinning.",
                "Main-thread counters omit Rayon worker scheduling and physical-host interference.",
                "Counters bracket the wall timer; run/wall ratios can exceed one and are not clamped.",
                "Matching caller offsets do not establish equal physical pages, cache placement or dynamic outputs.",
                "This replay does not replace original provenance/isolation checks or strict release preflight."]})
    else {
        return Err("invalid baseline metadata".into());
    };
    result.extend(metadata);
    Ok((Value::Object(result), binding, common_counts))
}

pub(crate) fn run(args: &Args) -> Result<()> {
    let compiled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing native tool root")?;
    let tool = std::path::Path::new(args.get(
        "--tool",
        compiled.to_str().ok_or("invalid native tool path")?,
    ));
    let request = Request {
        capture: PathBuf::from(args.required("--capture")?),
        renderer_sha256: fingerprint::sampling(tool)?,
        analyzer_sha256: fingerprint::file(&tool.join(
            if tool.join("xtask/src/control/baseline.rs").is_file() {
                "xtask/src/control/baseline.rs"
            } else {
                "scripts/analyze_baseline_variability.py"
            },
        ))?,
    };
    println!("{}", serde_json::to_string(&request.execute()?)?);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
