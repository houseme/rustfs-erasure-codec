//! One original frequency worker: replay, enclosing counters and service summary.
use super::clock;
use crate::{Result, diagnostics::baseline, sampling_contract::replay, statistics};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    replay: replay::Request,
    step: Value,
    entry: Value,
    begin: Value,
    context: Value,
    last_end: u64,
}
fn total(rows: &[Value], key: &str) -> Result<Value> {
    let (mut integer, mut floating) = (0_u64, None);
    for row in rows {
        let value = &row[key];
        if let Some(n) = value.as_u64() {
            if let Some(sum) = &mut floating {
                *sum += n as f64;
            } else {
                integer = integer
                    .checked_add(n)
                    .ok_or("original interval sum overflow")?;
            }
        } else {
            let n = statistics::positive(value)?;
            *floating.get_or_insert(integer as f64) += n;
        }
    }
    match floating {
        Some(n) if !n.is_finite() => Err("original interval sum overflow".into()),
        Some(n) => Ok(json!(n)),
        None => Ok(json!(integer)),
    }
}
impl Request {
    pub(crate) fn execute(&self) -> Result<Value> {
        if !self.replay.matches_context(&self.step, &self.context)
            || !clock::keys(
                &self.entry,
                &[
                    "step",
                    "before",
                    "after",
                    "start_ns",
                    "end_ns",
                    "counters_before",
                    "counters_after",
                    "samples_sha256",
                    "log_sha256",
                ],
            )
            || self.entry["step"] != self.step
            || self.entry["before"] != self.context
            || self.entry["after"] != self.context
        {
            return Err("frequency order/context differs".into());
        }
        if !clock::keys(
            &self.begin,
            &["step", "before", "start_ns", "counters_before"],
        ) || ["step", "before", "start_ns", "counters_before"]
            .iter()
            .any(|k| self.begin[k] != self.entry[k])
        {
            return Err("frequency start journal differs".into());
        }
        let start = statistics::uint(&self.entry["start_ns"])?;
        let end = statistics::uint(&self.entry["end_ns"])?;
        if start < self.last_end {
            return Err("frequency workers overlap".into());
        }
        let cpus: Vec<u64> =
            serde_json::from_value(self.context["isolation"]["cpu_affinity"].clone())?;
        let counters = clock::frequency(
            &self.entry["counters_before"],
            &self.entry["counters_after"],
            &cpus,
            start,
            end,
        )?;
        let wall = total(&self.replay.rows, "wall_ns")?;
        if statistics::compare(&wall, &json!(end - start)).is_gt() {
            return Err("frequency worker cannot contain original windows".into());
        }
        let summary = self.replay.validate()?;
        let cpu = statistics::uint(&self.replay.rows[0]["placement"]["cpu"])?;
        if counters[cpu.to_string()]["mperf_delta"]
            .as_u64()
            .is_none_or(|v| v == 0)
        {
            return Err("no measuring-CPU counter activity".into());
        }
        let service = baseline::service_times(&self.replay.rows)?;
        Ok(
            json!({"step":self.step,"original_metric":summary,"worker_elapsed_ns":end-start,
            "original_wall_ns":wall,"original_timed_ns":total(&self.replay.rows,"timed_ns")?,
            "service":service[&self.replay.case],"measuring_cpu":cpu,"counters":counters}),
        )
    }
}

#[cfg(test)]
mod tests;
