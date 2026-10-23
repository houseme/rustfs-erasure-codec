//! Darwin-only observation of the group anchored by an owned, unreaped leader.
use super::{ExitObservation, confirm_exited_group};
use std::time::{Duration, Instant};

type Identity = (libc::pid_t, u64, u64);
pub(super) enum Member {
    Exited(Identity),
    Changed,
    Unconfirmed(&'static str),
}

pub(super) fn member(pid: libc::pid_t, leader: libc::pid_t) -> Member {
    // SAFETY: all-zero fields form a valid writable C output record.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info);
    let Ok(bytes) = size.try_into() else {
        return Member::Unconfirmed("member information layout is invalid");
    };
    // SAFETY: info is writable for exactly bytes; argument 1 includes zombies.
    let copied = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            1,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            bytes,
        )
    };
    if copied <= 0 {
        let error = std::io::Error::last_os_error();
        return if pid != leader && error.raw_os_error() == Some(libc::ESRCH) {
            Member::Changed
        } else {
            eprintln!("owned Darwin member state query failed: {error}");
            Member::Unconfirmed("member information could not be read")
        };
    }
    if usize::try_from(copied).ok() != Some(size) {
        return Member::Unconfirmed("incomplete member state record");
    }
    if u32::try_from(pid).ok() != Some(info.pbi_pid)
        || u32::try_from(leader).ok() != Some(info.pbi_pgid)
    {
        // A different identity/group is not a disappearance witness.
        return Member::Unconfirmed("member identity differs from the owned group");
    }
    if info.pbi_status != libc::SZOMB {
        return Member::Unconfirmed("a member is not confirmed exited");
    }
    Member::Exited((pid, info.pbi_start_tvsec, info.pbi_start_tvusec))
}

pub(super) fn snapshot_with(
    leader: libc::pid_t,
    mut inspect: impl FnMut(libc::pid_t, libc::pid_t) -> Member,
) -> ExitObservation<Identity> {
    if leader <= 0 {
        return ExitObservation::Unconfirmed("invalid owned group identifier");
    }
    // libproc returns a PID count. One spare slot detects a filled/truncated view.
    // SAFETY: a null buffer with zero length only queries the required size.
    let hint = unsafe { libc::proc_listpgrppids(leader, std::ptr::null_mut(), 0) };
    let Some(capacity) = usize::try_from(hint).ok().and_then(|n| n.checked_add(1)) else {
        return ExitObservation::Unconfirmed("group size query failed");
    };
    let Some(bytes) = capacity
        .checked_mul(std::mem::size_of::<libc::pid_t>())
        .and_then(|n| libc::c_int::try_from(n).ok())
    else {
        return ExitObservation::Unconfirmed("group observation exceeds its byte budget");
    };
    let mut members = Vec::<libc::pid_t>::new();
    if members.try_reserve_exact(capacity).is_err() {
        return ExitObservation::Unconfirmed("group list allocation failed");
    }
    members.resize(capacity, 0);
    // SAFETY: the initialized buffer is writable for exactly bytes bytes.
    let count = unsafe { libc::proc_listpgrppids(leader, members.as_mut_ptr().cast(), bytes) };
    let Ok(count) = usize::try_from(count) else {
        return ExitObservation::Unconfirmed("group membership query failed");
    };
    if count >= members.len() {
        return ExitObservation::Unconfirmed("truncated group listing");
    }
    if count == 0 || !members[..count].contains(&leader) {
        return ExitObservation::Unconfirmed("owned leader missing from group observation");
    }
    let mut exited = Vec::new();
    if exited.try_reserve_exact(count).is_err() {
        return ExitObservation::Unconfirmed("member information allocation failed");
    }
    for &pid in &members[..count] {
        match inspect(pid, leader) {
            Member::Exited(identity) => exited.push(identity),
            Member::Changed => return ExitObservation::Changed,
            Member::Unconfirmed(reason) => return ExitObservation::Unconfirmed(reason),
        }
    }
    ExitObservation::Exited(exited)
}

pub(super) fn group_exited(leader: libc::pid_t) -> bool {
    // Membership enumeration and per-PID inspection are not atomic. Stabilize
    // only known-changing views while the owned leader still anchors its group.
    // The usual complete pair incurs no sleep. A deadline never grants success.
    let deadline = Instant::now() + Duration::from_secs(3);
    let confirmation = confirm_exited_group(
        || snapshot_with(leader, member),
        |changed| {
            if Instant::now() >= deadline {
                return false;
            }
            if changed {
                std::thread::sleep(Duration::from_millis(10));
            }
            true
        },
    );
    if let Err(reason) = &confirmation {
        // Keep the original signal error; add only the unavailable proof stage.
        eprintln!("owned Darwin group exit unconfirmed: {reason}");
    }
    confirmation.is_ok()
}
