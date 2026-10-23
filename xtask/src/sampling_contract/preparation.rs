//! Borrowed preparation projections shared by capture and original-directory replay.
use crate::{Result, benchmark_contract};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(PartialEq, Serialize)]
struct Workload<'a> {
    #[serde(flatten)]
    fields: BTreeMap<&'a str, &'a Value>,
    initial_rng: &'a Value,
    placement: &'a Value,
}
#[derive(Serialize)]
pub(crate) struct WorkerStart<'a> {
    case: &'a str,
    iterations: &'a Value,
    calibration_batch: &'a Value,
}
pub(crate) struct Preparation<'a> {
    pub(crate) worker_starts: Vec<WorkerStart<'a>>,
    pub(crate) fixtures: BTreeMap<&'a str, &'a Value>,
    workloads: BTreeMap<&'a str, Workload<'a>>,
    traces: BTreeMap<&'a str, Vec<&'a Value>>,
    contracts: BTreeMap<&'a str, (&'a Value, &'a Value)>,
}
impl<'a> Preparation<'a> {
    pub(crate) fn new(records: &'a [Value], calibration: bool) -> Result<Self> {
        let mut result = Self {
            worker_starts: Vec::new(),
            fixtures: BTreeMap::new(),
            workloads: BTreeMap::new(),
            traces: BTreeMap::new(),
            contracts: BTreeMap::new(),
        };
        let mut last = BTreeMap::new();
        for row in records {
            let case = row["case"].as_str().ok_or("missing preparation case")?;
            last.insert(case, &row["calibration_batch"]);
            if !row["fixture"].is_null()
                && if calibration {
                    row["complete"] == true
                } else {
                    row["kind"] == "sample"
                }
            {
                result.fixtures.insert(case, &row["fixture"]);
            }
            if row["index"] == 0 {
                result.worker_starts.push(WorkerStart {
                    case,
                    iterations: &row["iterations"],
                    calibration_batch: &row["calibration_batch"],
                });
                let fields = row["workload"]
                    .as_object()
                    .ok_or("missing workload preparation")?
                    .iter()
                    .filter(|(k, _)| k.as_str() != "rng")
                    .map(|(k, v)| (k.as_str(), v))
                    .collect();
                result.workloads.insert(
                    case,
                    Workload {
                        fields,
                        initial_rng: &row["workload"]["rng"]["next_before"],
                        placement: &row["placement"],
                    },
                );
            }
            if calibration && row["complete"] == true {
                result
                    .contracts
                    .insert(case, (&row["rows"][0], &row["placement"]));
            }
        }
        for row in records {
            let case = row["case"].as_str().ok_or("missing preparation case")?;
            if !calibration || &row["calibration_batch"] == last[case] {
                result
                    .traces
                    .entry(case)
                    .or_default()
                    .push(&row["workload"]["rng"]);
            }
        }
        result
            .traces
            .retain(|_, trace| trace.iter().any(|rng| !rng.is_null()));
        Ok(result)
    }
    pub(crate) fn catalog(&self) -> Value {
        json!({"fixtures":self.fixtures,"workloads":self.workloads,"traces":self.traces})
    }
    pub(crate) fn validate(&self, profile: &str, records: &[Value]) -> Result<()> {
        for row in records {
            let case = row["case"].as_str().ok_or("missing preparation case")?;
            let (source, placement) = self
                .contracts
                .get(case)
                .ok_or("observation case differs from calibration")?;
            benchmark_contract::observation_matches(profile, row, source, placement)?;
        }
        let actual = Preparation::new(records, false)?;
        if actual.workloads != self.workloads || actual.traces != self.traces {
            return Err(
                format!("{profile}: preparation or input replay differs from calibration").into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
