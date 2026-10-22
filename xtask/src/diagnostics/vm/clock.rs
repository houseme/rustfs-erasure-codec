//! One counter/window contract for both synchronous and whole-worker readings.
use crate::{Result, statistics as stats};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const SCHEMA: &str = "native-service-clock-v1";
const ALIGNMENT_LIMIT: f64 = 0.05;
const SCOPE: &str = "synchronous guest measuring-CPU counters enclosing original wall windows; includes observer/setup service; virtualized fidelity and physical-host exclusivity unproven; no exclusive API cycles, GHz inference or timing normalization";
const MSRS: [(&str, u64); 4] = [
    ("tsc_before", 0x10),
    ("aperf", 0xe8),
    ("mperf", 0xe7),
    ("tsc_after", 0x10),
];

pub(super) fn contract() -> Value {
    json!({"schema":SCHEMA,"alignment_limit":ALIGNMENT_LIMIT,"scope":SCOPE,"msrs":MSRS})
}
pub(super) fn keys(v: &Value, expected: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|m| m.len() == expected.len() && expected.iter().all(|k| m.contains_key(*k)))
}
#[derive(Clone, Copy, Serialize)]
struct Counters {
    tsc_before: u64,
    aperf: u64,
    mperf: u64,
    tsc_after: u64,
}
impl Counters {
    fn read(v: &Value) -> Result<Self> {
        if !keys(v, &["tsc_before", "aperf", "mperf", "tsc_after"]) {
            return Err("invalid counter point".into());
        }
        Ok(Self {
            tsc_before: stats::uint(&v["tsc_before"])?,
            aperf: stats::uint(&v["aperf"])?,
            mperf: stats::uint(&v["mperf"])?,
            tsc_after: stats::uint(&v["tsc_after"])?,
        })
    }
    fn ordered(&self) -> Result<()> {
        if self.tsc_after < self.tsc_before {
            return Err("invalid counter point".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub(super) struct Point {
    start: u64,
    end: u64,
    reading: Option<(u64, Counters)>,
}
pub(super) fn point(v: &Value) -> Result<Point> {
    if v["schema"] != SCHEMA || v["available"].as_bool().is_none() {
        return Err("invalid native service-clock point".into());
    }
    let available = v["available"] == true;
    let expected = if available {
        &[
            "schema",
            "start_ns",
            "end_ns",
            "available",
            "cpu",
            "counters",
        ]
    } else {
        &[
            "schema",
            "start_ns",
            "end_ns",
            "available",
            "reason",
            "errno",
        ]
    };
    if !keys(v, expected) {
        return Err("invalid native service-clock read bounds".into());
    }
    let start = stats::uint(&v["start_ns"])?;
    let end = stats::uint(&v["end_ns"])?;
    if end < start {
        return Err("invalid native service-clock read bounds".into());
    }
    let reading = if available {
        Some((stats::uint(&v["cpu"])?, Counters::read(&v["counters"])?))
    } else {
        if !v["reason"]
            .as_str()
            .is_some_and(|s| ["unsupported", "configuration", "io", "short-read"].contains(&s))
            || !v["errno"].is_null() && !v["errno"].as_u64().is_some_and(|n| n > 0)
        {
            return Err("invalid native service-clock failure".into());
        }
        None
    };
    Ok(Point {
        start,
        end,
        reading,
    })
}
pub(super) fn pair(record: &Value, required: bool) -> Result<Option<(Point, Point)>> {
    let a = &record["before"]["process"]["service_clock"];
    let b = &record["after"]["process"]["service_clock"];
    if a.is_null() && b.is_null() && !required {
        return Ok(None);
    }
    Ok(Some((point(a)?, point(b)?)))
}
fn snapshot<'a>(
    v: &'a Value,
    wanted: &BTreeSet<String>,
) -> Result<(u64, u64, BTreeMap<&'a str, Counters>)> {
    if !keys(v, &["start_ns", "end_ns", "cpus"]) {
        return Err("invalid counter snapshot".into());
    }
    let start = stats::uint(&v["start_ns"])?;
    let end = stats::uint(&v["end_ns"])?;
    let cpus = v["cpus"].as_object().ok_or("invalid counter snapshot")?;
    if end < start || cpus.len() != wanted.len() || cpus.keys().any(|cpu| !wanted.contains(cpu)) {
        return Err("invalid counter snapshot".into());
    }
    let counters = cpus
        .iter()
        .map(|(cpu, v)| {
            let c = Counters::read(v)?;
            c.ordered()?;
            Ok((cpu.as_str(), c))
        })
        .collect::<Result<_>>()?;
    Ok((start, end, counters))
}
fn deltas(before: &BTreeMap<&str, Counters>, after: &BTreeMap<&str, Counters>) -> Result<Value> {
    let mut result = BTreeMap::new();
    let mut active = false;
    for (cpu, old) in before {
        let new = after.get(cpu).ok_or("missing counter CPU")?;
        let low = new
            .tsc_before
            .checked_sub(old.tsc_after)
            .filter(|n| *n > 0)
            .ok_or("counter reset, wrap or invalid delta")?;
        let high = new
            .tsc_after
            .checked_sub(old.tsc_before)
            .filter(|n| *n > 0)
            .ok_or("counter reset, wrap or invalid delta")?;
        let aperf = new
            .aperf
            .checked_sub(old.aperf)
            .ok_or("counter reset, wrap or invalid delta")?;
        let mperf = new
            .mperf
            .checked_sub(old.mperf)
            .ok_or("counter reset, wrap or invalid delta")?;
        if (mperf == 0) != (aperf == 0) {
            return Err("counter reset, wrap or invalid delta".into());
        }
        active |= mperf > 0;
        result.insert(*cpu,json!({"aperf_delta":aperf,"mperf_delta":mperf,"tsc_delta_bounds":[low,high],
            "aperf_per_mperf":if mperf>0 {json!(stats::ratio(&json!(aperf),&json!(mperf))?)} else {Value::Null},
            "mperf_per_tsc_bounds":[stats::ratio(&json!(mperf),&json!(high))?,stats::ratio(&json!(mperf),&json!(low))?],
            "read_skew_tsc":[old.tsc_after-old.tsc_before,new.tsc_after-new.tsc_before]}));
    }
    if !active {
        return Err("no active counter delta".into());
    }
    Ok(json!(result))
}
pub(super) fn frequency(
    before: &Value,
    after: &Value,
    cpus: &[u64],
    start: u64,
    end: u64,
) -> Result<Value> {
    let wanted = cpus.iter().map(u64::to_string).collect();
    let (_, before_end, old) = snapshot(before, &wanted)?;
    let (after_start, _, new) = snapshot(after, &wanted)?;
    if !(before_end <= start && start < end && end <= after_start) {
        return Err("counter interval does not contain worker".into());
    }
    deltas(&old, &new)
}
pub(super) fn window(record: &Value, points: Option<(Point, Point)>) -> Result<Value> {
    let Some((before, after)) = points else {
        return Ok(Value::Null);
    };
    let wall = stats::uint(&record["wall_ns"])?;
    if wall == 0 || after.start.checked_sub(before.end).is_none_or(|n| n < wall) {
        return Err("native counter reads do not enclose the original wall window".into());
    }
    let extra = after.end - before.start - wall;
    let fraction = stats::ratio(&json!(extra), &json!(wall))?;
    let mut result = json!({"interpretation_qualified":false,"enclosure_overhead_fraction":fraction,"alignment_limit":ALIGNMENT_LIMIT,"scope":SCOPE,
        "exclusive_api_cycles":false,"time_normalization":false,"counters":null});
    let (Some((a_cpu, a)), Some((b_cpu, b))) = (before.reading, after.reading) else {
        result["refusal"] = json!("counter-unavailable");
        return Ok(result);
    };
    let cpu = stats::uint(&record["placement"]["cpu"])?;
    if a_cpu != cpu || b_cpu != cpu {
        return Err("native counters do not belong to the measuring CPU".into());
    }
    a.ordered()?;
    b.ordered()?;
    let key = cpu.to_string();
    let counters = deltas(
        &BTreeMap::from([(key.as_str(), a)]),
        &BTreeMap::from([(key.as_str(), b)]),
    )?;
    if fraction > ALIGNMENT_LIMIT {
        result["refusal"] = json!("counter-enclosure-too-wide");
        return Ok(result);
    }
    result["interpretation_qualified"] = json!(true);
    result["counters"] = counters;
    Ok(result)
}
