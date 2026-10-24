use crate::Result;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// Own one newly-created source file. Cleanup never removes edited/symlinked input.
pub struct File {
    path: PathBuf,
    text: String,
    active: bool,
}
impl File {
    pub fn create(path: &Path, text: String) -> Result<Self> {
        use std::io::Write;
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("refuse to overwrite or create allocation probe: {error}"))?;
        target.write_all(text.as_bytes())?;
        target.flush()?;
        Ok(Self {
            path: path.to_owned(),
            text,
            active: true,
        })
    }
    pub fn preserve(&mut self) {
        self.active = false;
    }
    fn remove_unchanged(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.is_file() || fs::read(&self.path)? != self.text.as_bytes() {
            return Err("changed allocation probe; preserve it".into());
        }
        fs::remove_file(&self.path)?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<()> {
        self.active = false;
        self.remove_unchanged()
    }
}
impl Drop for File {
    fn drop(&mut self) {
        if self.active
            && let Err(error) = self.remove_unchanged()
        {
            eprintln!("probe cleanup failed; preserve it: {error}");
        }
    }
}

#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
use darwin::group_exited as darwin_group_exited;

/// A non-atomic group read must not turn a partial view into exit evidence.
#[cfg(any(target_os = "macos", test))]
enum ExitObservation<T> {
    Exited(Vec<T>),
    Changed,
    PendingExit,
    Unconfirmed(&'static str),
}

#[cfg(any(target_os = "macos", test))]
fn confirm_exited_group<T: Ord>(
    mut observe: impl FnMut() -> ExitObservation<T>,
    mut continue_polling: impl FnMut(bool) -> bool,
) -> std::result::Result<(), &'static str> {
    let mut previous: Option<Vec<T>> = None;
    loop {
        let changed = match observe() {
            ExitObservation::Exited(mut current) => {
                current.sort_unstable();
                if current.is_empty() || current.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err("invalid complete owned-group observation");
                }
                if let Some(before) = &previous {
                    return if current
                        .iter()
                        .all(|member| before.binary_search(member).is_ok())
                    {
                        Ok(())
                    } else {
                        Err("member identity changed between snapshots")
                    };
                }
                previous = Some(current);
                false
            }
            ExitObservation::Changed | ExitObservation::PendingExit => {
                // Neither a partial read nor an identified live member proves
                // exit. Wait within the same budget, discarding any earlier
                // complete view before requiring two fresh exited snapshots.
                previous = None;
                true
            }
            ExitObservation::Unconfirmed(reason) => return Err(reason),
        };
        if !continue_polling(changed) {
            return Err("complete matching exit observations were not obtained");
        }
    }
}

/// Retain the child identity until its newly-created process group is stopped.
pub(crate) struct Process {
    pub(crate) child: Child,
    status: Option<ExitStatus>,
}
impl Process {
    pub(crate) fn spawn(mut command: Command) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        Ok(Self {
            child: command.spawn()?,
            status: None,
        })
    }
    pub(crate) fn poll(&mut self) -> Result<Option<ExitStatus>> {
        if self.status.is_none() {
            #[cfg(unix)]
            {
                // Observe exit without reaping: the child PID must continue to
                // anchor its group until every remaining worker is signalled.
                // SAFETY: siginfo_t is a C output record; all-zero fields are
                // valid and distinguish WNOHANG's no-event result.
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                // SAFETY: this handle owns the unwaited child and `info` is a
                // valid writable output. WNOWAIT preserves that ownership.
                if unsafe {
                    libc::waitid(
                        libc::P_PID,
                        self.child.id(),
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                } != 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                if info.si_signo == 0 {
                    return Ok(None);
                }
                // A wrapper may exit successfully or fail while its workers
                // remain alive, including workers that ignored SIGTERM.
                self.signal_group(libc::SIGKILL)?;
                self.status = Some(self.child.wait()?);
            }
            #[cfg(not(unix))]
            {
                self.status = self.child.try_wait()?;
            }
        }
        Ok(self.status)
    }
    #[cfg(unix)]
    fn signal_group(&self, signal: libc::c_int) -> Result<()> {
        let pid: libc::pid_t = self
            .child
            .id()
            .try_into()
            .map_err(|_| "child PID is out of range")?;
        // SAFETY: spawn created this group and its unreaped leader still anchors
        // its ID against reuse. Darwin cannot query an exited leader's getpgid;
        // use the original group ID, never a worker's possibly changed group.
        // No caller-supplied or reaped PID can reach this operation.
        if unsafe { libc::kill(-pid, signal) } != 0 {
            let error = std::io::Error::last_os_error();
            #[cfg(target_os = "macos")]
            if error.raw_os_error() == Some(libc::EPERM) {
                // Darwin can reject signals to an entirely exited group.
                // Confirm every member, including unreaped worker zombies;
                // a live member or an incomplete query keeps the original error.
                if darwin_group_exited(pid) {
                    return Ok(());
                }
            }
            // Some systems omit an already-exited leader from kill's targets.
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error.into());
            }
        }
        Ok(())
    }
    fn signal(&mut self, force: bool) -> Result<()> {
        if self.poll()?.is_some() {
            return Ok(());
        }
        #[cfg(unix)]
        {
            self.signal_group(if force { libc::SIGKILL } else { libc::SIGTERM })?;
        }
        #[cfg(not(unix))]
        {
            let _ = force;
            self.child.kill()?;
        }
        Ok(())
    }
    pub(crate) fn stop(&mut self) -> Result<()> {
        self.signal(false)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.poll()?.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.poll()?.is_none() {
            self.signal(true)?;
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.poll()?.is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if self.status.is_none() {
            return Err("owned child did not exit; preserve generated inputs".into());
        }
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.status.is_none()
            && let Err(error) = self.stop()
        {
            eprintln!("owned worker cleanup failed: {error}");
        }
    }
}

pub struct Outcome {
    pub result: Result<()>,
    pub reaped: bool,
}
pub fn run(command: Command, cancelled: &AtomicBool) -> Outcome {
    let mut process = match Process::spawn(command) {
        Ok(process) => process,
        Err(error) => {
            return Outcome {
                result: Err(error),
                reaped: true,
            };
        }
    };
    let result = loop {
        match process.poll() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!("native diagnostic worker failed: {status}").into())
                };
            }
            Err(error) => break Err(error),
            Ok(None) => {}
        }
        if cancelled.load(Ordering::Relaxed) {
            break process
                .stop()
                .and_then(|()| Err("native diagnostic cancelled; owned worker stopped".into()));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if process.status.is_none()
        && let Err(error) = process.stop()
    {
        return Outcome {
            result: Err(error),
            reaped: false,
        };
    }
    Outcome {
        result,
        reaped: process.status.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "rse-owned-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        root
    }
    #[test]
    fn failure_cleanup_removes_only_unchanged_owned_input() {
        let root = directory();
        let path = root.join("probe.rs");
        {
            let _owned = File::create(&path, "owned".into()).unwrap();
            assert!(File::create(&path, "foreign".into()).is_err());
        }
        assert!(!path.exists());
        let owned = File::create(&path, "owned".into()).unwrap();
        fs::write(&path, "changed").unwrap();
        assert!(owned.finish().is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "changed");
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn exited_leader_cannot_leave_a_background_worker_running() {
        for code in [0, 7] {
            let root = directory();
            let mut command = Command::new("sh");
            command.current_dir(&root).args([
                "-c",
                &format!(
                    "(trap '' TERM; printf ready > ready; sleep 0.2; printf escaped > escaped) &\n\
                     while [ ! -f ready ]; do :; done\nexit {code}"
                ),
            ]);
            let outcome = run(command, &AtomicBool::new(false));
            // The bounded background worker would publish after its leader exits.
            // Let that deadline pass even if the controller already returned.
            std::thread::sleep(Duration::from_millis(300));
            let escaped = root.join("escaped").exists();
            fs::remove_dir_all(&root).unwrap();
            assert_eq!(outcome.result.is_ok(), code == 0);
            assert!(outcome.reaped);
            assert!(!escaped, "background worker survived leader exit {code}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn cancellation_cannot_leave_a_worker_that_ignores_term() {
        let root = directory();
        let mut command = Command::new("sh");
        command.current_dir(&root).args([
            "-c",
            "(trap '' TERM; printf ready > ready; sleep 0.2; printf escaped > escaped) & wait",
        ]);
        let cancelled = AtomicBool::new(false);
        let (outcome, ready) = std::thread::scope(|scope| {
            let ready = scope.spawn(|| {
                let deadline = Instant::now() + Duration::from_secs(2);
                while !root.join("ready").exists() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                let ready = root.join("ready").exists();
                cancelled.store(true, Ordering::Relaxed);
                ready
            });
            (run(command, &cancelled), ready.join().unwrap())
        });
        std::thread::sleep(Duration::from_millis(300));
        let escaped = root.join("escaped").exists();
        fs::remove_dir_all(&root).unwrap();
        assert!(ready, "background worker did not start");
        assert!(outcome.result.is_err());
        assert!(outcome.reaped);
        assert!(!escaped, "background worker survived cancellation");
    }
    #[test]
    fn exit_confirmation_restarts_after_an_incomplete_member_observation() {
        let mut readings = [
            ExitObservation::Changed,
            ExitObservation::Exited(vec![1_u64]),
            ExitObservation::Exited(vec![1_u64]),
        ]
        .into_iter();
        let mut remaining = 3;
        assert!(
            confirm_exited_group(
                || readings.next().unwrap(),
                |_| {
                    remaining -= 1;
                    remaining > 0
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn exit_confirmation_requires_two_fresh_complete_views_after_change() {
        let mut readings = [
            ExitObservation::Exited(vec![1_u64, 2]),
            ExitObservation::Changed,
            ExitObservation::Exited(vec![1_u64, 3]),
        ]
        .into_iter();
        let mut remaining = 3;
        assert!(
            confirm_exited_group(
                || readings.next().unwrap(),
                |_| {
                    remaining -= 1;
                    remaining > 0
                },
            )
            .is_err()
        );
    }

    #[test]
    fn exit_confirmation_does_not_retry_an_unconfirmed_view() {
        let mut calls = 0;
        assert!(
            confirm_exited_group::<u64>(
                || {
                    calls += 1;
                    ExitObservation::Unconfirmed("unconfirmed fixture view")
                },
                |_| panic!("unconfirmed state must retain the original error"),
            )
            .is_err()
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn exit_confirmation_requires_two_fresh_views_after_an_identified_live_member() {
        let mut readings = [
            ExitObservation::Exited(vec![1_u64, 2]),
            ExitObservation::PendingExit,
            ExitObservation::Exited(vec![1_u64]),
            ExitObservation::Exited(vec![1_u64]),
        ]
        .into_iter();
        let mut transitions = Vec::new();
        let mut remaining = 4;
        let result = confirm_exited_group(
            || readings.next().unwrap(),
            |pending| {
                transitions.push(pending);
                remaining -= 1;
                remaining > 0
            },
        );
        assert!(result.is_ok());
        assert_eq!(transitions, [false, true, false]);
        assert!(readings.next().is_none());
    }

    #[test]
    fn exit_confirmation_never_accepts_a_live_member_when_the_budget_expires() {
        let mut reads = 0;
        let mut remaining = 3;
        let result = confirm_exited_group::<u64>(
            || {
                reads += 1;
                ExitObservation::PendingExit
            },
            |pending| {
                assert!(pending);
                remaining -= 1;
                remaining > 0
            },
        );
        assert!(result.is_err());
        assert_eq!(reads, 3);
    }

    #[test]
    fn exit_confirmation_rejects_empty_and_duplicate_member_views() {
        for members in [vec![], vec![1_u64, 1]] {
            let mut members = Some(members);
            assert!(
                confirm_exited_group(
                    || ExitObservation::Exited(members.take().unwrap()),
                    |_| panic!("invalid complete view must be refused"),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn exit_confirmation_rejects_changed_identity_between_complete_views() {
        let mut readings = [
            ExitObservation::Exited(vec![(1_u64, 10), (2, 20)]),
            ExitObservation::Exited(vec![(1_u64, 10), (2, 21)]),
        ]
        .into_iter();
        let mut continuations = 0;
        assert!(
            confirm_exited_group(
                || readings.next().unwrap(),
                |_| {
                    continuations += 1;
                    true
                },
            )
            .is_err()
        );
        assert_eq!(continuations, 1, "identity changes must not be retried");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_reaped_member_during_snapshot_requires_new_complete_observations() {
        use std::os::unix::process::CommandExt;
        for reap_during in [1, 2] {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            let mut process = Process::spawn(command).unwrap();
            observe_unreaped_exit(&process.child);
            let leader = process.child.id().try_into().unwrap();
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]).process_group(leader);
            let mut worker = command.spawn().unwrap();
            observe_unreaped_exit(&worker);
            let worker_pid: libc::pid_t = worker.id().try_into().unwrap();
            let mut reaped = false;
            let mut changed = false;
            let mut reads = 0;
            let mut remaining = 5;
            let confirmed = confirm_exited_group(
                || {
                    reads += 1;
                    darwin::snapshot_with(leader, |pid, group| {
                        if reads == reap_during && pid == worker_pid && !reaped {
                            worker.wait().unwrap();
                            reaped = true;
                        }
                        let member = darwin::member(pid, group);
                        if pid == worker_pid {
                            changed |= matches!(member, darwin::Member::Changed);
                        }
                        member
                    })
                },
                |_| {
                    remaining -= 1;
                    remaining > 0
                },
            );
            // Reap every owned child even when exit confirmation refuses.
            worker.wait().unwrap();
            process.status = Some(process.child.wait().unwrap());
            assert!(
                reaped && changed,
                "member did not disappear during native snapshot {reap_during}"
            );
            assert!(
                confirmed.is_ok(),
                "fresh complete snapshots must replace the incomplete view"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_exit_confirmation_observes_a_member_finishing_after_the_first_read() {
        let mut command = Command::new("sleep");
        command.arg("30");
        let mut process = Process::spawn(command).unwrap();
        let leader = process.child.id().try_into().unwrap();
        let mut reads = 0;
        let mut remaining = 3;
        let confirmation = confirm_exited_group(
            || {
                reads += 1;
                let snapshot = darwin::snapshot_with(leader, darwin::member);
                if reads == 1 {
                    // SAFETY: this unreaped child anchors the newly owned group.
                    // Finish it only after the first native live-state snapshot.
                    assert_eq!(unsafe { libc::kill(-leader, libc::SIGKILL) }, 0);
                    observe_unreaped_exit(&process.child);
                }
                snapshot
            },
            |_| {
                remaining -= 1;
                remaining > 0
            },
        );
        process.status = Some(process.child.wait().unwrap());
        assert!(
            confirmation.is_ok(),
            "the member exits within budget, then needs two fresh complete views: {confirmation:?}",
        );
        assert_eq!(reads, 3);
    }

    #[cfg(target_os = "macos")]
    fn observe_unreaped_exit(child: &Child) {
        // SAFETY: all-zero fields form a valid writable C output record.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: the test owns this exact unreaped child handle. WNOWAIT
        // observes its exit without releasing the PID used by its process group.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        assert_eq!(result, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_reaps_exited_groups_with_multiple_unwaited_members() {
        use std::os::unix::process::CommandExt;

        for member_count in [1, 2, 5, 33] {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            let mut process = Process::spawn(command).unwrap();
            observe_unreaped_exit(&process.child);
            let mut siblings = Vec::new();
            for _ in 1..member_count {
                let mut command = Command::new("sh");
                command
                    .args(["-c", "exit 0"])
                    .process_group(process.child.id().try_into().unwrap());
                let sibling = command.spawn().unwrap();
                observe_unreaped_exit(&sibling);
                siblings.push(sibling);
            }

            let result = process.poll();
            let reported_reaped = process.status.is_some();
            // Reap every test-owned child before asserting, including when the
            // old controller falsely refuses an already-stopped process group.
            for sibling in &mut siblings {
                sibling.wait().unwrap();
            }
            process.status = Some(process.child.wait().unwrap());
            assert!(
                result.unwrap().unwrap().success(),
                "group size {member_count}"
            );
            assert!(reported_reaped, "group size {member_count} was not reaped");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_exited_leader_cannot_hide_a_live_group_member() {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("sh");
        command.args(["-c", "exit 0"]);
        let mut process = Process::spawn(command).unwrap();
        observe_unreaped_exit(&process.child);
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30"])
            .process_group(process.child.id().try_into().unwrap());
        let mut worker = command.spawn().unwrap();
        let exited = darwin_group_exited(process.child.id().try_into().unwrap());
        process.stop().unwrap();
        worker.wait().unwrap();
        assert!(
            !exited,
            "an exited leader did not prove its live worker stopped"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_unknown_groups_are_not_reported_as_exited() {
        for pid in [0, -1, libc::pid_t::MAX] {
            assert!(!darwin_group_exited(pid), "unconfirmed group {pid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_worker_is_reaped_without_signalling_the_callers_group() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & wait"]);
        let result = run(command, &AtomicBool::new(true));
        assert!(result.result.is_err());
        assert!(result.reaped);
        let mut command = Command::new("sh");
        command.args(["-c", "exit 7"]);
        let result = run(command, &AtomicBool::new(false));
        assert!(result.result.is_err());
        assert!(result.reaped);
    }
}
