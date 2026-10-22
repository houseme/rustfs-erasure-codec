//! Repository-only tools. No dependency from the codec to this package.
mod abba;
mod allocator_ledger;
mod artifact;
mod audit;
mod benchmark_contract;
mod bridge;
mod criterion;
mod diagnostics;
mod manifest;
mod memory_policy;
mod metadata;
mod perf;
mod regression;
mod runtime;
mod sampling_contract;
mod summary;
mod workspace;

use std::{collections::BTreeMap, path::PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Args(BTreeMap<String, String>, BTreeMap<String, Vec<String>>);

impl Args {
    fn parse(arguments: impl Iterator<Item = String>, allowed: &[&str]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut repeated: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut arguments = arguments;
        while let Some(argument) = arguments.next() {
            let (key, inline) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(k, v)| (k, Some(v)));
            if !allowed.contains(&key) {
                return Err(format!("unknown option: {key}").into());
            }
            let value = if [
                "--write-header",
                "--summary",
                "--allow-missing-cases",
                "--allow-missing-provenance",
                "--strict-provenance",
                "--serve",
            ]
            .contains(&key)
            {
                if inline.is_some() {
                    return Err(format!("{key} takes no value").into());
                }
                String::new()
            } else {
                match inline {
                    Some(value) => value.to_owned(),
                    None => arguments
                        .next()
                        .filter(|value| !value.starts_with("--"))
                        .ok_or_else(|| format!("{key} requires a value"))?,
                }
            };
            if [
                "--threshold",
                "--require-case",
                "--require-scenario",
                "--scenario-max-regression",
                "--min-speedup",
            ]
            .contains(&key)
            {
                repeated.entry(key.to_owned()).or_default().push(value);
            } else if values.insert(key.to_owned(), value).is_some() {
                return Err(format!("duplicate option: {key}").into());
            }
        }
        Ok(Self(values, repeated))
    }

    fn required(&self, key: &str) -> Result<&str> {
        self.0
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("{key} is required").into())
    }

    fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.0.get(key).map_or(default, String::as_str)
    }

    fn root(&self) -> PathBuf {
        PathBuf::from(self.get("--root", "."))
    }

    fn many(&self, key: &str) -> &[String] {
        self.1.get(key).map_or(&[], Vec::as_slice)
    }
}

const COMMANDS: &[(&str, &[&str])] = &[
    ("validate-diagnostic-ledger", &["--input"]),
    ("validate-stream-views", &["--input"]),
    ("summarize-smoke", &["--input"]),
    ("sampling-contract", &["--input"]),
    ("memory-policy", &["--input", "--base-page-size"]),
    ("check-sampler-telemetry", &["--root"]),
    ("native-helper", &["--input", "--serve"]),
    (
        "stream-allocator-audit",
        &["--trace", "--samples", "--output"],
    ),
    (
        "check-benchmark",
        &[
            "--baseline",
            "--current",
            "--threshold",
            "--metric",
            "--require-case",
            "--allow-missing-cases",
            "--strict-provenance",
            "--allow-missing-provenance",
        ],
    ),
    (
        "check-hotspot",
        &[
            "--baseline",
            "--current",
            "--require-scenario",
            "--metric",
            "--max-regression",
            "--scenario-max-regression",
            "--min-speedup",
        ],
    ),
    ("check-release", &["--root", "--tag"]),
    (
        "extract-criterion",
        &[
            "--criterion-root",
            "--stage",
            "--commit",
            "--backend-override",
            "--csv",
            "--jsonl",
            "--write-header",
            "--updated-after-epoch",
        ],
    ),
    (
        "analyze-abba",
        &[
            "--csv",
            "--summary-json",
            "--summary-md",
            "--drift-threshold",
        ],
    ),
    ("audit-json", &["--root"]),
    ("audit-package", &["--root"]),
    ("audit-workspace", &["--root"]),
    (
        "summarize-simd",
        &[
            "--root",
            "--machine-json",
            "--machine-slug",
            "--date",
            "--summary",
        ],
    ),
    (
        "machine-metadata",
        &[
            "--output",
            "--date",
            "--machine-slug",
            "--git-revision",
            "--feature-set",
            "--backends",
        ],
    ),
    (
        "perf-stack-audit",
        &[
            "--input",
            "--records",
            "--record-log",
            "--record-exit-code",
            "--source",
            "--event",
            "--output",
        ],
    ),
    ("perf-record-stats", &["--input", "--output"]),
    (
        "abba-metadata",
        &[
            "--output",
            "--baseline-ref",
            "--candidate-ref",
            "--backend-override",
            "--features",
            "--sample-size",
            "--warmup-time",
            "--measurement-time",
            "--cooldown-seconds",
            "--drift-threshold",
            "--cpuset",
        ],
    ),
    ("epoch", &[]),
];

fn run() -> Result<bool> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    if command.is_empty() || ["help", "--help", "-h"].contains(&command.as_str()) {
        println!(
            "usage: cargo xtask <command> [options]\ncommands: {}",
            COMMANDS
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Ok(true);
    }
    let options = COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, options)| *options)
        .ok_or_else(|| format!("unknown command: {command}; use cargo xtask --help"))?;
    let arguments: Vec<_> = arguments.collect();
    if arguments == ["--help"] || arguments == ["-h"] {
        println!("cargo xtask {command} [{}]", options.join(" "));
        return Ok(true);
    }
    let args = Args::parse(arguments.into_iter(), options)?;
    match command.as_str() {
        "sampling-contract" => sampling_contract::run(&args)?,
        "validate-stream-views" => diagnostics::views::run(&args)?,
        "validate-diagnostic-ledger" => diagnostics::ledger::run(&args)?,
        "memory-policy" => memory_policy::run(&args)?,
        "check-sampler-telemetry" => diagnostics::check_telemetry(&args)?,
        "native-helper" => bridge::run(&args)?,
        "stream-allocator-audit" => allocator_ledger::run(&args)?,
        "check-benchmark" => return regression::benchmark(&args),
        "check-hotspot" => return regression::hotspot(&args),
        "check-release" => println!(
            "core release manifests consistent: {}",
            manifest::check(&args.root(), args.0.get("--tag").map(String::as_str))?
        ),
        "extract-criterion" => criterion::extract(&args)?,
        "analyze-abba" => return abba::run(&args),
        "audit-json" => return audit::json(&args.root()),
        "audit-package" => audit::package(&args.root())?,
        "audit-workspace" => workspace::audit(&args.root())?,
        "summarize-simd" => summary::run(&args)?,
        "summarize-smoke" => summary::local_smoke(&args)?,
        "machine-metadata" => metadata::write_machine(&args)?,
        "perf-stack-audit" => perf::run(&args)?,
        "perf-record-stats" => perf::stats(&args)?,
        "abba-metadata" => metadata::write(&args)?,
        "epoch" => println!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs_f64()
        ),
        _ => unreachable!(),
    }
    Ok(true)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(1),
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}
