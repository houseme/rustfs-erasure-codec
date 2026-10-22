//! Shared descriptive baseline statistics. No causal or release inference.
use crate::{Args, Result, release_rows, sampling_contract, statistics as stats};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
};

const PHASES: [&str; 2] = ["A1", "A2"];
const PROFILES: [&str; 4] = ["smoke", "small_file", "hotspot", "stream"];
const SPREAD_LIMIT: f64 = sampling_contract::CENTRAL_SPREAD_LIMIT;
const REPEAT_LIMIT: f64 = sampling_contract::REPEAT_DRIFT_LIMIT;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    ServiceTimes {
        records: Vec<Value>,
    },
    Gap {
        first: (Interval, Vec<Value>),
        second: (Interval, Vec<Value>),
    },
    Describe {
        groups: Vec<SourceGroup>,
    },
    Report {
        phases: Vec<String>,
        groups: Vec<Group>,
        frozen: Option<Frozen>,
    },
    Schedule {
        counts: BTreeMap<String, BTreeMap<String, u64>>,
    },
    AdjacentChecks {
        summaries: Vec<(String, BTreeMap<String, PairSummary>)>,
        gaps: Vec<u64>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceGroup {
    profile: String,
    case: String,
    phase: String,
    records: Vec<Value>,
    fixture: Option<Value>,
    entry: Option<Interval>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Group {
    profile: String,
    case: String,
    phase: String,
    detail: Value,
    window: Option<Budget>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Frozen {
    drift_failures: u64,
    spread_failures: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Interval {
    start_ns: u64,
    end_ns: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Budget {
    start_ns: u64,
    end_ns: u64,
    wall: u64,
    warmup: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairSummary {
    metric: String,
    value: Value,
    central_spread: Value,
}

fn delta(row: &Value, key: &str) -> Result<Value> {
    let before = stats::uint(&row["before"][key])?;
    let after = stats::uint(&row["after"][key])?;
    Ok(json!(
        after
            .checked_sub(before)
            .ok_or("backwards service counters")?
    ))
}
pub(crate) fn describe<T: Borrow<Value>>(
    rows: &[T],
    metric: &str,
    fixture: Option<&Value>,
) -> Result<Value> {
    if rows.len() != 12 {
        return Err("all fixed baseline observations are required".into());
    }
    let mut trace = Vec::with_capacity(12);
    for (index, r) in rows.iter().enumerate() {
        let r = r.borrow();
        if r["index"].as_u64() != Some(index as u64)
            || r["kind"] != if index < 3 { "warmup" } else { "sample" }
        {
            return Err("baseline observation order differs".into());
        }
        let iterations = stats::uint(&r["iterations"])?;
        if iterations == 0 {
            return Err("invalid baseline iteration count".into());
        }
        stats::positive(&r["rows"][0][metric])?;
        stats::positive(&r["wall_ns"])?;
        stats::positive(&r["timed_ns"])?;
        stats::uint(&r["before"]["cpu"])?;
        stats::uint(&r["after"]["cpu"])?;
        let run = delta(r, "run_ns")?;
        let wait = delta(r, "wait_ns")?;
        trace.push(json!({"index":r["index"],"kind":r["kind"],"iterations":iterations,"metric":r["rows"][0][metric],
            "wall_ns":r["wall_ns"],"timed_ns":r["timed_ns"],"cpu_before":r["before"]["cpu"],"cpu_after":r["after"]["cpu"],
            "main_run_ns":run,"main_run_ns_per_iteration":stats::ratio(&run,&json!(iterations))?,
            "main_wait_ns":wait,"main_switches":delta(r,"switches")?,"process_minor_faults":delta(r,"process_minor_faults")?,"process_major_faults":delta(r,"process_major_faults")?,
            "main_run_to_wall":stats::ratio(&run,&r["wall_ns"])? ,"main_wait_to_wall":stats::ratio(&wait,&r["wall_ns"])?,
            "wall_minus_timed_ns":stats::subtract(&r["wall_ns"],&r["timed_ns"])?}));
    }
    let measured = &trace[3..];
    let (median, spread) = stats::center(measured.iter().map(|r| &r["metric"]))?;
    let run = stats::ordered(
        measured.iter().map(|r| &r["main_run_ns_per_iteration"]),
        true,
    )?;
    let run_median = run[4];
    let run_spread = if run_median.as_f64().is_some_and(|v| v > 0.0) {
        json!(stats::ratio(
            &stats::difference(run[7], run[1]),
            run_median
        )?)
    } else {
        Value::Null
    };
    let range = |key: &str| -> Result<[f64; 2]> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for row in measured {
            let n = row[key].as_f64().ok_or("invalid service ratio")?;
            min = min.min(n);
            max = max.max(n);
        }
        Ok([min, max])
    };
    let gap: Vec<_> = measured
        .iter()
        .map(|r| stats::ratio(&r["wall_minus_timed_ns"], &r["wall_ns"]))
        .collect::<Result<_>>()?;
    let faults = |key| -> Result<u64> {
        measured.iter().try_fold(0_u64, |n, r| {
            n.checked_add(stats::uint(&r[key])?)
                .ok_or_else(|| "baseline fault count overflow".into())
        })
    };
    let cpus: BTreeSet<_> = measured
        .iter()
        .flat_map(|r| {
            [
                r["cpu_before"].as_u64().unwrap_or(0),
                r["cpu_after"].as_u64().unwrap_or(0),
            ]
        })
        .collect();
    Ok(json!({"median":median,"central_spread":spread,
        "short_samples":measured.iter().filter(|r|r["wall_ns"].as_f64().unwrap_or(0.0)<sampling_contract::MINIMUM_WALL_NS as f64||r["timed_ns"].as_f64().unwrap_or(0.0)<sampling_contract::MINIMUM_TIMED_NS as f64).count(),
        "measured_endpoint_cpus":cpus,"measured_endpoint_change_count":measured.iter().filter(|r|r["cpu_before"]!=r["cpu_after"]).count(),
        "maximum_measured_main_wait_to_wall":range("main_wait_to_wall")?[1],
        "measured_service":{"main_run_ns_per_iteration_median":run_median,"main_run_central_spread":run_spread,"main_run_to_wall_range":range("main_run_to_wall")?,
            "wall_minus_timed_fraction_range":[gap.iter().copied().fold(f64::INFINITY,f64::min),gap.iter().copied().fold(f64::NEG_INFINITY,f64::max)],
            "total_process_minor_faults":faults("process_minor_faults")?,"total_process_major_faults":faults("process_major_faults")?,
            "scope":"calling-thread counters bracket wall windows; not exclusive API cycles or an additive CPU-time partition"},
        "trace":trace,"caller_layout_recorded":!rows[0].borrow()["fixture"].is_null(),
        "caller_matches_calibration":fixture.map(|f|rows[0].borrow()["fixture"]==*f)}))
}
fn budget(entry: Interval, rows: &[Value]) -> Result<Budget> {
    let elapsed = entry
        .end_ns
        .checked_sub(entry.start_ns)
        .filter(|n| *n > 0)
        .ok_or("invalid integer wall/worker interval")?;
    if rows.is_empty() {
        return Err("invalid integer wall/worker interval".into());
    }
    let (mut wall, mut warmup) = (0_u128, 0_u128);
    for r in rows {
        let n = stats::uint(&r["wall_ns"])?;
        if n == 0 || r["kind"] != "warmup" && r["kind"] != "sample" {
            return Err("invalid integer wall/worker interval".into());
        }
        wall += u128::from(n);
        if r["kind"] == "warmup" {
            warmup += u128::from(n);
        }
    }
    if wall > u128::from(elapsed) {
        return Err("wall windows exceed containing worker".into());
    }
    Ok(Budget {
        start_ns: entry.start_ns,
        end_ns: entry.end_ns,
        wall: wall as u64,
        warmup: warmup as u64,
    })
}
fn window(first: &Budget, second: &Budget) -> Result<Value> {
    let gap = second
        .start_ns
        .checked_sub(first.end_ns)
        .ok_or("overlapping adjacent workers")?;
    let envelope = second
        .end_ns
        .checked_sub(first.start_ns)
        .ok_or("invalid adjacent envelope")?;
    let a = first
        .end_ns
        .checked_sub(first.start_ns)
        .and_then(|n| n.checked_sub(first.wall))
        .ok_or("invalid first worker budget")?;
    let b = second
        .end_ns
        .checked_sub(second.start_ns)
        .and_then(|n| n.checked_sub(second.wall))
        .ok_or("invalid second worker budget")?;
    let bound = a
        .checked_add(b)
        .and_then(|n| n.checked_add(gap))
        .ok_or("adjacent bound overflow")?;
    Ok(json!({"parent_interworker_gap_ns":gap,
        "phases":{"A1":{"worker_elapsed_ns":first.end_ns-first.start_ns,"recorded_wall_ns":first.wall,"outside_wall_ns":a},"A2":{"worker_elapsed_ns":second.end_ns-second.start_ns,"recorded_wall_ns":second.wall,"outside_wall_ns":b}},
        "pair_envelope_ns":envelope,"a2_warmup_wall_ns":second.warmup,"first_observation_gap_upper_ns":bound,
        "first_sample_gap_upper_ns":bound.checked_add(second.warmup).ok_or("adjacent warmup bound overflow")?}))
}
fn report(phases: &[String], groups: Vec<Group>, frozen: Option<Frozen>) -> Result<Value> {
    if phases != ["A1"] && phases != PHASES {
        return Err("complete baseline phases are required".into());
    }
    let mut summaries: BTreeMap<_, Value> = phases
        .iter()
        .map(|p| {
            (
                p,
                json!({"wide_cases":[],"short_samples":0,"layout_mismatches":[]}),
            )
        })
        .collect();
    let mut cases = BTreeMap::<(String, String), BTreeMap<String, Group>>::new();
    for g in groups {
        if !phases.contains(&g.phase) {
            return Err("unexpected baseline phase".into());
        }
        let summary = summaries
            .get_mut(&g.phase)
            .ok_or("missing baseline phase")?;
        if g.detail["central_spread"]
            .as_f64()
            .is_none_or(|v| !v.is_finite() || v < 0.0)
        {
            return Err("invalid baseline spread".into());
        }
        if g.detail["central_spread"].as_f64().unwrap_or(0.0) > SPREAD_LIMIT {
            summary["wide_cases"].as_array_mut().ok_or("missing wide-case list")?.push(json!({"profile":g.profile,"case":g.case,"central_spread":g.detail["central_spread"]}));
        }
        summary["short_samples"] = json!(
            stats::uint(&summary["short_samples"])?
                .checked_add(stats::uint(&g.detail["short_samples"])?)
                .ok_or("baseline short count overflow")?
        );
        if g.detail["caller_matches_calibration"] == false {
            summary["layout_mismatches"]
                .as_array_mut()
                .ok_or("missing mismatch list")?
                .push(json!({"profile":g.profile,"case":g.case}));
        }
        let key = (g.profile.clone(), g.case.clone());
        if cases
            .entry(key)
            .or_default()
            .insert(g.phase.clone(), g)
            .is_some()
        {
            return Err("duplicate baseline case/phase".into());
        }
    }
    let mut endpoints = BTreeMap::<&str, (u64, u64)>::new();
    let mut output = Vec::with_capacity(cases.len());
    let (mut drift_failures, mut spread_failures) = (0, 0);
    for ((profile, case), mut group) in cases {
        if group.len() != phases.len() || phases.iter().any(|p| !group.contains_key(p)) {
            return Err("missing baseline case phase".into());
        }
        let repeat = if phases.len() == 1 {
            Value::Null
        } else {
            let (first, last) = (&group["A1"].detail, &group["A2"].detail);
            let (a, b) = (
                &first["measured_endpoint_cpus"],
                &last["measured_endpoint_cpus"],
            );
            let single = a.as_array().is_some_and(|v| v.len() == 1)
                && b.as_array().is_some_and(|v| v.len() == 1);
            let category = if single {
                if a == b {
                    "single_endpoint_cpu_same"
                } else {
                    "single_endpoint_cpu_changed"
                }
            } else {
                "multiple_endpoint_cpus"
            };
            let drift = stats::ratio(
                &stats::difference(&last["median"], &first["median"]),
                &first["median"],
            )?;
            let failed = drift > REPEAT_LIMIT;
            drift_failures += u64::from(failed);
            let counter = endpoints.entry(category).or_default();
            counter.0 += 1;
            counter.1 += u64::from(failed);
            json!({"absolute_metric_drift":drift,"exceeds_repeat_limit":failed,"endpoint_category":category})
        };
        spread_failures += u64::from(
            group
                .values()
                .any(|g| g.detail["central_spread"].as_f64().unwrap_or(0.0) > SPREAD_LIMIT),
        );
        let separation = match (
            group["A1"].window.as_ref(),
            group.get("A2").and_then(|g| g.window.as_ref()),
        ) {
            (None, None) => None,
            (Some(a), Some(b)) => Some(window(a, b)?),
            _ => return Err("incomplete adjacent worker windows".into()),
        };
        let details: BTreeMap<_, _> = std::mem::take(&mut group)
            .into_iter()
            .map(|(p, g)| (p, g.detail))
            .collect();
        let mut result = json!({"profile":profile,"case":case,"metric":release_rows::metric(&profile)?,"phases":details,"repeat":repeat});
        if let Some(separation) = separation {
            result["window_separation"] = separation;
        }
        output.push(result);
    }
    if frozen
        .is_some_and(|f| f.drift_failures != drift_failures || f.spread_failures != spread_failures)
    {
        return Err("replayed statistics differ from frozen outcome".into());
    }
    let mut result = json!({"phase_quality":summaries,"repeat_endpoint_counts":endpoints.into_iter().map(|(k,(cases,failures))|(k,json!({"cases":cases,"repeat_failures":failures}))).collect::<BTreeMap<_,_>>(),"case_count":output.len(),"cases":output});
    for key in [
        "pair_envelope_ns",
        "first_observation_gap_upper_ns",
        "first_sample_gap_upper_ns",
    ] {
        if output.iter().any(|r| !r["window_separation"].is_null()) {
            let max = output
                .iter()
                .map(|r| stats::uint(&r["window_separation"][key]))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .max()
                .ok_or("empty baseline report")?;
            result[format!("maximum_{key}")] = json!(max);
        }
    }
    Ok(result)
}
/// Formal baseline service summaries over the retained nine samples.
fn service_times(records: &[Value]) -> Result<Value> {
    let mut groups = BTreeMap::<&str, Vec<&Value>>::new();
    for r in records {
        if r["kind"] == "sample" {
            groups
                .entry(r["case"].as_str().ok_or("missing baseline case")?)
                .or_default()
                .push(r);
        }
    }
    let mut result = serde_json::Map::new();
    for (case, rows) in groups {
        let ratios = |field: &str| -> Result<Vec<Value>> {
            rows.iter()
                .map(|r| {
                    let value = if field == "run_ns" {
                        delta(r, field)?
                    } else {
                        r[field].clone()
                    };
                    let count = stats::uint(&r["iterations"])?;
                    if count == 0 {
                        return Err("invalid baseline iteration count".into());
                    }
                    Ok(json!(stats::ratio(&value, &json!(count))?))
                })
                .collect()
        };
        let timed = ratios("timed_ns")?;
        let wall = ratios("wall_ns")?;
        let run = ratios("run_ns")?;
        let timed = stats::ordered(timed.iter(), true)?[4].clone();
        let wall = stats::ordered(wall.iter(), true)?[4].clone();
        let run = stats::ordered(run.iter(), true)?[4].clone();
        let mut wait = 0.0_f64;
        for r in &rows {
            wait = wait.max(stats::ratio(&delta(r, "wait_ns")?, &r["wall_ns"])?);
        }
        let faults = |key| -> Result<Vec<f64>> {
            rows.iter()
                .map(|r| stats::ratio(&delta(r, key)?, &r["iterations"]))
                .collect()
        };
        result.insert(case.to_owned(),json!({"timed_ns_per_iteration":timed,"wall_ns_per_iteration":wall,"thread_run_ns_per_iteration":run,
            "maximum_runnable_wait_fraction":wait,"process_minor_faults_per_iteration":faults("process_minor_faults")?,"process_major_faults_per_iteration":faults("process_major_faults")?}));
    }
    Ok(Value::Object(result))
}

impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::ServiceTimes { records } => service_times(&records),
            Self::Gap {
                first: (a, rows),
                second: (b, other),
            } => window(&budget(a, &rows)?, &budget(b, &other)?),
            Self::Describe { groups } => groups
                .into_iter()
                .map(|g| {
                    let detail = describe(
                        &g.records,
                        release_rows::metric(&g.profile)?,
                        g.fixture.as_ref(),
                    )?;
                    let window = g.entry.map(|entry| budget(entry, &g.records)).transpose()?;
                    Ok(Group {
                        profile: g.profile,
                        case: g.case,
                        phase: g.phase,
                        detail,
                        window,
                    })
                })
                .collect::<Result<Vec<_>>>()
                .and_then(|v| Ok(serde_json::to_value(v)?)),
            Self::Report {
                phases,
                groups,
                frozen,
            } => report(&phases, groups, frozen),
            Self::Schedule { counts } => {
                let catalog = sampling_contract::case_catalog();
                if counts.len() != 4
                    || PROFILES.iter().any(|p| {
                        counts.get(*p).is_none_or(|c| {
                            c.keys().collect::<BTreeSet<_>>()
                                != catalog[p].iter().collect::<BTreeSet<_>>()
                                || c.values().any(|n| !(1..=1_000_000_000).contains(n))
                        })
                    })
                {
                    return Err("all 126 original cases required".into());
                }
                let mut steps = Vec::with_capacity(252);
                for profile in PROFILES {
                    for (case, iterations) in &counts[profile] {
                        for phase in PHASES {
                            steps.push(json!({"ordinal":steps.len(),"profile":profile,"case":case,"phase":phase,"iterations":iterations}));
                        }
                    }
                }
                Ok(json!(steps))
            }
            Self::AdjacentChecks { summaries, gaps } => {
                let mut checks = Vec::with_capacity(summaries.len());
                for (case, pair) in summaries {
                    let first = pair.get("A1").ok_or("missing adjacent first phase")?;
                    let second = pair.get("A2").ok_or("missing adjacent repeat")?;
                    checks.push(json!({"case":case,"first":first.value,"second":second.value,"metric":first.metric,
                        "drift":stats::ratio(&stats::difference(&second.value,&first.value),&first.value)?,"widths":[first.central_spread,second.central_spread]}));
                }
                Ok(
                    json!({"checks":checks,"maximum_pair_gap_ns":gaps.into_iter().max().ok_or("missing adjacent gaps")?}),
                )
            }
        }
    }
}
pub(crate) fn run(args: &Args) -> Result<()> {
    let request: Request =
        crate::artifact::read_json(std::path::Path::new(args.required("--input")?))?;
    println!("{}", serde_json::to_string(&request.execute()?)?);
    Ok(())
}
#[cfg(test)]
mod tests;
