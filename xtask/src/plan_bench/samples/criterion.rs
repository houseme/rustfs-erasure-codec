//! Validate original Criterion data without reproducing stochastic bootstrap CIs.
use crate::{Result, statistics};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    sampling_mode: Mode,
    iters: Vec<Value>,
    times: Vec<Value>,
}
#[derive(Deserialize)]
enum Mode {
    Flat,
    Linear,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Interval {
    confidence_level: f64,
    lower_bound: f64,
    upper_bound: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Estimate {
    confidence_interval: Interval,
    point_estimate: f64,
    standard_error: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Estimates {
    mean: Estimate,
    median: Estimate,
    median_abs_dev: Estimate,
    std_dev: Estimate,
    slope: Option<Estimate>,
}
#[derive(Serialize)]
pub(super) struct Distribution {
    pub samples: usize,
    pub mean_ns: f64,
    median_ns: f64,
    minimum_ns: f64,
    maximum_ns: f64,
    mean_lower_ns: f64,
    mean_upper_ns: f64,
    pub relative_ci_width: f64,
}
impl Estimate {
    fn validate(&self, confidence: f64, positive: bool) -> Result<()> {
        let c = &self.confidence_interval;
        if [
            c.confidence_level,
            c.lower_bound,
            c.upper_bound,
            self.point_estimate,
            self.standard_error,
        ]
        .iter()
        .any(|x| !x.is_finite())
            || c.confidence_level != confidence
            || c.lower_bound < 0.0
            || c.lower_bound > self.point_estimate
            || self.point_estimate > c.upper_bound
            || self.standard_error < 0.0
            || (positive && c.lower_bound <= 0.0)
        {
            return Err("invalid estimate confidence interval or standard error".into());
        }
        Ok(())
    }
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= a.abs().max(b.abs()) * 1e-9
}
fn iterations(value: &Value) -> Result<u64> {
    const EXACT: u64 = 1 << 53;
    if let Some(n) = value.as_u64() {
        return if (1..=EXACT).contains(&n) {
            Ok(n)
        } else {
            Err("iteration count exceeds exact Criterion integer range".into())
        };
    }
    let n = statistics::positive(value)?;
    if n.fract() != 0.0 || n > EXACT as f64 {
        return Err("iteration count must be an exact positive integer".into());
    }
    Ok(n as u64)
}
pub(super) fn analyze(
    sample: Value,
    estimates: Value,
    minimum: usize,
    confidence: f64,
) -> Result<Distribution> {
    let sample: Sample = serde_json::from_value(sample)?;
    let estimates: Estimates = serde_json::from_value(estimates)?;
    let count = sample.iters.len();
    if !(minimum..=100_000).contains(&count) || sample.times.len() != count {
        return Err("incomplete or unbounded paired raw sample arrays".into());
    }
    let counts: Vec<_> = sample.iters.iter().map(iterations).collect::<Result<_>>()?;
    for (i, n) in counts.iter().enumerate() {
        let expected = match sample.sampling_mode {
            Mode::Flat => u128::from(counts[0]),
            Mode::Linear => u128::from(counts[0]) * (i as u128 + 1),
        };
        if u128::from(*n) != expected {
            return Err("raw iteration counts conflict with sampling mode".into());
        }
    }
    let mut values = Vec::with_capacity(count);
    for (time, iters) in sample.times.iter().zip(&sample.iters) {
        statistics::positive(time)?;
        let ratio = statistics::ratio(time, iters)?;
        if ratio <= 0.0 {
            return Err("nonpositive time per iteration".into());
        }
        values.push(ratio);
    }
    let maximum = values.iter().copied().fold(0.0, f64::max);
    let mean = maximum * (values.iter().map(|v| v / maximum).sum::<f64>() / count as f64);
    values.sort_by(f64::total_cmp);
    let median = if count.is_multiple_of(2) {
        let a = values[count / 2 - 1];
        a + (values[count / 2] - a) / 2.0
    } else {
        values[count / 2]
    };
    for (estimate, positive) in [
        (&estimates.mean, true),
        (&estimates.median, true),
        (&estimates.median_abs_dev, false),
        (&estimates.std_dev, false),
    ] {
        estimate.validate(confidence, positive)?;
    }
    match (&sample.sampling_mode, &estimates.slope) {
        (Mode::Flat, None) => {}
        (Mode::Linear, Some(slope)) => {
            slope.validate(confidence, true)?;
            // Weighted average of time/iteration; normalize both factors so
            // finite original values do not overflow intermediate dot products.
            let scale = counts[count - 1] as f64;
            let (mut numerator, mut denominator) = (0.0, 0.0);
            for ((time, iters), n) in sample.times.iter().zip(&sample.iters).zip(&counts) {
                let w = (*n as f64 / scale).powi(2);
                numerator += w * (statistics::ratio(time, iters)? / maximum);
                denominator += w;
            }
            if !close(slope.point_estimate, maximum * (numerator / denominator)) {
                return Err("slope estimate conflicts with raw samples".into());
            }
        }
        _ => return Err("slope presence conflicts with sampling mode".into()),
    }
    if !mean.is_finite()
        || mean <= 0.0
        || !close(mean, estimates.mean.point_estimate)
        || !close(median, estimates.median.point_estimate)
    {
        return Err("mean or median estimate conflicts with raw samples".into());
    }
    let ci = estimates.mean.confidence_interval;
    let width = (ci.upper_bound - ci.lower_bound) / estimates.mean.point_estimate;
    if !width.is_finite() {
        return Err("nonfinite relative confidence width".into());
    }
    Ok(Distribution {
        samples: count,
        mean_ns: mean,
        median_ns: median,
        minimum_ns: values[0],
        maximum_ns: maximum,
        mean_lower_ns: ci.lower_bound,
        mean_upper_ns: ci.upper_bound,
        relative_ci_width: width,
    })
}
