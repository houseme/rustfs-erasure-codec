use crate::{
    Result,
    artifact::{python_json, snapshot},
};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
pub(super) type Environment = BTreeMap<String, String>;
pub(super) struct Context<'a> {
    pub cancelled: &'a AtomicBool,
    pub unreaped: Cell<bool>,
}
impl<'a> Context<'a> {
    pub fn new(cancelled: &'a AtomicBool) -> Self {
        Self {
            cancelled,
            unreaped: Cell::new(false),
        }
    }
    pub fn now_ns(&self) -> Result<u64> {
        monotonic_ns()
    }
    pub fn run(&self, command: Command) -> Result<()> {
        let outcome = crate::diagnostics::owned::run(command, self.cancelled);
        if !outcome.reaped {
            self.unreaped.set(true);
        }
        outcome.result
    }
    pub fn capture(&self, mut command: Command) -> Result<String> {
        let temporary = Temporary::new(self, "command")?;
        let path = temporary.path.join("output.log");
        let log = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        command.stdout(log.try_clone()?).stderr(log);
        let outcome = self.run(command);
        let text = fs::read_to_string(&path)?;
        if let Err(error) = outcome {
            return Err(format!("{error}\n{text}").into());
        }
        Ok(text)
    }
    pub fn logged(&self, mut command: Command, path: &Path, new: bool) -> Result<()> {
        let log = OpenOptions::new()
            .write(true)
            .create(!new)
            .create_new(new)
            .append(!new)
            .open(path)?;
        command.stdout(log.try_clone()?).stderr(log);
        self.run(command)
    }
}
pub(super) fn command(
    root: &Path,
    env: &Environment,
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(root)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null());
    command
}
pub(super) fn output(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    if !out.status.success() {
        return Err(format!("{program} failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().into())
}
pub(super) fn git_bytes(root: &Path, object: &str) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .args(["show", object])
        .current_dir(root)
        .output()?;
    if !out.status.success() {
        return Err("git source member is unavailable".into());
    }
    Ok(out.stdout)
}
pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }
    let name = path.file_name().ok_or("invalid new output path")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(absolute(parent)?.join(name))
}
pub(super) fn present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
pub(super) fn bytes(path: &Path, sha: Option<&str>) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err("require bounded regular original evidence".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(snapshot::MEMBER_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > snapshot::MEMBER_LIMIT {
        return Err("require bounded regular original evidence".into());
    }
    if let Some(sha) = sha
        && (!digest_valid(sha) || snapshot::digest(&bytes) != sha)
    {
        return Err("original member digest mismatch".into());
    }
    Ok(bytes)
}
pub(super) fn text(path: &Path, sha: Option<&str>) -> Result<String> {
    Ok(String::from_utf8(bytes(path, sha)?)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
pub(super) fn json(path: &Path, sha: Option<&str>) -> Result<Value> {
    python_json::parse(&text(path, sha)?)
}
pub(super) fn bound_json(path: &Path) -> Result<(Value, String)> {
    let bytes = bytes(path, None)?;
    let sha = snapshot::digest(&bytes);
    Ok((
        python_json::parse(
            &String::from_utf8(bytes)?
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
        )?,
        sha,
    ))
}
pub(super) fn digest(path: &Path) -> Result<String> {
    Ok(snapshot::digest(&fs::read(path)?))
}
pub(super) fn digest_valid(sha: &str) -> bool {
    sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn revision_valid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn write_json<T: serde::Serialize>(path: &Path, value: &T, new: bool) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create(!new)
        .truncate(!new)
        .create_new(new)
        .open(path)?;
    serde_json::to_writer_pretty(&mut f, value)?;
    f.write_all(b"\n")?;
    Ok(())
}
pub(super) fn append_json(file: &mut File, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
pub(super) fn directory(path: &Path) -> Result<()> {
    if present(path) {
        return Err("use a new private output directory; preserve existing evidence".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub(super) fn monotonic_ns() -> Result<u64> {
    #[cfg(unix)]
    {
        let mut point = std::mem::MaybeUninit::<libc::timespec>::uninit();
        // SAFETY: clock_gettime initializes the complete valid writable output on success.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, point.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful clock_gettime initialized both fields above.
        let point = unsafe { point.assume_init() };
        let seconds: u64 = point.tv_sec.try_into()?;
        let nanos: u64 = point.tv_nsec.try_into()?;
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|n| n.checked_add(nanos))
            .ok_or_else(|| "monotonic clock overflow".into())
    }
    #[cfg(not(unix))]
    {
        static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        Ok(ORIGIN
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_nanos()
            .try_into()?)
    }
}
pub(super) fn utc_ns() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .try_into()?)
}
pub(super) fn member(root: &Path, relative: &str) -> Result<PathBuf> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err("require a regular original directory".into());
    }
    let mut out = root.to_path_buf();
    if relative.is_empty() || relative.contains('\\') {
        return Err("invalid evidence member path".into());
    }
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(part) = component else {
            return Err("invalid evidence member path".into());
        };
        out.push(part);
        if fs::symlink_metadata(&out)?.file_type().is_symlink() {
            return Err("symlinked evidence member".into());
        }
    }
    Ok(out)
}
pub(super) fn failure(output: &Path, error: &dyn std::fmt::Display) -> Result<()> {
    write_json(
        &output.join("diagnostic-status.json"),
        &json!({"status":"diagnostic_failed","release_approval":false,"candidate_attribution":null,"original_records_preserved":true}),
        false,
    )?;
    write_json(
        &output.join("failure.json"),
        &json!({"error":error.to_string(),"release_approval":false,"original_records_preserved":true}),
        true,
    )
}
pub(super) struct Temporary<'a, 'b> {
    pub path: PathBuf,
    context: &'a Context<'b>,
}
impl<'a, 'b> Temporary<'a, 'b> {
    pub fn new(context: &'a Context<'b>, name: &str) -> Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "rse-{name}-{}-{}-{}",
            std::process::id(),
            utc_ns()?,
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self { path, context })
    }
}
impl Drop for Temporary<'_, '_> {
    fn drop(&mut self) {
        if !self.context.unreaped.get() {
            if let Err(error) = fs::remove_dir_all(&self.path) {
                eprintln!("private source cleanup failed: {error}");
            }
        } else {
            eprintln!(
                "owned child was not reaped; preserve {}",
                self.path.display()
            );
        }
    }
}
