//! One owned functional campaign. Pins are trust inputs, not an execution attestation.
use super::*;
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Instant};

#[cfg(test)]
mod tests;

const SCHEMA: &str = "plan-bench-functional-campaign-v1";
const STAGES: [&str; 4] = ["A1", "B1", "B2", "A2"];
const SECOND: u64 = 1_000_000_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: String,
    mode: String,
    nonce: String,
    baseline_sha256: String,
    candidate_sha256: String,
    stage_seconds: u64,
    window_seconds: u64,
    gap_seconds: u64,
}
impl Plan {
    fn validate(&self) -> Result<()> {
        for pin in [&self.nonce, &self.baseline_sha256, &self.candidate_sha256] {
            valid_digest(pin)?;
        }
        if self.schema != SCHEMA
            || self.mode != "functional"
            || !(1..=1200).contains(&self.stage_seconds)
            || !(1..=4800).contains(&self.window_seconds)
            || !(1..=60).contains(&self.gap_seconds)
            || self.stage_seconds > self.window_seconds
        {
            return Err("unsupported functional campaign or execution budget".into());
        }
        Ok(())
    }
    fn pin(&self, index: usize) -> &str {
        if index == 0 || index == 3 {
            &self.baseline_sha256
        } else {
            &self.candidate_sha256
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    plan_sha256: String,
    stage: String,
    previous_sha256: String,
    started_ns: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct End {
    start_sha256: String,
    ended_ns: u64,
    receipt_sha256: Option<String>,
    error: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    start_sha256: String,
    end_sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CampaignReceipt {
    schema: String,
    plan_sha256: String,
    stages: Vec<Link>,
    ended_ns: u64,
    failure: Option<String>,
}
#[derive(Serialize)]
struct CampaignCheck {
    accepted: bool,
    stages: usize,
    observed: usize,
    errors: Vec<String>,
    performance_accepted: bool,
}

fn save<T: Serialize>(path: &Path, value: &T) -> Result<String> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write(path, &bytes)?;
    Ok(digest(&bytes))
}
fn elapsed(origin: Instant) -> Result<u64> {
    Ok(origin.elapsed().as_nanos().try_into()?)
}
fn interval(plan: &Plan, start: u64, end: u64, previous: u64) -> Result<()> {
    if start < previous
        || end < start
        || end > plan.window_seconds * SECOND
        || start - previous > plan.gap_seconds * SECOND
        || end - start > plan.stage_seconds * SECOND
    {
        return Err("campaign overlaps, exceeds a deadline, or has a discontinuous gap".into());
    }
    Ok(())
}

pub(crate) fn capture(args: &Args) -> Result<bool> {
    if !cfg!(unix) {
        return Err("functional campaign capture requires Unix".into());
    }
    let mut nonce = [0; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut nonce)?;
    let plan = Plan {
        schema: SCHEMA.into(),
        mode: "functional".into(),
        nonce: artifact::snapshot::hex(&nonce),
        baseline_sha256: args.required("--baseline-sha256")?.into(),
        candidate_sha256: args.required("--candidate-sha256")?.into(),
        stage_seconds: args.get("--stage-seconds", "120").parse()?,
        window_seconds: args.get("--window-seconds", "600").parse()?,
        gap_seconds: args.get("--gap-seconds", "5").parse()?,
    };
    plan.validate()?;
    let a = load(Path::new(args.required("--baseline")?), plan.pin(0))?;
    let b = load(Path::new(args.required("--candidate")?), plan.pin(1))?;
    compatible(&a.manifest, &b.manifest)?;
    let root = Path::new(args.required("--output")?);
    new_directory(root)?;
    let root = fs::canonicalize(root)?;
    // Prepare all self-contained inputs before the monotonic window. No imports,
    // source switches, builds, resumptions, or caller-supplied stages inside it.
    for (index, stage) in STAGES.iter().enumerate() {
        install(
            if index == 0 || index == 3 { &a } else { &b },
            &root.join(stage),
        )?;
    }
    a.finish()?;
    b.finish()?;
    let plan_pin = save(&root.join("campaign.json"), &plan)?;
    println!("campaign plan sha256: {plan_pin}");
    use std::io::Write;
    std::io::stdout().flush()?;
    let origin = Instant::now();
    let mut receipt = CampaignReceipt {
        schema: SCHEMA.into(),
        plan_sha256: plan_pin.clone(),
        stages: Vec::with_capacity(4),
        ended_ns: 0,
        failure: None,
    };
    let mut previous_pin = plan_pin.clone();
    let mut previous_end = 0;
    for (index, stage) in STAGES.iter().enumerate() {
        let started_ns = elapsed(origin)?;
        if interval(&plan, started_ns, started_ns, previous_end).is_err() {
            receipt.failure = Some("campaign deadline or inter-stage gap expired".into());
            break;
        }
        let directory = root.join(stage);
        let start_pin = save(
            &directory.join("start.json"),
            &Start {
                plan_sha256: plan_pin.clone(),
                stage: (*stage).into(),
                previous_sha256: previous_pin,
                started_ns,
            },
        )?;
        // Bound the worker by both remaining deadlines; cleanup overhead is also
        // recorded and can refuse acceptance if it exceeds the declared window.
        let remaining =
            (plan.window_seconds * SECOND - started_ns).min(plan.stage_seconds * SECOND);
        let captured = capture_prepared(
            &directory,
            plan.pin(index),
            stage,
            Duration::from_nanos(remaining),
        );
        let end = End {
            start_sha256: start_pin.clone(),
            ended_ns: elapsed(origin)?,
            receipt_sha256: captured.as_ref().ok().map(|(_, pin)| pin.clone()),
            error: captured.as_ref().err().map(ToString::to_string),
        };
        let passed = captured.is_ok_and(|(check, _)| check.accepted)
            && interval(&plan, started_ns, end.ended_ns, previous_end).is_ok();
        let end_pin = save(&directory.join("end.json"), &end)?;
        receipt.stages.push(Link {
            start_sha256: start_pin,
            end_sha256: end_pin.clone(),
        });
        previous_pin = end_pin;
        previous_end = end.ended_ns;
        if !passed {
            receipt.failure = Some(format!("{stage} refused; preserve all stage originals"));
            break;
        }
    }
    receipt.ended_ns = elapsed(origin)?;
    let receipt_pin = save(&root.join("campaign.receipt.json"), &receipt)?;
    // Publish terminal pins even when replay refuses partial or corrupted output.
    println!("campaign receipt sha256: {receipt_pin}");
    let check = verify(&root, &plan_pin, &receipt_pin)?;
    let bytes = serde_json::to_vec_pretty(&check)?;
    write(&root.join("campaign.replay.json"), &bytes)?;
    println!("{}", std::str::from_utf8(&bytes)?);
    Ok(check.accepted)
}

fn verify(root: &Path, plan_pin: &str, receipt_pin: &str) -> Result<CampaignCheck> {
    valid_digest(plan_pin)?;
    valid_digest(receipt_pin)?;
    // Fixed four-stage budget, with bounded members and one final re-read of
    // every original. Earlier stages cannot change unnoticed during replay.
    let mut snapshot = Snapshot::with_budget(root, 4 * 256 * 1024 * 1024)?;
    let plan: Plan = serde_json::from_value(snapshot.json_bound("campaign.json", Some(plan_pin))?)?;
    plan.validate()?;
    let receipt: CampaignReceipt =
        serde_json::from_value(snapshot.json_bound("campaign.receipt.json", Some(receipt_pin))?)?;
    if receipt.schema != SCHEMA || receipt.plan_sha256 != plan_pin || receipt.stages.len() > 4 {
        return Err("campaign receipt identity or stage count mismatch".into());
    }
    let mut check = CampaignCheck {
        accepted: false,
        stages: receipt.stages.len(),
        observed: 0,
        errors: Vec::new(),
        performance_accepted: false,
    };
    let mut previous_pin = plan_pin;
    let mut previous_end = 0;
    let mut baseline_manifest = None;
    for (index, link) in receipt.stages.iter().enumerate() {
        let stage = STAGES[index];
        valid_digest(&link.start_sha256)?;
        valid_digest(&link.end_sha256)?;
        let start: Start = serde_json::from_value(
            snapshot.json_bound(&format!("{stage}/start.json"), Some(&link.start_sha256))?,
        )?;
        let end: End = serde_json::from_value(
            snapshot.json_bound(&format!("{stage}/end.json"), Some(&link.end_sha256))?,
        )?;
        if start.plan_sha256 != plan_pin
            || start.stage != stage
            || start.previous_sha256 != previous_pin
            || end.start_sha256 != link.start_sha256
        {
            return Err("campaign stage order, chain or identity mismatch".into());
        }
        if let Err(error) = interval(&plan, start.started_ns, end.ended_ns, previous_end) {
            check.errors.push(format!("{stage}: {error}"));
        }
        match &end.receipt_sha256 {
            Some(pin) => {
                let (stage_check, manifest) = verify_snapshot(
                    &mut snapshot,
                    &format!("{stage}/"),
                    plan.pin(index),
                    pin,
                    stage,
                )?;
                if index == 0 {
                    baseline_manifest = Some(manifest);
                } else if index == 1
                    && let Some(a) = &baseline_manifest
                {
                    compatible(a, &manifest)?;
                }
                check.observed += stage_check.observed;
                check.errors.extend(
                    stage_check
                        .errors
                        .into_iter()
                        .map(|e| format!("{stage}: {e}")),
                );
            }
            None => check
                .errors
                .push(format!("{stage}: no completed capture receipt")),
        }
        if end.error.is_some() {
            check
                .errors
                .push(format!("{stage}: capture error retained"));
        }
        previous_pin = &link.end_sha256;
        previous_end = end.ended_ns;
    }
    if receipt.stages.len() != 4 || receipt.failure.is_some() {
        check
            .errors
            .push("campaign stopped before four accepted stages".into());
    }
    if receipt.ended_ns < previous_end
        || receipt.ended_ns > plan.window_seconds * SECOND
        || receipt.ended_ns - previous_end > plan.gap_seconds * SECOND
    {
        check.errors.push("invalid terminal campaign window".into());
    }
    snapshot.finish()?;
    check.accepted = check.errors.is_empty();
    Ok(check)
}
pub(crate) fn replay(args: &Args) -> Result<bool> {
    let check = verify(
        Path::new(args.required("--input")?),
        args.required("--plan-sha256")?,
        args.required("--receipt-sha256")?,
    )?;
    println!("{}", serde_json::to_string_pretty(&check)?);
    Ok(check.accepted)
}
