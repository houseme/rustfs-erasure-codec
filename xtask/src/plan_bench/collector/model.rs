use super::*;
use serde::{Deserialize, Serialize};
pub(super) const SCHEMA: &str = "plan-bench-collection-v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub schema: String,
    pub origin: samples::Origin,
    pub session_sha256: String,
    pub baseline_sha256: String,
    pub candidate_sha256: String,
    pub baseline_members: BTreeMap<String, String>,
    pub renderer_sha256: String,
    pub analyzer_sha256: String,
    pub repetitions: u8,
    pub minimum_samples: usize,
    pub confidence_level: f64,
    pub max_ci_width: f64,
    pub max_drift: f64,
    pub warm_up_seconds: u64,
    pub measurement_seconds: u64,
    pub stage_seconds: u64,
    pub window_seconds: u64,
    pub gap_seconds: u64,
}
impl Plan {
    pub fn validate(&self) -> Result<()> {
        for pin in [
            &self.session_sha256,
            &self.baseline_sha256,
            &self.candidate_sha256,
            &self.renderer_sha256,
            &self.analyzer_sha256,
        ] {
            valid_digest(pin)?;
        }
        if self.schema != SCHEMA
            || self.session_sha256.bytes().all(|b| b == b'0')
            || self
                .baseline_members
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
                != execution::baseline_catalog()
            || !(1..=60).contains(&self.warm_up_seconds)
            || !(1..=600).contains(&self.measurement_seconds)
            || !(1..=86400).contains(&self.stage_seconds)
            || !(1..=604800).contains(&self.window_seconds)
            || !(1..=60).contains(&self.gap_seconds)
            || self.stage_seconds > self.window_seconds
        {
            return Err("invalid predeclared collection plan".into());
        }
        for pin in self.baseline_members.values() {
            valid_digest(pin)?;
        }
        samples::validate_configuration(self.document(Vec::new()))
    }
    pub fn document(&self, attempts: Vec<Value>) -> Value {
        json!({"schema":"plan-bench-statistical-inputs-v1","origin":self.origin,"baseline_sha256":self.baseline_sha256,"candidate_sha256":self.candidate_sha256,"repetitions":self.repetitions,"minimum_samples":self.minimum_samples,"confidence_level":self.confidence_level,"max_ci_width":self.max_ci_width,"max_drift":self.max_drift,"attempts":attempts})
    }
    pub fn args(&self) -> Vec<String> {
        vec![
            if self.origin == samples::Origin::Synthetic {
                "--test"
            } else {
                "--bench"
            }
            .into(),
            "--noplot".into(),
            "--sample-size".into(),
            self.minimum_samples.to_string(),
            "--warm-up-time".into(),
            self.warm_up_seconds.to_string(),
            "--measurement-time".into(),
            self.measurement_seconds.to_string(),
            "--confidence-level".into(),
            self.confidence_level.to_string(),
        ]
    }
    pub fn pin(&self, index: usize) -> &str {
        if index.is_multiple_of(4) || index % 4 == 3 {
            &self.baseline_sha256
        } else {
            &self.candidate_sha256
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactCopy {
    pub canonical: String,
    pub original: String,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    pub repetition: u8,
    pub stage: String,
    pub binary_sha256: String,
    pub arguments: Vec<String>,
    pub start_sha256: String,
    pub outcome_sha256: Option<String>,
    pub stdout_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
    pub execution: Option<Value>,
    pub copies: Vec<ArtifactCopy>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: String,
    pub plan_sha256: String,
    pub controller_sha256: String,
    pub completed: bool,
    pub execution_sha256: Option<String>,
    pub samples_sha256: Option<String>,
    pub steps: Vec<Step>,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub(super) struct Check {
    pub accepted: bool,
    pub observed_steps: usize,
    pub execution: Option<execution::Report>,
    pub errors: Vec<String>,
    pub capture_provenance_verified: bool,
    pub performance_accepted: bool,
}
