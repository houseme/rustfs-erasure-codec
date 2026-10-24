//! Explicit trusted source is built in a private checkout; only its Rust models execute.
use super::{
    environment as env,
    io::{Context, Temporary},
    sampling, *,
};
use crate::diagnostics::owned::Process;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
const LIMIT: usize = 32 * 1024 * 1024 + 64 * 1024;
struct Session<'a, 'b> {
    process: Process,
    input: ChildStdin,
    output: ChildStdout,
    pending: Vec<u8>,
    context: &'a Context<'b>,
}
impl<'a, 'b> Session<'a, 'b> {
    fn spawn(context: &'a Context<'b>, mut command: Command) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut process = Process::spawn(command)?;
        let input = process
            .child
            .stdin
            .take()
            .ok_or("missing frozen model input")?;
        let output = process
            .child
            .stdout
            .take()
            .ok_or("missing frozen model output")?;
        Ok(Self {
            process,
            input,
            output,
            pending: Vec::new(),
            context,
        })
    }
    #[cfg(unix)]
    fn request(&mut self, value: Value) -> Result<Value> {
        use std::os::fd::AsRawFd;
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        if bytes.len() > LIMIT {
            return Err("frozen request exceeds its bounded frame".into());
        }
        let deadline = Instant::now() + Duration::from_secs(120);
        let input = self.input.as_raw_fd();
        let output = self.output.as_raw_fd();
        for fd in [input, output] {
            // SAFETY: these descriptors are owned exclusively by this session. Read their flags.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            // SAFETY: enable nonblocking IO on the exact owned pipe, preserving all other flags.
            if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let mut sent = 0;
        while sent < bytes.len() {
            self.wait(input, libc::POLLOUT, deadline)?;
            match self.input.write(&bytes[sent..]) {
                Ok(0) => return Err("frozen helper closed its input".into()),
                Ok(n) => sent += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        loop {
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                let remainder = self.pending.split_off(end + 1);
                let bytes = std::mem::replace(&mut self.pending, remainder);
                let crate::artifact::StrictJson(mut reply) = serde_json::from_slice(&bytes)?;
                if reply["ok"].as_bool() != Some(true) {
                    return Err(reply["error"]
                        .as_str()
                        .unwrap_or("invalid frozen Rust response")
                        .to_owned()
                        .into());
                }
                return reply
                    .as_object_mut()
                    .ok_or("invalid frozen Rust result")?
                    .remove("value")
                    .ok_or_else(|| "missing frozen Rust result".into());
            }
            self.wait(output, libc::POLLIN, deadline)?;
            let mut buffer = [0u8; 65536];
            match self.output.read(&mut buffer) {
                Ok(0) => return Err("frozen Rust worker ended before its reply".into()),
                Ok(n) => self.pending.extend_from_slice(&buffer[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
            if self.pending.len() > LIMIT {
                return Err("frozen response exceeds its bounded frame".into());
            }
        }
    }
    #[cfg(unix)]
    fn wait(&self, fd: libc::c_int, event: libc::c_short, deadline: Instant) -> Result<()> {
        loop {
            if self.context.cancelled.load(Ordering::Relaxed) {
                return Err("frozen replay cancelled".into());
            }
            if Instant::now() > deadline {
                return Err("frozen replay exceeded its bounded runtime".into());
            }
            let mut descriptor = libc::pollfd {
                fd,
                events: event,
                revents: 0,
            };
            // SAFETY: poll receives one valid writable owned descriptor record with a bounded timeout.
            let ready = unsafe { libc::poll(&mut descriptor, 1, 100) };
            if ready > 0 {
                return Ok(());
            }
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::Interrupted {
                    return Err(error.into());
                }
            }
        }
    }
    #[cfg(not(unix))]
    fn request(&mut self, _value: Value) -> Result<Value> {
        Err("bounded frozen replay requires a Unix host".into())
    }
}
impl Drop for Session<'_, '_> {
    fn drop(&mut self) {
        if let Err(error) = self.process.stop() {
            self.context.unreaped.set(true);
            eprintln!("frozen Rust worker cleanup failed: {error}");
        }
    }
}
pub(super) struct Models<'a, 'b> {
    session: Option<Session<'a, 'b>>,
    temporary: Option<Temporary<'a, 'b>>,
    assets: PathBuf,
}
impl<'a, 'b> Models<'a, 'b> {
    pub fn current() -> Self {
        Self {
            session: None,
            temporary: None,
            assets: tool().to_owned(),
        }
    }
    pub fn frozen(context: &'a Context<'b>, repository: &Path, source: &Value) -> Result<Self> {
        let temporary = Temporary::new(context, "frozen-models")?;
        let assets = temporary.path.join("source");
        sampling::checkout(
            context,
            repository,
            &assets,
            source["commit"]
                .as_str()
                .ok_or("missing trusted source revision")?,
        )?;
        if env::source(&assets)? != *source {
            return Err("private frozen source differs from the trusted checkout".into());
        }
        let mut command = io::command(
            &assets,
            &env::capture(),
            "cargo",
            [
                "build",
                "--quiet",
                "--release",
                "--locked",
                "--offline",
                "--package",
                "xtask",
                "--manifest-path",
            ],
        );
        command
            .arg(assets.join("xtask/Cargo.toml"))
            .arg("--target-dir")
            .arg(assets.join("target"));
        context.capture(command)?;
        let executable = assets
            .join("target/release")
            .join(format!("xtask{}", std::env::consts::EXE_SUFFIX));
        let command = io::command(
            &assets,
            &env::capture(),
            executable
                .to_str()
                .ok_or("invalid frozen executable path")?,
            ["native-helper", "--serve"],
        );
        let session = Session::spawn(context, command)?;
        Ok(Self {
            session: Some(session),
            temporary: Some(temporary),
            assets,
        })
    }
    pub fn call(&mut self, value: Value) -> Result<Value> {
        match &mut self.session {
            Some(session) => session.request(value),
            None => crate::bridge::evaluate(value),
        }
    }
    pub fn model(&mut self, kind: &str, request: Value) -> Result<Value> {
        self.call(Value::Object(serde_json::Map::from_iter([
            ("kind".into(), kind.into()),
            ("request".into(), request),
        ])))
    }
    pub fn assets(&self) -> &Path {
        &self.assets
    }
    pub fn overlay(&mut self) -> Result<String> {
        let root = self.assets.clone();
        let files = self.call(json!({"kind":"sampler-render","root":root,"tool":root}))?;
        crate::fingerprint::overlay(&serde_json::from_value(files)?, &root)
    }
    pub fn implementation(&self) -> Result<String> {
        crate::fingerprint::sampling(&self.assets)
    }
}
impl Drop for Models<'_, '_> {
    fn drop(&mut self) {
        drop(self.session.take());
        drop(self.temporary.take());
    }
}

#[cfg(test)]
mod tests;
