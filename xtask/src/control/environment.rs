use super::{
    BASELINE,
    io::{self, Environment},
};
use crate::{Result, runtime, sampling_contract};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
pub(super) fn capture() -> Environment {
    let mut env: Environment = std::env::vars()
        .filter(|(k, _)| {
            !k.starts_with("RSE_")
                && !k.starts_with("RS_")
                && !k.starts_with("CARGO_PROFILE_")
                && ![
                    "CARGO_ENCODED_RUSTFLAGS",
                    "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER",
                ]
                .contains(&k.as_str())
        })
        .collect();
    for (k, v) in sampling_contract::protocol::sampling_environment()
        .as_object()
        .expect("compiled environment")
    {
        env.insert(k.clone(), v.as_str().expect("compiled setting").into());
    }
    env.insert("RUSTUP_TOOLCHAIN".into(), "stable".into());
    env
}
pub(super) fn build(root: &Path, env: &Environment) -> Result<Environment> {
    let mut out = env.clone();
    out.insert(
        "CARGO_TARGET_DIR".into(),
        fs::canonicalize(root)?.join("target").display().to_string(),
    );
    Ok(out)
}
pub(super) fn page_size() -> Result<u64> {
    #[cfg(unix)]
    {
        // SAFETY: sysconf reads a process constant without pointers or mutation.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let page: u64 = page.try_into()?;
        if sampling_contract::valid_page_size(page) {
            return Ok(page);
        }
    }
    Err("invalid or unsupported OS base page size".into())
}
pub(super) fn cpu_set(text: &str) -> Result<BTreeSet<u64>> {
    let mut out = BTreeSet::new();
    for part in text.trim().split(',') {
        let v: Vec<u64> = part
            .split('-')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        if v.is_empty()
            || v.len() > 2
            || v[0] > *v.last().ok_or("empty CPU range")?
            || v.last().copied().unwrap_or_default() > 1_000_000
        {
            return Err("invalid CPU set".into());
        }
        out.extend(v[0]..=*v.last().ok_or("empty CPU range")?);
    }
    Ok(out)
}
#[cfg(target_os = "linux")]
pub(super) fn affinity(pid: i32) -> Result<Vec<u64>> {
    // SAFETY: zero initializes a writable CPU-set record; the kernel receives its exact size.
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    // SAFETY: the initialized writable CPU-set has the exact size passed to the kernel.
    if unsafe { libc::sched_getaffinity(pid, std::mem::size_of_val(&set), &mut set) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((0..libc::CPU_SETSIZE as usize)
        .filter(|cpu| {
            // SAFETY: each index is below CPU_SETSIZE and the mask was initialized by the kernel.
            unsafe { libc::CPU_ISSET(*cpu, &set) }
        })
        .map(|c| c as u64)
        .collect())
}
#[cfg(not(target_os = "linux"))]
pub(super) fn affinity(_pid: i32) -> Result<Vec<u64>> {
    Err("native CPU affinity requires Linux".into())
}
pub(super) fn worker(env: &Environment) -> Result<Environment> {
    #[cfg(target_os = "linux")]
    {
        let mut out = env.clone();
        let cpus = affinity(0)?;
        out.insert(
            "RSE_SAMPLER_CPU".into(),
            cpus.first().ok_or("empty worker affinity")?.to_string(),
        );
        Ok(out)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(env.clone())
    }
}
pub(super) fn source_at(root: &Path, revision: &str) -> Result<Value> {
    let commit = io::output(
        root,
        "git",
        &["rev-parse", &format!("{revision}^{{commit}}")],
    )?;
    if !io::revision_valid(&commit) {
        return Err("require a complete Git source identity".into());
    }
    let tree = io::output(root, "git", &["rev-parse", &format!("{commit}^{{tree}}")])?;
    Ok(
        json!({"commit":commit,"tree":tree,"lock_sha256":crate::artifact::snapshot::digest(&io::git_bytes(root,&format!("{commit}:Cargo.lock"))?)}),
    )
}
pub(super) fn source(root: &Path) -> Result<Value> {
    if !io::output(
        root,
        "git",
        &["status", "--porcelain", "--untracked-files=no"],
    )?
    .is_empty()
    {
        return Err(
            "tracked source changes must be committed before capture or verification".into(),
        );
    }
    source_at(root, "HEAD")
}
pub(super) fn baseline(root: &Path) -> Result<Value> {
    source_at(root, BASELINE)
}
pub(super) fn compiler(root: &Path) -> Result<String> {
    let out = std::process::Command::new("rustc")
        .arg("--version")
        .current_dir(root)
        .env("RUSTUP_TOOLCHAIN", "stable")
        .output()?;
    if !out.status.success() {
        return Err("compiler identity is unavailable".into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().into())
}
pub(super) fn runtime_context(env: &Environment) -> Result<Value> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("ldd")
            .arg("--version")
            .output()?;
        let text = String::from_utf8(output.stdout)?;
        let first = text.lines().next().ok_or("missing libc identity")?;
        let version = first
            .split_whitespace()
            .last()
            .ok_or("missing libc version")?;
        let name = if first.to_ascii_lowercase().contains("musl") {
            "musl"
        } else if first.contains("GLIBC") || first.contains("GNU libc") {
            "glibc"
        } else {
            return Err("unsupported libc identity".into());
        };
        let allocator: Environment = env
            .iter()
            .filter(|(k, _)| runtime::allocator_key(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let result = json!({"libc":{"name":name,"version":version},"page_size":page_size()?,"kernel_memory_policy":crate::memory_policy::capture()?,"allocator_environment":allocator});
        runtime::validate_runtime(&result)?;
        Ok(result)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = env;
        Err("formal runtime provenance requires Linux".into())
    }
}
pub(super) fn isolation(group: &Path) -> Result<Value> {
    let group = fs::canonicalize(group)?;
    let root = Path::new("/sys/fs/cgroup");
    let current = fs::read_to_string("/proc/self/cgroup")?;
    let relative = current
        .split_once("0::")
        .ok_or("require unified cgroup v2")?
        .1
        .trim()
        .trim_start_matches('/');
    if group == root || fs::canonicalize(root.join(relative))? != group {
        return Err("capture must run inside the reserved child CPU partition".into());
    }
    if !["root", "isolated"]
        .contains(&fs::read_to_string(group.join("cpuset.cpus.partition"))?.trim())
    {
        return Err("require exclusive CPU partition".into());
    }
    let cpus = cpu_set(&fs::read_to_string(group.join("cpuset.cpus.effective"))?)?;
    if cpus
        != cpu_set(&fs::read_to_string(
            group.join("cpuset.cpus.exclusive.effective"),
        )?)?
        || cpus.len() < 2
        || cpus.iter().copied().collect::<Vec<_>>() != affinity(0)?
    {
        return Err("capture affinity must match the exclusive reservation".into());
    }
    if fs::read_dir(&group)?.any(|e| e.is_ok_and(|e| e.file_type().is_ok_and(|t| t.is_dir()))) {
        return Err("nested workloads are not allowed in the capture partition".into());
    }
    for raw in fs::read_to_string(group.join("cgroup.procs"))?.split_whitespace() {
        let pid: u32 = raw.parse()?;
        let mut current = pid;
        while current != 0 && current != 1 && current != std::process::id() {
            let stat = match fs::read_to_string(format!("/proc/{current}/stat")) {
                Ok(stat) => stat,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    current = 0;
                    break;
                }
                Err(e) => return Err(e.into()),
            };
            current = stat
                .rsplit_once(") ")
                .ok_or("invalid process identity")?
                .1
                .split_whitespace()
                .nth(1)
                .ok_or("missing parent identity")?
                .parse()?;
        }
        if current == 1 && pid != std::process::id() {
            return Err("another workload is present in the capture partition".into());
        }
    }
    Ok(
        json!({"scope":"exclusive-vm-cpu-partition","reserved_cpu_count":cpus.len(),"cpu_affinity":cpus,"physical_host_exclusivity_proven":false}),
    )
}
pub(super) fn context(root: &Path, env: &Environment, group: &Path) -> Result<Value> {
    let isolation = isolation(group)?;
    let cpus: Vec<u64> = serde_json::from_value(isolation["cpu_affinity"].clone())?;
    let cpu = runtime::cpu_identity(
        &fs::read_to_string("/proc/cpuinfo")?,
        std::env::consts::ARCH,
        &cpus,
    )?;
    Ok(
        json!({"compiler":compiler(root)?,"cpu_model":cpu,"isolation":isolation,"runtime":runtime_context(env)?,"settings":sampling_contract::protocol::sampling_environment()}),
    )
}
