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

/// A live child handle and, on Unix, its newly-created process group.
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
            self.status = self.child.try_wait()?;
        }
        Ok(self.status)
    }
    fn signal(&mut self, force: bool) -> Result<()> {
        if self.poll()?.is_some() {
            return Ok(());
        }
        #[cfg(unix)]
        {
            let pid: libc::pid_t = self
                .child
                .id()
                .try_into()
                .map_err(|_| "child PID is out of range")?;
            // SAFETY: pid is positive and belongs to this still-live Child. Its
            // process group was created at spawn; no arbitrary PID is accepted.
            let group = unsafe { libc::getpgid(pid) };
            if group != pid {
                return Err("refuse to signal a process group not owned by this command".into());
            }
            // SAFETY: the live unreaped child anchors this new, owned group;
            // negating its positive group ID cannot signal the caller's group.
            if unsafe { libc::kill(-pid, if force { libc::SIGKILL } else { libc::SIGTERM }) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
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
        let root = std::env::temp_dir().join(format!(
            "rse-owned-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
