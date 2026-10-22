use super::numeric::uint;
use crate::{Result, sampling_contract};
use serde_json::{Value, json};
pub(super) fn analyze(records: &[Value], iterations: u64, page: u64) -> Result<Value> {
    if records.len() != 12 {
        return Err("all twelve instrumented observations are required".into());
    }
    let mut complete = true;
    let mut sources = Vec::new();
    let mut counts = Vec::new();
    let mut transitions = Vec::new();
    for record in records {
        if record["entry"] != "Vec<ShardSlot<Vec<u8>>>"
            || record["source_lifetime"] != "worker"
            || record["clone_lifetime"] != "iteration"
            || uint(&record["iterations"])? != iterations
            || uint(&record["page_size"])? != page
        {
            return Err("incorrect Slot workload contract".into());
        }
        let overflow = uint(&record["overflow"])?;
        let transition = uint(&record["transitions"])?;
        if transition >= iterations {
            return Err("invalid bounded histogram counters".into());
        }
        let phases = record["phase_ns"]
            .as_object()
            .filter(|m| {
                m.len() == 3
                    && ["clone", "reconstruct", "drop"]
                        .iter()
                        .all(|key| m.contains_key(*key))
            })
            .ok_or("complete positive phase intervals are required")?;
        for v in phases.values() {
            if uint(v)? == 0 {
                return Err("complete positive phase intervals are required".into());
            }
        }
        let buckets = record["layouts"]
            .as_array()
            .filter(|v| (1..=128).contains(&v.len()))
            .ok_or("invalid bounded histogram")?;
        let mut observed = u128::from(overflow);
        sampling_contract::positions(&record["source"], 6, 65536, page)?;
        for bucket in buckets {
            let count = uint(&bucket["count"])?;
            if count == 0 || count > iterations {
                return Err("invalid layout count".into());
            }
            observed += u128::from(count);
            sampling_contract::positions(&bucket["buffers"], 6, 65536, page)?;
        }
        if observed != u128::from(iterations) {
            return Err("missing clone observations".into());
        }
        complete &= overflow == 0;
        sources.push(&record["source"]);
        counts.push(buckets.len());
        transitions.push(transition);
    }
    Ok(
        json!({"histogram_complete":complete,"source_layouts":sources,"clone_layout_counts":counts,"clone_layout_transitions":transitions}),
    )
}
