use super::clock;
use crate::{Result, sampling_contract, statistics as stats};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Thread {
    cpu: u64,
    run: u64,
    wait: u64,
    slices: u64,
}
struct Process {
    minor: u64,
    major: u64,
    observer: u64,
    threads: BTreeMap<u64, Thread>,
}
impl Process {
    fn read(v: &Value) -> Result<Self> {
        if v["available"] != true || v["complete"] != true {
            return Err("incomplete process observation".into());
        }
        let raw = v["threads"]
            .as_array()
            .filter(|r| !r.is_empty())
            .ok_or("invalid anonymous thread set")?;
        let mut measuring = 0;
        let mut threads = BTreeMap::new();
        for t in raw {
            measuring += u64::from(
                t["measuring_thread"]
                    .as_bool()
                    .ok_or("invalid measuring-thread flag")?,
            );
            stats::uint(&t["minor_faults"])?;
            stats::uint(&t["major_faults"])?;
            let thread = Thread {
                cpu: stats::uint(&t["cpu"])?,
                run: stats::uint(&t["run_ns"])?,
                wait: stats::uint(&t["wait_ns"])?,
                slices: stats::uint(&t["timeslices"])?,
            };
            if threads.insert(stats::uint(&t["slot"])?, thread).is_some() {
                return Err("invalid anonymous thread set".into());
            }
        }
        if measuring != 1 {
            return Err("one measuring thread is required".into());
        }
        Ok(Self {
            minor: stats::uint(&v["minor_faults"])?,
            major: stats::uint(&v["major_faults"])?,
            observer: stats::uint(&v["snapshot_ns"])?,
            threads,
        })
    }
}
fn processes(r: &Value) -> Result<(Process, Process)> {
    Ok((
        Process::read(&r["before"]["process"])?,
        Process::read(&r["after"]["process"])?,
    ))
}
pub(super) fn validate(records: &[Value], native: bool) -> Result<()> {
    for r in records {
        processes(r)?;
        if native {
            clock::pair(r, true)?;
        }
    }
    Ok(())
}
pub(super) fn strip(mut records: Vec<Value>) -> Result<Value> {
    for r in &mut records {
        for side in ["before", "after"] {
            r[side]
                .as_object_mut()
                .ok_or("invalid diagnostic counters")?
                .remove("process");
        }
    }
    Ok(Value::Array(records))
}
struct Sample {
    value: Value,
    trace: Value,
}
pub(super) fn analyze(
    records: &[Value],
    profile: &str,
    native: bool,
    validate_all: bool,
) -> Result<Value> {
    let mut groups: Vec<(&str, Vec<Sample>)> = Vec::new();
    let mut indices = BTreeMap::new();
    for r in records {
        let sample = r["kind"] == "sample";
        if !sample && !validate_all {
            continue;
        }
        let (before, after) = processes(r)?;
        let points = if sample || native {
            clock::pair(r, native)?
        } else {
            None
        };
        if !sample {
            continue;
        }
        let case = r["case"].as_str().ok_or("missing diagnostic case")?;
        let iterations = stats::uint(&r["iterations"])?;
        if iterations == 0 {
            return Err("invalid diagnostic iteration count".into());
        }
        stats::positive(&r["timed_ns"])?;
        stats::positive(&r["wall_ns"])?;
        let value = json!(stats::ratio(&r["timed_ns"], &json!(iterations))?);
        let (mut run, mut wait, mut migrations) = (0_u128, 0_u128, 0_u64);
        for (slot, old) in &before.threads {
            if let Some(new) = after.threads.get(slot) {
                // Stable slots include the thread start identity: reject before summing.
                run += u128::from(
                    new.run
                        .checked_sub(old.run)
                        .ok_or("thread counters reset")?,
                );
                wait += u128::from(
                    new.wait
                        .checked_sub(old.wait)
                        .ok_or("thread counters reset")?,
                );
                new.slices
                    .checked_sub(old.slices)
                    .ok_or("thread counters reset")?;
                migrations += u64::from(old.cpu != new.cpu);
            }
        }
        let minor = after
            .minor
            .checked_sub(before.minor)
            .ok_or("process fault counters reset")?;
        let major = after
            .major
            .checked_sub(before.major)
            .ok_or("process fault counters reset")?;
        let mut trace = json!({"index":r["index"],"timed_ms":r["timed_ns"].as_f64().ok_or("invalid diagnostic interval")?/1e6,
            "wall_ms":r["wall_ns"].as_f64().ok_or("invalid diagnostic interval")?/1e6,
            "interval_sufficient":r["wall_ns"].as_f64().is_some_and(|n|n>=sampling_contract::MINIMUM_WALL_NS as f64)&&r["timed_ns"].as_f64().is_some_and(|n|n>=sampling_contract::MINIMUM_TIMED_NS as f64),
            "process_faults":{"minor_faults":minor,"major_faults":major},
            "stable_thread_set":before.threads.keys().eq(after.threads.keys()),"thread_count":after.threads.len(),
            "shared_thread_run_ms":run as f64/1e6,"shared_thread_wait_ms":wait as f64/1e6,"thread_cpu_migrations":migrations,
            "observer_ms":(u128::from(before.observer)+u128::from(after.observer)) as f64/1e6});
        let clock = clock::window(r, points)?;
        if !clock.is_null() {
            trace["native_service_clock"] = clock;
        }
        let group = *indices.entry(case).or_insert_with(|| {
            groups.push((case, Vec::with_capacity(9)));
            groups.len() - 1
        });
        groups[group].1.push(Sample { value, trace });
    }
    let mut result = Vec::with_capacity(groups.len());
    for (case, samples) in groups {
        if samples.len() != 9 {
            return Err("incomplete diagnostic samples".into());
        }
        let (median, spread) = stats::center(samples.iter().map(|s| &s.value))?;
        let trace = samples
            .into_iter()
            .map(|s| {
                let mut t = s.trace;
                t["relative_ns"] = json!(stats::ratio(&s.value, &median)?);
                Ok(t)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut summary = json!({"profile":profile,"case":case,"central_ns_spread":spread});
        summary["samples"] = Value::Array(trace);
        result.push(summary);
    }
    Ok(Value::Array(result))
}
