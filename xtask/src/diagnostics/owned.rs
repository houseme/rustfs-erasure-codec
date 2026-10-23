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

/// Prove that the owned, unreaped leader's group contains only exited members.
#[cfg(target_os = "macos")]
fn darwin_group_exited(leader: libc::pid_t) -> bool {
    darwin_group_exited_observe(leader, || {})
}

#[cfg(target_os = "macos")]
fn darwin_group_exited_observe(leader: libc::pid_t, mut listed: impl FnMut()) -> bool {
    enum Refusal {
        ReapedMember,
        QueryFailed(std::io::Error),
        Unknown(&'static str),
    }
    fn snapshot(
        leader: libc::pid_t,
        listed: &mut impl FnMut(),
    ) -> std::result::Result<Vec<(libc::pid_t, u64, u64)>, Refusal> {
        use Refusal::Unknown;
        if leader <= 0 {
            return Err(Unknown("invalid owned leader"));
        }
        // libproc returns a PID count (not bytes). Keep one spare slot so a
        // filled/truncated output cannot be mistaken for a complete snapshot.
        // SAFETY: a null buffer with zero length only queries the required size.
        let hint = unsafe { libc::proc_listpgrppids(leader, std::ptr::null_mut(), 0) };
        let capacity = usize::try_from(hint)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(Unknown("invalid group size hint"))?;
        let bytes: libc::c_int = capacity
            .checked_mul(std::mem::size_of::<libc::pid_t>())
            .and_then(|n| n.try_into().ok())
            .ok_or(Unknown("group size overflow"))?;
        let mut members = Vec::<libc::pid_t>::new();
        members
            .try_reserve_exact(capacity)
            .map_err(|_| Unknown("group allocation failed"))?;
        members.resize(capacity, 0);
        // SAFETY: the initialized buffer is writable for exactly `bytes` bytes.
        let count = unsafe { libc::proc_listpgrppids(leader, members.as_mut_ptr().cast(), bytes) };
        let count = usize::try_from(count).map_err(|_| Unknown("group enumeration failed"))?;
        if count == 0 {
            return Err(Unknown("empty group listing"));
        }
        if count >= members.len() {
            return Err(Unknown("truncated group listing"));
        }
        if !members[..count].contains(&leader) {
            return Err(Unknown("owned leader absent from group listing"));
        }
        listed();
        let mut exited = Vec::new();
        exited
            .try_reserve_exact(count)
            .map_err(|_| Unknown("identity allocation failed"))?;
        for &pid in &members[..count] {
            // SAFETY: all-zero fields form a valid writable C output record.
            let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of_val(&info);
            // SAFETY: `info` is writable for `size` bytes. Argument 1 includes
            // zombies; argument 0 cannot establish an exited member's state.
            let copied = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDTBSDINFO,
                    1,
                    (&mut info as *mut libc::proc_bsdinfo).cast(),
                    size.try_into()
                        .map_err(|_| Unknown("invalid member record size"))?,
                )
            };
            if copied <= 0 {
                let error = std::io::Error::last_os_error();
                // A parent may reap an exited worker after enumeration. Never
                // infer its state from ESRCH: restart both complete snapshots.
                return Err(
                    if pid != leader && error.raw_os_error() == Some(libc::ESRCH) {
                        Refusal::ReapedMember
                    } else {
                        Refusal::QueryFailed(error)
                    },
                );
            }
            if usize::try_from(copied).ok() != Some(size) {
                return Err(Unknown("incomplete member state record"));
            }
            if u32::try_from(pid).ok() != Some(info.pbi_pid)
                || u32::try_from(leader).ok() != Some(info.pbi_pgid)
            {
                return Err(Unknown("member PID or group changed"));
            }
            if info.pbi_status != libc::SZOMB {
                return Err(Unknown("member is not confirmed exited"));
            }
            exited.push((pid, info.pbi_start_tvsec, info.pbi_start_tvusec));
        }
        exited.sort_unstable();
        Ok(exited)
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let confirmation = (|| {
            let before = snapshot(leader, &mut listed)?;
            let after = snapshot(leader, &mut listed)?;
            // Already-exited members may be reaped between reads. New members
            // or reused PIDs cannot rely on an earlier zombie record.
            if after
                .iter()
                .all(|member| before.binary_search(member).is_ok())
            {
                Ok(())
            } else {
                Err(Refusal::Unknown(
                    "member identity changed between snapshots",
                ))
            }
        })();
        match confirmation {
            Ok(()) => return true,
            Err(Refusal::ReapedMember) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(refusal) => {
                match refusal {
                    Refusal::ReapedMember => eprintln!(
                        "Darwin group exit unconfirmed: member reaping did not settle within three seconds"
                    ),
                    Refusal::QueryFailed(error) => eprintln!(
                        "Darwin group exit unconfirmed: member state query failed: {error}"
                    ),
                    Refusal::Unknown(reason) => {
                        eprintln!("Darwin group exit unconfirmed: {reason}")
                    }
                }
                return false;
            }
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
    fn darwin_confirms_exit_when_a_member_is_reaped_after_enumeration() {
        use std::os::unix::process::CommandExt;

        for reap_during in [1, 2] {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            let mut process = Process::spawn(command).unwrap();
            observe_unreaped_exit(&process.child);
            let leader = process.child.id().try_into().unwrap();
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]).process_group(leader);
            let sibling = command.spawn().unwrap();
            observe_unreaped_exit(&sibling);
            let mut sibling = Some(sibling);
            // Use real kernel records, and reap the exited member in the exact
            // list-to-info window instead of relying on scheduler timing.
            let mut reads = 0;
            let confirmed = darwin_group_exited_observe(leader, || {
                reads += 1;
                if reads == reap_during
                    && let Some(mut sibling) = sibling.take()
                {
                    sibling.wait().unwrap();
                }
            });
            let reaped_during_read = sibling.is_none();
            if let Some(mut sibling) = sibling {
                sibling.wait().unwrap();
            }
            process.status = Some(process.child.wait().unwrap());
            assert!(reaped_during_read);
            assert!(
                confirmed,
                "an already-reaped worker does not keep its group alive"
            );
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
