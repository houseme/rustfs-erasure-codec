//! Redirect to owned files; bounded polling never waits on an unbounded pipe.
use super::{model::Outcome, write};
use crate::{Result, artifact::snapshot::MEMBER_LIMIT, diagnostics::owned::Process};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(super) fn execute(
    command: Command,
    output: &Path,
    name: &str,
    timeout: Duration,
) -> Result<Outcome> {
    execute_with_cancel(command, output, name, timeout, cancellation()?)
}
pub(super) fn cancellation() -> Result<&'static AtomicBool> {
    static CANCELLED: AtomicBool = AtomicBool::new(false);
    static HANDLER: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    HANDLER
        .get_or_init(|| {
            ctrlc::set_handler(|| CANCELLED.store(true, Ordering::Relaxed))
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| format!("cannot install owned-worker cancellation: {e}"))?;
    Ok(&CANCELLED)
}
pub(super) fn execute_with_cancel(
    command: Command,
    output: &Path,
    name: &str,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<Outcome> {
    execute_guarded(command, output, name, timeout, cancelled, |_| Ok(()))
}
pub(super) fn execute_guarded(
    mut command: Command,
    output: &Path,
    name: &str,
    timeout: Duration,
    cancelled: &AtomicBool,
    mut guard: impl FnMut(bool) -> Result<()>,
) -> Result<Outcome> {
    let stdout = output.join(format!("{name}.stdout"));
    let stderr = output.join(format!("{name}.stderr"));
    let create = |p| fs::OpenOptions::new().write(true).create_new(true).open(p);
    command
        .stdout(Stdio::from(create(&stdout)?))
        .stderr(Stdio::from(create(&stderr)?));
    if cancelled.load(Ordering::Relaxed) {
        return Ok(Outcome {
            exit_code: None,
            reason: "cancelled".into(),
            reaped: true,
        });
    }
    let mut process = match Process::spawn(command) {
        Ok(p) => p,
        Err(_) => {
            return Ok(Outcome {
                exit_code: None,
                reason: "spawn_failed".into(),
                reaped: true,
            });
        }
    };
    let oversized = || {
        [&stdout, &stderr]
            .iter()
            .any(|p| fs::metadata(p).map_or(true, |m| m.len() > MEMBER_LIMIT))
    };
    let start = Instant::now();
    let reason = loop {
        if oversized() {
            break "output_limit";
        }
        if guard(false).is_err() {
            break "guard_failed";
        }
        match process.poll() {
            Ok(Some(status)) => {
                if oversized() {
                    break "output_limit";
                }
                if guard(true).is_err() {
                    break "guard_failed";
                }
                return Ok(Outcome {
                    exit_code: status.code(),
                    reason: "exited".into(),
                    reaped: true,
                });
            }
            Ok(None) => {}
            Err(_) => break "poll_error",
        }
        if cancelled.load(Ordering::Relaxed) {
            break "cancelled";
        }
        if start.elapsed() >= timeout {
            break "timeout";
        }
        if [&stdout, &stderr]
            .iter()
            .any(|p| fs::metadata(p).map_or(true, |m| m.len() > MEMBER_LIMIT))
        {
            break "output_limit";
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let reaped = process.stop().is_ok();
    Ok(Outcome {
        exit_code: None,
        reason: reason.into(),
        reaped,
    })
}
pub(super) fn checked(
    command: Command,
    output: &Path,
    name: &str,
    timeout: Duration,
) -> Result<()> {
    let outcome = execute(command, output, name, timeout)?;
    write(
        &output.join(format!("{name}.outcome.json")),
        &serde_json::to_vec_pretty(&outcome)?,
    )?;
    if outcome.exit_code != Some(0) || outcome.reason != "exited" || !outcome.reaped {
        return Err(format!("{name} failed; preserve the output directory and logs").into());
    }
    Ok(())
}
