use super::numeric::{self, center, close_product, positive, uint};
use crate::{Result, sampling_contract};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(super) fn plan(mut cpus: Vec<u64>) -> Value {
    cpus.sort_unstable();
    let reverse: Vec<_> = cpus.iter().rev().copied().collect();
    let mut result = vec![
        json!([
            "same-buffer-forward-reverse",
            cpus.iter().chain(&reverse).collect::<Vec<_>>()
        ]),
        json!([
            "same-buffer-reverse-forward",
            reverse.iter().chain(&cpus).collect::<Vec<_>>()
        ]),
    ];
    for (wave, order) in [&cpus, &reverse].into_iter().enumerate() {
        for cpu in order {
            result.push(json!([format!("fresh-{wave}-cpu-{cpu}"), [cpu]]));
        }
    }
    result.into()
}
pub(super) fn analyze(
    records: &[Value],
    sequence: &[u64],
    iterations: u64,
    page: u64,
) -> Result<Value> {
    if records.len()
        != sequence
            .len()
            .checked_mul(12)
            .ok_or("placement plan overflow")?
    {
        return Err("every placement warmup and sample is required".into());
    }
    let mut fixture = None;
    let mut backend = None;
    let mut groups = Vec::new();
    for (epoch, cpu) in sequence.iter().enumerate() {
        let group = &records[epoch * 12..(epoch + 1) * 12];
        for (index, row) in group.iter().enumerate() {
            for (key, value) in [
                ("epoch", epoch as u64),
                ("cpu", *cpu),
                ("index", index as u64),
                ("iterations", iterations),
            ] {
                if uint(&row[key])? != value {
                    return Err("placement observation order differs from the declared plan".into());
                }
            }
            if row["kind"] != if index < 3 { "warmup" } else { "sample" } {
                return Err("placement observation order differs from the declared plan".into());
            }
            if row["case"] != "encode:10:4:524288"
                || row["call_entry"] != "Vec<Vec<u8>>"
                || row["backend"].as_str().is_none_or(str::is_empty)
            {
                return Err("ordinary encode contract is required".into());
            }
            sampling_contract::fixture(row, "small_file", Some(page))?;
            sampling_contract::telemetry(row, Some(&[*cpu]))?;
            if fixture.is_some_and(|previous| previous != &row["fixture"])
                || backend.is_some_and(|previous| previous != &row["backend"])
            {
                return Err("same-worker caller geometry or selected backend changed".into());
            }
            fixture = Some(&row["fixture"]);
            backend = Some(&row["backend"]);
            for key in ["ns_per_iter", "wall_ns", "timed_ns"] {
                positive(&row[key])?;
            }
            if !close_product(&row["ns_per_iter"], iterations, &row["timed_ns"])? {
                return Err("placement timed interval differs".into());
            }
            for point in [&row["faults_before"], &row["faults_after"]] {
                if !point.as_object().is_some_and(|m| {
                    m.len() == 3
                        && ["minor", "major", "cpu"]
                            .iter()
                            .all(|key| m.contains_key(*key))
                }) || uint(&point["cpu"])? != *cpu
                {
                    return Err("missing or invalid fault/CPU counters".into());
                }
                uint(&point["minor"])?;
                uint(&point["major"])?;
            }
            for key in ["minor", "major"] {
                if uint(&row["faults_after"][key])? < uint(&row["faults_before"][key])? {
                    return Err("missing or invalid fault/CPU counters".into());
                }
            }
        }
        let measured = &group[3..];
        let (median, spread) = center(measured.iter().map(|r| &r["ns_per_iter"]))?;
        let total = |key| -> Result<u64> {
            measured.iter().try_fold(0u64, |sum, r| {
                sum.checked_add(uint(&r["faults_after"][key])? - uint(&r["faults_before"][key])?)
                    .ok_or_else(|| "placement fault count overflow".into())
            })
        };
        let short = measured
            .iter()
            .filter(|r| {
                r["wall_ns"].as_f64().unwrap_or(0.0) < sampling_contract::MINIMUM_WALL_NS as f64
                    || r["timed_ns"].as_f64().unwrap_or(0.0)
                        < sampling_contract::MINIMUM_TIMED_NS as f64
            })
            .count();
        groups.push(json!({"epoch":epoch,"cpu":cpu,"median_ns_per_iter":median,"central_spread":spread,"short_samples":short,"measured_minor_faults":total("minor")?,"measured_major_faults":total("major")?}));
    }
    let mut repeats = Vec::new();
    for cpu in sequence.iter().collect::<BTreeSet<_>>() {
        let pair: Vec<_> = groups.iter().filter(|g| g["cpu"] == *cpu).collect();
        if pair.len() == 2 {
            repeats.push(json!({"cpu":cpu,"absolute_drift":(numeric::ratio(&pair[1]["median_ns_per_iter"],&pair[0]["median_ns_per_iter"])?-1.0).abs()}));
        }
    }
    Ok(
        json!({"groups":groups,"same_worker_repeats":repeats,"release_approval":false,"candidate_attribution":null}),
    )
}
