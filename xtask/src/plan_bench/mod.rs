//! Frozen, functional-only plan benchmark capture. Never a timing/release gate.
pub(crate) mod campaign;
mod model;
mod run;
pub(crate) mod samples;
#[cfg(test)]
mod tests;

use crate::{Args, Result, artifact};
use artifact::snapshot::{Snapshot, digest};
use model::*;
use std::{fs, path::Path, process::Command, time::Duration};

fn new_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|e| format!("require a new output directory: {e}").into())
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn worker(binary: &Path, argument: &str, directory: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(directory)
        .arg(argument)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NO_COLOR", "1")
        .env("RAYON_NUM_THREADS", "1");
    command
}
fn bounded_log(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(artifact::snapshot::MEMBER_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > artifact::snapshot::MEMBER_LIMIT {
        return Err("worker output exceeds original member budget; preserve raw files".into());
    }
    Ok(bytes)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").current_dir(root).args(args).output()?;
    if !out.status.success() {
        return Err("git identity lookup failed".into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}
fn command_text(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .env("RUSTUP_TOOLCHAIN", "stable")
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(format!("{program} identity unavailable").into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}
struct Frozen {
    manifest: Manifest,
    manifest_bytes: Vec<u8>,
    archive: Vec<u8>,
    binary: Vec<u8>,
    snapshot: Snapshot,
}
fn load(root: &Path, pin: &str) -> Result<Frozen> {
    valid_digest(pin)?;
    let mut snapshot = Snapshot::new(root)?;
    let manifest_bytes = snapshot.binary_bound("manifest.json", pin)?;
    let artifact::StrictJson(value) = serde_json::from_slice(&manifest_bytes)?;
    let manifest: Manifest = serde_json::from_value(value)?;
    manifest.validate()?;
    let archive = snapshot.binary_bound("source.tar.gz", &manifest.archive_sha256)?;
    let binary = snapshot.binary_bound("binary", &manifest.binary_sha256)?;
    Ok(Frozen {
        manifest,
        manifest_bytes,
        archive,
        binary,
        snapshot,
    })
}

fn compatible(a: &Manifest, b: &Manifest) -> Result<()> {
    if a.compiler != b.compiler || a.features != b.features || a.catalog != b.catalog {
        return Err("campaign requires identical compiler, features and full case catalog".into());
    }
    Ok(())
}

pub(crate) fn freeze(args: &Args) -> Result<()> {
    if !cfg!(unix) {
        return Err("plan benchmark freezing requires Unix cargo/tar tools".into());
    }
    let repository = fs::canonicalize(args.root())?;
    let output = Path::new(args.required("--output")?);
    new_directory(output)?;
    let output = fs::canonicalize(output)?;
    let commit = git(
        &repository,
        &[
            "rev-parse",
            &format!("{}^{{commit}}", args.get("--revision", "HEAD")),
        ],
    )?;
    valid_revision(&commit)?;
    let tree = git(&repository, &["rev-parse", &format!("{commit}^{{tree}}")])?;
    let archive = output.join("source.tar.gz");
    let mut archive_cmd = Command::new("git");
    archive_cmd
        .current_dir(&repository)
        .args(["archive", "--format=tar.gz", &commit, "--output"])
        .arg(&archive);
    run::checked(archive_cmd, &output, "archive", Duration::from_secs(60))?;
    let source = output.join("build-source");
    new_directory(&source)?;
    let mut unpack = Command::new("tar");
    unpack.arg("-xzf").arg(&archive).arg("-C").arg(&source);
    run::checked(unpack, &output, "unpack", Duration::from_secs(60))?;
    let catalog_bytes = fs::read(source.join("benches/reconstruct_plans/cases.txt"))?;
    let catalog = catalog(&String::from_utf8(catalog_bytes.clone())?)?;
    let compiler = command_text("rustc", &["--version", "--verbose"])?;
    let host = compiler
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("missing compiler host")?;
    let target = fs::canonicalize(args.required("--target-dir")?)?;
    let source_digest = source_hash(&source)?;
    let mut build = Command::new("cargo");
    build
        .current_dir(&source)
        .env("RUSTUP_TOOLCHAIN", "stable")
        .env("RUSTFLAGS", "")
        .env("CARGO_ENCODED_RUSTFLAGS", "")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTC")
        .env_remove("CARGO_BUILD_TARGET")
        .args([
            "bench",
            "--no-run",
            "--offline",
            "--locked",
            "-p",
            "rustfs-erasure-codec",
            "--features",
            "simd-accel",
            "--bench",
            "reconstruct_plans",
            "--message-format=json",
            "--target",
        ])
        .arg(host)
        .arg("--target-dir")
        .arg(&target);
    for (key, _) in std::env::vars_os() {
        if key
            .to_str()
            .is_some_and(|k| k.starts_with("CARGO_PROFILE_"))
        {
            build.env_remove(key);
        }
    }
    run::checked(build, &output, "build", Duration::from_secs(1200))?;
    if source_hash(&source)? != source_digest {
        return Err("build source changed during compilation".into());
    }
    let binary = build_executable(
        &fs::read_to_string(output.join("build.stdout"))?,
        &source,
        &target,
    )?;
    let bytes = fs::read(binary)?;
    if bytes.is_empty() || bytes.len() as u64 > artifact::snapshot::MEMBER_LIMIT {
        return Err("benchmark binary exceeds bounded member budget".into());
    }
    write(&output.join("binary"), &bytes)?;
    executable(&output.join("binary"))?;
    let list = worker(&output.join("binary"), "--list", &output);
    run::checked(list, &output, "catalog", Duration::from_secs(120))?;
    validate_list(
        &fs::read_to_string(output.join("catalog.stdout"))?,
        &catalog,
    )?;
    let manifest = Manifest {
        schema: SCHEMA.into(),
        source_commit: commit,
        source_tree: tree,
        source_sha256: source_digest,
        compiler,
        features: vec!["std".into(), "simd-accel".into()],
        binary_sha256: digest(&bytes),
        archive_sha256: digest(&fs::read(&archive)?),
        catalog_sha256: digest(&catalog_bytes),
        catalog,
    };
    manifest.validate()?;
    artifact::write_json(&output.join("manifest.json"), &manifest)?;
    // Validate all bounded original members after building before publishing the pin.
    let pin = crate::fingerprint::file(&output.join("manifest.json"))?;
    load(&output, &pin)?.snapshot.finish()?;
    fs::remove_dir_all(&source)?;
    println!("frozen functional plan benchmark; manifest sha256: {pin}");
    Ok(())
}

pub(crate) fn capture(args: &Args) -> Result<bool> {
    if !cfg!(unix) {
        return Err("plan benchmark capture requires Unix".into());
    }
    let stage = args.required("--stage")?;
    valid_stage(stage)?;
    let pin = args.required("--manifest-sha256")?;
    let frozen = Path::new(args.required("--frozen")?);
    let original = load(frozen, pin)?;
    let timeout: u64 = args.get("--timeout-seconds", "120").parse()?;
    if !(1..=1200).contains(&timeout) {
        return Err("timeout must be between 1 and 1200 seconds".into());
    }
    let output = Path::new(args.required("--output")?);
    install(&original, output)?;
    original.snapshot.finish()?;
    Ok(
        capture_prepared(output, pin, stage, Duration::from_secs(timeout))?
            .0
            .accepted,
    )
}

fn install(original: &Frozen, output: &Path) -> Result<()> {
    new_directory(output)?;
    write(&output.join("manifest.json"), &original.manifest_bytes)?;
    write(&output.join("source.tar.gz"), &original.archive)?;
    write(&output.join("binary"), &original.binary)?;
    executable(&output.join("binary"))
}

fn capture_prepared(
    output: &Path,
    pin: &str,
    stage: &str,
    timeout: Duration,
) -> Result<(Check, String)> {
    let output = fs::canonicalize(output)?;
    let originals = load(&output, pin)?;
    let command = worker(&output.join("binary"), "--test", &output);
    let outcome = run::execute(command, &output, "worker", timeout)?;
    write(
        &output.join("worker.outcome.json"),
        &serde_json::to_vec_pretty(&outcome)?,
    )?;
    let stdout = bounded_log(&output.join("worker.stdout"))?;
    let stderr = bounded_log(&output.join("worker.stderr"))?;
    let unchanged = originals.snapshot.finish().is_ok();
    let receipt = Receipt {
        schema: SCHEMA.into(),
        stage: stage.into(),
        mode: "functional".into(),
        manifest_sha256: pin.into(),
        stdout_sha256: digest(&stdout),
        stderr_sha256: digest(&stderr),
        outcome,
        originals_unchanged: unchanged,
    };
    artifact::write_json(&output.join("receipt.json"), &receipt)?;
    let receipt_pin = crate::fingerprint::file(&output.join("receipt.json"))?;
    let check = verify(&output, pin, &receipt_pin, stage)?;
    artifact::write_json(&output.join("replay.json"), &check)?;
    println!(
        "{stage}: {}/{} functional cases; accepted={}; performance_accepted=false",
        check.observed,
        originals.manifest.catalog.len(),
        check.accepted
    );
    println!("receipt sha256: {receipt_pin}");
    Ok((check, receipt_pin))
}

pub(crate) fn verify(root: &Path, pin: &str, receipt_pin: &str, stage: &str) -> Result<Check> {
    valid_stage(stage)?;
    valid_digest(receipt_pin)?;
    let mut snapshot = Snapshot::new(root)?;
    let (check, _) = verify_snapshot(&mut snapshot, "", pin, receipt_pin, stage)?;
    snapshot.finish()?;
    Ok(check)
}

// The campaign owns one snapshot across all four stages, including the final re-read.
fn verify_snapshot(
    snapshot: &mut Snapshot,
    prefix: &str,
    pin: &str,
    receipt_pin: &str,
    stage: &str,
) -> Result<(Check, Manifest)> {
    valid_stage(stage)?;
    valid_digest(pin)?;
    valid_digest(receipt_pin)?;
    let manifest = frozen_snapshot(snapshot, prefix, pin)?;
    let value = snapshot.json_bound(&format!("{prefix}receipt.json"), Some(receipt_pin))?;
    let receipt: Receipt = serde_json::from_value(value)?;
    if receipt.schema != SCHEMA
        || receipt.stage != stage
        || receipt.mode != "functional"
        || receipt.manifest_sha256 != pin
    {
        return Err("capture stage, mode or trusted source pin mismatch".into());
    }
    let stdout = snapshot.raw_bound(&format!("{prefix}worker.stdout"), &receipt.stdout_sha256)?;
    snapshot.raw_bound(&format!("{prefix}worker.stderr"), &receipt.stderr_sha256)?;
    let mut check = check_output(std::str::from_utf8(&stdout)?, &manifest.catalog);
    if receipt.outcome.exit_code != Some(0)
        || receipt.outcome.reason != "exited"
        || !receipt.outcome.reaped
        || !receipt.originals_unchanged
    {
        check
            .errors
            .push("worker failed, timed out, was not reaped, or original inputs changed".into());
    }
    check.accepted = check.errors.is_empty();
    Ok((check, manifest))
}
fn frozen_snapshot(snapshot: &mut Snapshot, prefix: &str, pin: &str) -> Result<Manifest> {
    let manifest = manifest_snapshot(snapshot, prefix, pin)?;
    snapshot.binary_bound(&format!("{prefix}source.tar.gz"), &manifest.archive_sha256)?;
    snapshot.binary_bound(&format!("{prefix}binary"), &manifest.binary_sha256)?;
    Ok(manifest)
}

fn manifest_snapshot(snapshot: &mut Snapshot, prefix: &str, pin: &str) -> Result<Manifest> {
    let value = snapshot.json_bound(&format!("{prefix}manifest.json"), Some(pin))?;
    let manifest: Manifest = serde_json::from_value(value)?;
    manifest.validate()?;
    Ok(manifest)
}
pub(crate) fn replay(args: &Args) -> Result<bool> {
    let check = verify(
        Path::new(args.required("--input")?),
        args.required("--manifest-sha256")?,
        args.required("--receipt-sha256")?,
        args.required("--stage")?,
    )?;
    println!("{}", serde_json::to_string_pretty(&check)?);
    Ok(check.accepted)
}
