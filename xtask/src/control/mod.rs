//! Native capture controllers: shared resource owners and typed validation models.
mod audits;
mod baseline;
mod capture;
mod environment;
mod frozen;
mod inventory;
mod io;
mod layout;
mod overlay;
mod placement;
mod release;
mod sampling;
#[path = "../../source_stamp.rs"]
mod source_stamp;
mod utility;
mod vm;
use crate::Result;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct CommandSpec {
    pub name: &'static str,
    values: &'static [&'static str],
    flags: &'static [&'static str],
    modes: &'static [&'static str],
}
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "release-sampling",
        values: &["--baseline-ref"],
        flags: &["--check", "--check-workers"],
        modes: &[],
    },
    CommandSpec {
        name: "release-evidence",
        values: &[
            "--manifest",
            "--repository",
            "--env-file",
            "--report",
            "--baseline",
            "--candidate",
            "--output",
            "--isolation-cgroup",
        ],
        flags: &[],
        modes: &[
            "capture",
            "qualify-baseline",
            "diagnose-complete",
            "verify",
            "materialize",
            "verify-diagnostic",
        ],
    },
    CommandSpec {
        name: "diagnose-verify-layout",
        values: &[
            "--verify",
            "--repository",
            "--baseline",
            "--candidate",
            "--output",
            "--isolation-cgroup",
            "--iterations",
        ],
        flags: &["--check"],
        modes: &[],
    },
    CommandSpec {
        name: "verify-scratch-audit",
        values: &["--replay", "--baseline-ref", "--output"],
        flags: &["--check"],
        modes: &[],
    },
    CommandSpec {
        name: "stream-core-audit",
        values: &["--replay", "--baseline-ref", "--output"],
        flags: &["--check"],
        modes: &[],
    },
    CommandSpec {
        name: "dynamic-buffer-audit",
        values: &["--replay", "--baseline-ref", "--output"],
        flags: &["--check"],
        modes: &[],
    },
    CommandSpec {
        name: "diagnose-fixture-layout",
        values: &[
            "--tool",
            "--baseline",
            "--candidate",
            "--output",
            "--isolation-cgroup",
            "--iterations",
            "--replay-dynamic",
            "--baseline-ref",
        ],
        flags: &["--stream-views", "--check-dynamic", "--check-stream-views"],
        modes: &[],
    },
    CommandSpec {
        name: "diagnose-cpu-placement",
        values: &[
            "--tool",
            "--repository",
            "--output",
            "--isolation-cgroup",
            "--iterations",
        ],
        flags: &["--check"],
        modes: &[],
    },
    CommandSpec {
        name: "diagnose-shard-slot",
        values: &[
            "--tool",
            "--repository",
            "--output",
            "--isolation-cgroup",
            "--iterations",
        ],
        flags: &["--check"],
        modes: &[],
    },
    CommandSpec {
        name: "diagnose-adjacent-baseline",
        values: &["--prior", "--output", "--baseline", "--isolation-cgroup"],
        flags: &[],
        modes: &["capture", "replay"],
    },
    CommandSpec {
        name: "analyze-baseline-variability",
        values: &[
            "--capture",
            "--adjacent-capture",
            "--prior",
            "--frozen-tool",
            "--output",
        ],
        flags: &[],
        modes: &[],
    },
    CommandSpec {
        name: "diagnose-release-vm",
        values: &[
            "--baseline",
            "--output",
            "--isolation-cgroup",
            "--frequency-from-adjacent",
            "--prior",
            "--frozen-tool",
        ],
        flags: &[
            "--replay-frequency",
            "--native-service-clock",
            "--probe-only",
        ],
        modes: &[],
    },
    CommandSpec {
        name: "runtime-context",
        values: &[],
        flags: &[],
        modes: &[],
    },
    CommandSpec {
        name: "lock-version",
        values: &["--lock", "--package"],
        flags: &[],
        modes: &[],
    },
    CommandSpec {
        name: "check-verification-resources",
        values: &["--input"],
        flags: &[],
        modes: &[],
    },
    CommandSpec {
        name: "write-ci-status",
        values: &["--kind", "--output"],
        flags: &[],
        modes: &[],
    },
];
pub(super) fn known(name: &str) -> bool {
    COMMANDS.iter().any(|spec| spec.name == name)
}
static CANCELLED: AtomicBool = AtomicBool::new(false);
pub(crate) const BASELINE: &str = "9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6";
pub(crate) fn tool() -> &'static Path {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::env::var_os("RSE_NATIVE_TOOL_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("workspace parent")
                    .to_owned()
            })
    })
    .as_path()
}
fn model(kind: &str, request: Value) -> Result<Value> {
    crate::bridge::evaluate(Value::Object(serde_json::Map::from_iter([
        ("kind".into(), kind.into()),
        ("request".into(), request),
    ])))
}
fn sampling_model(operation: &str, mut values: Value) -> Result<Value> {
    values
        .as_object_mut()
        .ok_or("invalid sampling request")?
        .insert("operation".into(), operation.into());
    model("sampling", values)
}
fn controller_digest(name: &str) -> Result<String> {
    crate::fingerprint::file(&tool().join("xtask/src/control").join(format!("{name}.rs")))
}
#[derive(Default)]
struct Options {
    values: BTreeMap<String, String>,
    flags: BTreeSet<String>,
    mode: Option<String>,
}
impl Options {
    fn command(name: &str, args: Vec<String>) -> Result<Self> {
        let spec = COMMANDS
            .iter()
            .find(|spec| spec.name == name)
            .ok_or("unknown controller syntax")?;
        Self::parse(args.into_iter(), spec.values, spec.flags, spec.modes)
    }
    fn only(&self, allowed: &[&str]) -> Result<()> {
        if let Some(key) = self
            .values
            .keys()
            .chain(&self.flags)
            .find(|key| !allowed.contains(&key.as_str()))
        {
            return Err(format!("{key} does not apply to this mode").into());
        }
        Ok(())
    }

    fn parse(
        args: impl Iterator<Item = String>,
        values: &[&str],
        flags: &[&str],
        modes: &[&str],
    ) -> Result<Self> {
        let mut out = Self::default();
        let mut args = args;
        while let Some(arg) = args.next() {
            if modes.contains(&arg.as_str()) && out.mode.is_none() {
                out.mode = Some(arg);
                continue;
            }
            let (key, inline) = arg
                .split_once('=')
                .map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
            if flags.contains(&key) {
                if inline.is_some() || !out.flags.insert(key.into()) {
                    return Err(format!("invalid repeated flag: {key}").into());
                }
            } else if values.contains(&key) {
                let value = inline
                    .map(str::to_owned)
                    .or_else(|| args.next().filter(|v| !v.starts_with("--")))
                    .ok_or_else(|| format!("{key} requires a value"))?;
                if out.values.insert(key.into(), value).is_some() {
                    return Err(format!("duplicate option: {key}").into());
                }
            } else {
                return Err(format!("unknown option or mode: {arg}").into());
            }
        }
        Ok(out)
    }
    fn has(&self, key: &str) -> bool {
        self.flags.contains(key)
    }
    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    fn required(&self, key: &str) -> Result<&str> {
        self.get(key)
            .ok_or_else(|| format!("{key} is required").into())
    }
    fn path(&self, key: &str) -> Result<PathBuf> {
        io::absolute(Path::new(self.required(key)?))
    }
    fn number(&self, key: &str, default: u64, maximum: u64) -> Result<u64> {
        let value = self
            .get(key)
            .map(str::parse)
            .transpose()?
            .unwrap_or(default);
        if value == 0 || value > maximum {
            return Err(format!("invalid bounded {key}").into());
        }
        Ok(value)
    }
}
pub(super) fn run(command: &str, args: Vec<String>) -> Result<bool> {
    if args == ["--help"] || args == ["-h"] {
        let spec = COMMANDS
            .iter()
            .find(|spec| spec.name == command)
            .ok_or("unknown controller")?;
        println!("cargo xtask {command} {}", spec.modes.join("|"));
        for flag in spec.flags {
            println!("  {flag}");
        }
        for option in spec.values {
            println!("  {option} VALUE");
        }
        return Ok(true);
    }
    if command != "release-evidence" {
        source_current()?;
    }
    CANCELLED.store(false, Ordering::Relaxed);
    ctrlc::set_handler(|| CANCELLED.store(true, Ordering::Relaxed))?;
    let context = io::Context::new(&CANCELLED);
    match command {
        "release-sampling" => sampling::cli(&context, args)?,
        "release-evidence" => release::cli(&context, args)?,
        "diagnose-verify-layout" => layout::cli(&context, args)?,
        "verify-scratch-audit" => audits::cli(&context, audits::Kind::Verify, args)?,
        "stream-core-audit" => audits::cli(&context, audits::Kind::Stream, args)?,
        "dynamic-buffer-audit" => audits::cli(&context, audits::Kind::Dynamic, args)?,
        "diagnose-fixture-layout" => inventory::cli(&context, args)?,
        "diagnose-cpu-placement" => placement::cli(&context, true, args)?,
        "diagnose-shard-slot" => placement::cli(&context, false, args)?,
        "diagnose-adjacent-baseline" => baseline::adjacent_cli(&context, args)?,
        "analyze-baseline-variability" => baseline::cli(&context, args)?,
        "diagnose-release-vm" => vm::cli(&context, args)?,
        "runtime-context" | "lock-version" | "check-verification-resources" | "write-ci-status" => {
            utility::cli(&context, command, args)?
        }
        _ => return Err("unknown native controller".into()),
    }
    Ok(true)
}

pub(crate) fn source_current() -> Result<()> {
    if source_stamp::digest(tool())? != env!("RSE_NATIVE_SOURCE_SHA256") {
        return Err(
            "native controller sources differ from this executable; rebuild with cargo xtask"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Read the existing Linux reservation; never change cgroups or affinity.
pub(crate) fn plan_resource_context(root: &Path, group: &Path) -> Result<Value> {
    if !cfg!(target_os = "linux") {
        return Err("plan resource preflight requires Linux".into());
    }
    environment::context(root, &environment::capture(), group)
}

/// Preserve the existing context format while requiring an exact native build.
pub(crate) fn plan_collection_context(
    root: &Path,
    group: &Path,
    frozen_compiler: &str,
) -> Result<Value> {
    let observed = plan_resource_context(root, group)?;
    let compiler = environment::compiler_identity(root, true)?;
    crate::plan_bench::collector::require_compiler(
        frozen_compiler,
        &compiler,
        env!("RSE_NATIVE_TARGET"),
    )?;
    Ok(observed)
}
