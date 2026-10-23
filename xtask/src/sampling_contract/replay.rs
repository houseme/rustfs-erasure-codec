//! Original worker journal and frozen calibration agreement in one borrowed model.
use crate::{Result, benchmark_contract, release_rows, statistics};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Calibration {
    reference: Option<(Value, Value)>,
    fixture: Value,
    workload: Value,
    trace: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub(crate) rows: Vec<Value>,
    raw: String,
    pub(crate) profile: String,
    pub(crate) case: String,
    iterations: u64,
    revision: String,
    calibration: Calibration,
    allowed_cpus: Option<Vec<u64>>,
    expected_page_size: Option<u64>,
}

// Match sorted JSON agreement, including integer/float types and signed zero.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            a.is_f64() == b.is_f64() && a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| same(a, b)))
        }
        _ => a == b,
    }
}
fn journal(text: &str) -> Result<(Vec<Value>, Vec<Value>)> {
    let (mut samples, mut timing) = (Vec::new(), Vec::new());
    for line in text.split_inclusive('\n') {
        let (target, payload) = if let Some(s) = line.strip_prefix("RSE_SAMPLE ") {
            (&mut samples, s)
        } else if let Some(s) = line.strip_prefix("RSE_TIMING ") {
            (&mut timing, s)
        } else {
            continue;
        };
        match serde_json::from_str::<Value>(payload) {
            Ok(value) => target.push(value),
            Err(_) if !line.ends_with('\n') => {}
            Err(e) => return Err(format!("raw original worker journal differs: {e}").into()),
        }
    }
    Ok((samples, timing))
}
impl Request {
    pub(crate) fn matches_context(&self, step: &Value, context: &Value) -> bool {
        step["profile"] == self.profile
            && step["case"] == self.case
            && step["iterations"] == self.iterations
            && context["isolation"]["cpu_affinity"] == json!(self.allowed_cpus)
            && context["runtime"]["page_size"] == json!(self.expected_page_size)
    }
    pub(crate) fn validate(&self) -> Result<Value> {
        let (samples, timing) = journal(&self.raw)?;
        const FIELDS: &[&str] = &[
            "case",
            "index",
            "iterations",
            "wall_ns",
            "timed_ns",
            "fixture",
            "workload",
            "placement",
            "before",
            "after",
        ];
        if samples.len() != self.rows.len()
            || samples.iter().zip(&self.rows).any(|(a, b)| !same(a, b))
            || timing.len() != 12
            || self.rows.len() != 12
            || timing.iter().zip(&self.rows).any(|(t, r)| {
                FIELDS
                    .iter()
                    .any(|k| t.get(*k).zip(r.get(*k)).is_none_or(|(a, b)| !same(a, b)))
            })
        {
            return Err("raw original worker journal differs".into());
        }
        let contract = release_rows::Request {
            profile: self.profile.clone(),
            revision: self.revision.clone(),
            rows: Value::Null,
            iteration_counts: None,
            default_iterations: Some(self.iterations),
            complete: false,
        };
        let expected = BTreeSet::from([self.case.clone()]);
        super::batch::validate_groups(
            &self.rows,
            &contract,
            &expected,
            self.allowed_cpus.as_deref(),
            self.expected_page_size,
        )?;
        let (reference, placement) = self
            .calibration
            .reference
            .as_ref()
            .ok_or("observation case differs from calibration")?;
        for row in &self.rows {
            benchmark_contract::observation_matches(&self.profile, row, reference, placement)?;
        }
        let prepared = super::preparation::Preparation::new(&self.rows, false)?.catalog();
        for (key, expected, label) in [
            ("fixtures", &self.calibration.fixture, "caller geometry"),
            ("workloads", &self.calibration.workload, "workload"),
            ("traces", &self.calibration.trace, "input replay"),
        ] {
            if &prepared[key][&self.case] != expected {
                return Err(
                    format!("{label} differs from frozen input; no temporal attribution").into(),
                );
            }
        }
        let metric = release_rows::metric(&self.profile)?;
        let (index, spread) = super::batch::select(&self.rows, &self.case, metric, false)?;
        Ok(
            json!({"metric":metric,"value":statistics::positive(&self.rows[index]["rows"][0][metric])?,"central_spread":spread["central_spread"]}),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests;
