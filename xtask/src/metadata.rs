use crate::{Args, Result};
use serde_json::{Map, Value};
use std::process::Command;

fn capture(program: &str, arguments: &[&str]) -> String {
    match Command::new(program).args(arguments).output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => format!("<unavailable: {}>", output.status),
        Err(error) => format!("<unavailable: {error}>"),
    }
}

pub(crate) fn machine_info() -> Value {
    let mut values = Map::new();
    for (key, arguments) in [
        ("hostname", &["-n"][..]),
        ("arch", &["-m"][..]),
        ("platform", &["-s", "-r", "-m"][..]),
        ("uname_a", &["-a"][..]),
    ] {
        values.insert(key.into(), capture("uname", arguments).into());
    }
    let lscpu = capture("lscpu", &[]);
    values.insert(
        "lscpu".into(),
        if lscpu.starts_with("<unavailable:") {
            String::new()
        } else {
            lscpu
        }
        .into(),
    );
    Value::Object(values)
}

pub(crate) fn write_machine(args: &Args) -> Result<()> {
    let mut info = machine_info();
    for (key, option) in [
        ("date_utc", "--date"),
        ("machine_slug", "--machine-slug"),
        ("git_revision", "--git-revision"),
        ("feature_set", "--feature-set"),
    ] {
        info[key] = args.required(option)?.into();
    }
    info["backends"] = args
        .required("--backends")?
        .split_whitespace()
        .collect::<Vec<_>>()
        .into();
    crate::artifact::write_json(std::path::Path::new(args.required("--output")?), &info)
}

pub(crate) fn write(args: &Args) -> Result<()> {
    let mut values = Map::new();
    for key in [
        "baseline_ref",
        "candidate_ref",
        "backend_override",
        "features",
        "sample_size",
        "warmup_time",
        "measurement_time",
        "cooldown_seconds",
        "drift_threshold",
        "cpuset",
    ] {
        values.insert(
            key.to_owned(),
            Value::String(
                args.required(&format!("--{}", key.replace('_', "-")))?
                    .to_owned(),
            ),
        );
    }
    for (key, program, arguments) in [
        ("hostname", "uname", &["-n"][..]),
        ("machine", "uname", &["-m"][..]),
        ("platform", "uname", &["-s", "-r", "-m"][..]),
        ("uname", "uname", &["-a"][..]),
        ("lscpu", "lscpu", &[][..]),
        ("rustc", "rustc", &["--version", "--verbose"][..]),
        ("cargo", "cargo", &["--version"][..]),
    ] {
        values.insert(key.to_owned(), Value::String(capture(program, arguments)));
    }
    std::fs::write(
        args.required("--output")?,
        serde_json::to_string_pretty(&values)? + "\n",
    )?;
    Ok(())
}
