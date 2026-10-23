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

/// Retain the child identity until its newly-created process group is stopped.
struct Process {
    child: Child,
    status: Option<ExitStatus>,
}
impl Process {
    fn spawn(mut command: Command) -> Result<Self> {
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
    fn poll(&mut self) -> Result<Option<ExitStatus>> {
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
                // Darwin excludes zombies from group signalling and can return
                // EPERM for a group containing only our exited leader. Prove
                // that exact case; a real permission failure stays an error.
                let mut members = [0 as libc::pid_t; 2];
                // SAFETY: the output is writable for the declared byte length.
                let count = unsafe {
                    libc::proc_listpgrppids(
                        pid,
                        members.as_mut_ptr().cast(),
                        std::mem::size_of_val(&members) as libc::c_int,
                    )
                };
                if count == 1 && members[0] == pid {
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
    fn stop(&mut self) -> Result<()> {
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
