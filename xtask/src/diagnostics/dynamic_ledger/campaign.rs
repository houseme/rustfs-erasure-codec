//! One fixed dynamic plan and complete envelope/journal contract.
use crate::{
    Result, artifact::equality::python as equivalent, memory_policy, runtime, sampling_contract,
    statistics,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::LazyLock};

pub(super) const MODES: [&str; 2] = ["calibrate", "sample"];
const ROLES: [&str; 2] = ["candidate", "baseline"];
static STEPS: LazyLock<Vec<Value>> =
    LazyLock::new(|| {
        let catalog = sampling_contract::case_catalog();
        let mut steps = Vec::with_capacity(516);
        for role in ROLES {
            for mode in MODES {
                for profile in sampling_contract::PROFILES {
                    let mut cases = catalog[profile].clone();
                    cases.sort_unstable();
                    steps.extend(cases.into_iter().map(
                        |case| json!({"role":role,"mode":mode,"profile":profile,"case":case}),
                    ));
                }
            }
            // Python iterated the sorted native supplement map, not declaration order.
            let mut supplements: Vec<_> =
                super::payload::SUPPLEMENTS.iter().map(|s| s.case).collect();
            supplements.sort_unstable();
            steps.extend(supplements.into_iter().map(
                |case| json!({"role":role,"mode":"structure","profile":"decoder","case":case}),
            ));
        }
        steps
    });
pub(super) fn steps() -> &'static [Value] {
    &STEPS
}
fn keys(value: &Value, expected: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.len() == expected.len() && expected.iter().all(|k| v.contains_key(*k)))
}

fn source(value: &Value) -> Result<()> {
    if !keys(value, &["commit", "tree", "lock_sha256"])
        || [("commit", 40), ("tree", 40), ("lock_sha256", 64)]
            .iter()
            .any(|(k, n)| {
                !value[*k].as_str().is_some_and(|s| {
                    s.len() == *n
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
            })
    {
        return Err("invalid source/tool identity".into());
    }
    Ok(())
}
const FIELDS: &[&str] = &[
    "schema",
    "release_approval",
    "candidate_attribution",
    "formal_performance_geometry_equivalence",
    "tool",
    "sources",
    "cases",
    "source_overlays",
    "harness_sha256",
    "overlay_sha256",
    "context",
    "page_size",
    "cpu_affinity",
    "native_sha256",
    "sampler_implementation_sha256",
    "controller_sha256",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    report: Value,
    plan: Value,
    execution: String,
}
impl Request {
    pub(crate) fn execute(&self) -> Result<Value> {
        let report = &self.report;
        if !keys(report, FIELDS)
            || report["schema"] != "dynamic-buffer-check-v1"
            || report["release_approval"].as_bool() != Some(false)
            || !report["candidate_attribution"].is_null()
            || report["formal_performance_geometry_equivalence"].as_bool() != Some(false)
        {
            return Err("unexpected dynamic inventory envelope".into());
        }
        let context = &report["context"];
        if !keys(
            context,
            &[
                "compiler",
                "machine",
                "cpu_identity",
                "cpu_affinity",
                "page_size",
                "kernel_memory_policy",
                "libc",
                "allocator_environment",
                "settings",
            ],
        ) {
            return Err("missing dynamic runtime provenance".into());
        }
        if ["compiler", "machine", "cpu_identity"]
            .iter()
            .any(|k| !context[k].as_str().is_some_and(|s| !s.is_empty()))
            || !equivalent(&context["cpu_affinity"], &report["cpu_affinity"])
            || !equivalent(&context["page_size"], &report["page_size"])
            || context["settings"] != sampling_contract::protocol::sampling_environment()
        {
            return Err("dynamic runtime/settings differ".into());
        }
        if !context["libc"]
            .as_array()
            .is_some_and(|v| v.len() == 2 && v.iter().all(Value::is_string))
            || !context["allocator_environment"]
                .as_object()
                .is_some_and(|v| {
                    v.iter()
                        .all(|(k, v)| runtime::allocator_key(k) && v.is_string())
                })
        {
            return Err("invalid dynamic allocator provenance".into());
        }
        let page = statistics::uint(&report["page_size"])?;
        if !sampling_contract::valid_page_size(page) {
            return Err("invalid dynamic page/CPU contract".into());
        }
        if report["cpu_affinity"].is_null() {
            if !context["kernel_memory_policy"].is_null() {
                return Err("unrecorded native memory policy".into());
            }
        } else {
            let cpus: Vec<u64> = serde_json::from_value(report["cpu_affinity"].clone())?;
            if cpus.is_empty() || cpus.windows(2).any(|v| v[0] >= v[1]) {
                return Err("invalid dynamic page/CPU contract".into());
            }
            memory_policy::parse(context["kernel_memory_policy"].clone(), Some(page))?;
        }
        for key in [
            "sources",
            "cases",
            "source_overlays",
            "harness_sha256",
            "overlay_sha256",
        ] {
            if !keys(&report[key], &ROLES) {
                return Err("incomplete per-source dynamic envelope".into());
            }
        }
        source(&report["tool"])?;
        let mut sources = BTreeMap::new();
        for role in ROLES {
            source(&report["sources"][role])?;
            sources.insert(role, &report["sources"][role]);
        }
        if report["tool"] != report["sources"]["candidate"] {
            return Err("dynamic tool/candidate identity differs".into());
        }
        let mut plan_fields = FIELDS.to_vec();
        plan_fields.push("steps");
        if !keys(&self.plan, &plan_fields)
            || self.plan["steps"] != json!(&*STEPS)
            || self.plan["cases"] != json!({"candidate":{},"baseline":{}})
            || FIELDS
                .iter()
                .filter(|k| **k != "cases")
                .any(|k| !equivalent(&self.plan[k], &report[k]))
        {
            return Err("frozen complete dynamic plan differs".into());
        }
        let mut lines = self.execution.lines();
        for step in &*STEPS {
            let role = step["role"].as_str().ok_or("missing plan role")?;
            let key = format!(
                "{}/{}/{}",
                step["mode"].as_str().ok_or("missing plan mode")?,
                step["profile"].as_str().ok_or("missing plan profile")?,
                step["case"].as_str().ok_or("missing plan case")?
            );
            let entry = report["cases"][role]
                .get(&key)
                .ok_or("incomplete dynamic mode/profile/catalog")?;
            for (stage, end) in [("begin", false), ("end", true)] {
                let line = lines
                    .next()
                    .ok_or("incomplete original execution journal")?;
                let actual: Value = serde_json::from_str(line)?; // Original last-key JSONL behavior.
                if !keys(
                    &actual,
                    if end {
                        &["stage", "step", "context", "entry"]
                    } else {
                        &["stage", "step", "context"]
                    },
                ) || actual["stage"] != stage
                    || actual["step"] != *step
                    || !equivalent(&actual["context"], context)
                    || end && !equivalent(&actual["entry"], entry)
                {
                    return Err("dynamic execution order/context/member differs".into());
                }
            }
        }
        if lines.next().is_some() {
            return Err("incomplete original execution journal".into());
        }
        Ok(json!(sources))
    }
}
#[cfg(test)]
mod tests;
