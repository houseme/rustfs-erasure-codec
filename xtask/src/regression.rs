use crate::{
    Args, Result,
    benchmark_contract::{self as model, CONTRACT, Key},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn finite_ratio(text: &str, flag: &str, unit: bool) -> Result<f64> {
    let value = text.parse::<f64>()?;
    if !value.is_finite() || value < 0.0 || unit && value > 1.0 {
        return Err(format!(
            "{flag} expects a finite nonnegative ratio{}",
            if unit { " at most 1" } else { "" }
        )
        .into());
    }
    Ok(value)
}

pub(crate) fn benchmark(args: &Args) -> Result<bool> {
    let metric = args.get("--metric", "throughput_mb_s");
    if !["throughput_mb_s", "ns_per_iter", "ns_per_block"].contains(&metric) {
        return Err(format!("unsupported metric: {metric}").into());
    }
    let mut thresholds = CONTRACT.default_thresholds.clone();
    for item in args.many("--threshold") {
        let (op, raw) = item
            .split_once('=')
            .ok_or("invalid threshold override, expected op=value")?;
        if op.trim().is_empty() {
            return Err("invalid threshold override".into());
        }
        thresholds.insert(
            op.trim().to_owned(),
            finite_ratio(raw.trim(), "--threshold", false)?,
        );
    }
    let strict = args.0.contains_key("--strict-provenance");
    let baseline = model::index(
        model::load_rows(Path::new(args.required("--baseline")?))?,
        "baseline",
        strict,
    )?;
    let current = model::index(
        model::load_rows(Path::new(args.required("--current")?))?,
        "current",
        strict,
    )?;
    let mut failures: Vec<Value> = Vec::new();
    let mut comparisons = Vec::new();
    let mut unchecked = Vec::new();
    let missing: Vec<_> = baseline
        .keys()
        .filter(|key| !current.contains_key(*key))
        .map(|key| json!({"case":key.label(),"error":"baseline case missing from current results"}))
        .collect();
    if !args.0.contains_key("--allow-missing-cases") {
        failures.extend(missing.iter().cloned());
    }
    for (key, new) in &current {
        let Some(old) = baseline.get(key) else {
            continue;
        };
        let case = key.label();
        let mut incompatible = false;
        for field in &CONTRACT.provenance_fields {
            let (before, after) = (&old.provenance[field], &new.provenance[field]);
            let reason = if before.is_none() || after.is_none() {
                unchecked.push(json!({"case":case,"field":field}));
                if before == after || args.0.contains_key("--allow-missing-provenance") {
                    continue;
                }
                "provenance missing from one artifact"
            } else if before == after {
                continue;
            } else {
                "incompatible provenance"
            };
            failures.push(
                json!({"case":case,"field":field,"error":reason,"baseline":before,"current":after}),
            );
            incompatible = true;
        }
        if incompatible {
            continue;
        }
        let before = *old.metrics.get(metric).ok_or_else(|| {
            format!("metric {metric} requested but artifact does not contain {metric}")
        })?;
        let after = *new.metrics.get(metric).ok_or_else(|| {
            format!("metric {metric} requested but artifact does not contain {metric}")
        })?;
        let ratio = if metric == "throughput_mb_s" {
            if after >= before {
                0.0
            } else {
                (before - after) / before
            }
        } else {
            if after <= before {
                0.0
            } else {
                (after - before) / before
            }
        };
        if !ratio.is_finite() {
            return Err("derived regression ratio is not finite".into());
        }
        let ratio = ratio.max(0.0);
        let threshold = *thresholds.get(&key.0[0]).unwrap_or(&0.15);
        let comparison = json!({"operation":key.0[0],"data_shards":key.0[1],"parity_shards":key.0[2],"shard_size":key.0[3],"stream_block_size":key.0[4],"metric":metric,
            "baseline_metric_value":before,"current_metric_value":after,"baseline_throughput_mb_s":old.metrics["throughput_mb_s"],"current_throughput_mb_s":new.metrics["throughput_mb_s"],
            "baseline_ns_per_iter":old.metrics["ns_per_iter"],"current_ns_per_iter":new.metrics["ns_per_iter"],"regression_ratio":ratio,"threshold":threshold});
        if ratio > threshold {
            failures.push(comparison.clone());
        }
        comparisons.push(comparison);
    }
    for required in args.many("--require-case") {
        let parts: Vec<_> = required.split(':').collect();
        if ![4, 5].contains(&parts.len()) {
            return Err(format!("invalid required case '{required}', expected operation:data:parity:shard_size[:stream_block_size]").into());
        }
        let mut values = [
            parts[0].to_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        for (index, part) in parts[1..].iter().enumerate() {
            values[index + 1] =
                model::positive_integer(&Value::String((*part).into()), "required case dimension")?;
        }
        let key = Key(values);
        for (source, rows) in [("baseline", &baseline), ("current", &current)] {
            if !rows.contains_key(&key) {
                failures.push(json!({"operation":parts[0],"data_shards":parts[1],"parity_shards":parts[2],"shard_size":parts[3],"error":format!("required case missing from {source} results")}));
            }
        }
    }
    if comparisons.is_empty() {
        failures.push(json!({"error":"no comparable benchmark cases"}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"baseline":args.required("--baseline")?,"current":args.required("--current")?,"metric":metric,"strict_provenance":strict,
        "thresholds":thresholds,"comparisons":comparisons,"failures":failures,"unchecked_provenance":unchecked,"missing_cases":missing})
        )?
    );
    Ok(failures.is_empty())
}

fn scenario_values(args: &Args, flag: &str) -> Result<BTreeMap<String, f64>> {
    let mut values = BTreeMap::new();
    for item in args.many(flag) {
        let (scenario, raw) = item
            .split_once('=')
            .ok_or_else(|| format!("{flag} expects SCENARIO=VALUE"))?;
        if scenario.trim().is_empty()
            || values
                .insert(
                    scenario.to_owned(),
                    finite_ratio(raw.trim(), flag, flag != "--min-speedup")?,
                )
                .is_some()
        {
            return Err(format!("{flag} requires unique nonempty scenario names").into());
        }
    }
    Ok(values)
}

pub(crate) fn hotspot(args: &Args) -> Result<bool> {
    let metric = args.get("--metric", "candidate_mb_s");
    if !["candidate_mb_s", "baseline_mb_s", "speedup"].contains(&metric) {
        return Err(format!("unsupported metric: {metric}").into());
    }
    let limit = finite_ratio(
        args.get("--max-regression", "0.12"),
        "--max-regression",
        true,
    )?;
    let baseline = model::hotspots(
        model::load_rows(Path::new(args.required("--baseline")?))?,
        metric,
    )?;
    let current = model::hotspots(
        model::load_rows(Path::new(args.required("--current")?))?,
        metric,
    )?;
    let overrides = scenario_values(args, "--scenario-max-regression")?;
    let floors = scenario_values(args, "--min-speedup")?;
    let required: BTreeSet<_> = baseline
        .keys()
        .chain(args.many("--require-scenario"))
        .chain(overrides.keys())
        .chain(floors.keys())
        .cloned()
        .collect();
    let missing: Vec<_> = required
        .iter()
        .filter(|case| !baseline.contains_key(*case) || !current.contains_key(*case))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(format!("missing required hotspot scenarios: {}", missing.join(", ")).into());
    }
    let mut failures = Vec::new();
    for scenario in &required {
        let (old, new) = (&baseline[scenario], &current[scenario]);
        for field in &CONTRACT.hotspot_case_fields {
            if old.case_fields[field] != new.case_fields[field] {
                return Err(format!("incomparable {field} for {scenario}").into());
            }
        }
        for field in &CONTRACT.provenance_fields {
            if old.provenance[field] != new.provenance[field] {
                return Err(format!("incomparable {field} for {scenario}").into());
            }
        }
        let ratio = ((old.metrics[metric] - new.metrics[metric]) / old.metrics[metric]).max(0.0);
        if !ratio.is_finite() {
            return Err("derived regression ratio is not finite".into());
        }
        let allowed = *overrides.get(scenario).unwrap_or(&limit);
        if ratio > allowed {
            failures.push(format!(
                "- {scenario}: {metric} regressed by {ratio:.4}, allowed {allowed:.4}"
            ));
        }
        if let Some(floor) = floors.get(scenario) {
            let speedup = *new
                .metrics
                .get("speedup")
                .ok_or("speedup is required for a minimum floor")?;
            if speedup < *floor {
                failures.push(format!(
                    "- {scenario}: speedup {speedup:.4} is below floor {floor:.4}"
                ));
            }
        }
    }
    if !failures.is_empty() {
        eprintln!(
            "reconstruction hotspot gate failed:\n{}",
            failures.join("\n")
        );
        return Ok(false);
    }
    println!(
        "reconstruction hotspot gate passed for {}",
        required.into_iter().collect::<Vec<_>>().join(", ")
    );
    Ok(true)
}
