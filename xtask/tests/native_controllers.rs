//! Native controller entry points and shell release policy; no interpreter dependency.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "rse-native-cli-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}
const BASELINES: [&str; 4] = [
    "RSE_SMOKE_BASELINE",
    "RSE_SMALL_FILE_BASELINE",
    "RSE_RECONSTRUCTION_HOTSPOT_BASELINE",
    "RSE_STREAM_PATH_BASELINE",
];
const SWITCHES: [&str; 7] = [
    "RUN_BACKEND_CONSISTENCY",
    "RUN_SMALL_FILE_GATE",
    "RUN_RECONSTRUCTION_HOTSPOT_GATE",
    "RUN_STREAM_PATH_GATE",
    "RUN_SIMD_ACCEL_TESTS",
    "RUN_X86_SIMD_OVERRIDE_MATRIX",
    "RUN_AARCH64_SIMD_OVERRIDE_MATRIX",
];
#[cfg(unix)]
fn checks(
    profile: &str,
    baselines: bool,
    invalid: Option<(&str, &str)>,
    changes: &[(&str, &str)],
) -> (Output, String) {
    use std::os::unix::fs::PermissionsExt;
    let d = Directory::new();
    let bin = d.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let trace = d.0.join("trace");
    let stub = r#"#!/bin/sh
printf '%s|profile=%s|simd=%s|x86=%s|arm=%s\n' "$0 $*" "${RSE_SMOKE_PROFILE:-}" "${RUN_SIMD_ACCEL_TESTS:-}" "${RUN_X86_SIMD_OVERRIDE_MATRIX:-}" "${RUN_AARCH64_SIMD_OVERRIDE_MATRIX:-}" >> "$TRACE"
case "$*" in *"${FAIL_ON:-__never__}"*) exit 42 ;; esac
case "$*" in *"release-evidence materialize"*)
 for item do output="$item"; done
 : > "$output"
 for name in RSE_SMOKE_BASELINE RSE_SMALL_FILE_BASELINE RSE_RECONSTRUCTION_HOTSPOT_BASELINE RSE_STREAM_PATH_BASELINE; do value=$(printenv "$name"); printf '%s=%s\n' "$name" "$value" >> "$output"; done
 ;; esac
"#;
    for name in ["cargo", "bash"] {
        let path = bin.join(name);
        fs::write(&path, stub).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut env: BTreeMap<String, String> = std::env::vars().collect();
    for key in BASELINES.into_iter().chain(SWITCHES).chain([
        "RSE_BENCHMARK_ALLOW_MISSING_CASES",
        "FAIL_ON",
        "RSE_RELEASE_EVIDENCE",
    ]) {
        env.remove(key);
    }
    env.insert(
        "PATH".into(),
        format!("{}:{}", bin.display(), env.get("PATH").unwrap()),
    );
    env.insert("TRACE".into(), trace.display().to_string());
    env.insert("VALIDATION_PROFILE".into(), profile.into());
    if baselines {
        for key in BASELINES {
            let path = d.0.join(format!("{key}.json"));
            fs::write(&path, "[]\n").unwrap();
            env.insert(key.into(), path.display().to_string());
        }
    }
    if let Some((key, kind)) = invalid {
        let path = d.0.join("bad");
        match kind {
            "directory" => fs::create_dir(&path).unwrap(),
            "empty" => fs::write(&path, "").unwrap(),
            _ => (),
        };
        env.insert(key.into(), path.display().to_string());
    }
    for (key, value) in changes {
        env.insert((*key).into(), (*value).into());
    }
    let result = Command::new("/bin/bash")
        .arg(root().join("scripts/release-check.sh"))
        .current_dir(&d.0)
        .env_clear()
        .envs(env)
        .output()
        .unwrap();
    let trace = fs::read_to_string(trace).unwrap_or_default();
    (result, trace)
}
#[test]
#[cfg(unix)]
fn release_inputs_must_exist_before_any_command_executes() {
    let (result, trace) = checks("release", false, None, &[]);
    assert!(!result.status.success());
    assert!(trace.is_empty());
    for name in BASELINES {
        for kind in ["missing", "directory", "empty"] {
            let (result, trace) = checks("release", true, Some((name, kind)), &[]);
            assert!(!result.status.success(), "{name}/{kind}");
            assert!(String::from_utf8_lossy(&result.stderr).contains(name));
            assert!(trace.is_empty());
        }
    }
}
#[test]
#[cfg(unix)]
fn invalid_profiles_and_partial_release_settings_fail_closed() {
    for (profile, changes) in [
        ("typo", vec![]),
        ("release", vec![("RSE_BENCHMARK_ALLOW_MISSING_CASES", "1")]),
        ("fast", vec![("RSE_RELEASE_EVIDENCE", "fixture")]),
    ] {
        let (result, trace) = checks(profile, true, None, &changes);
        assert!(!result.status.success());
        assert!(trace.is_empty());
    }
}
#[test]
#[cfg(unix)]
fn strict_release_cannot_disable_required_functional_and_regression_gates() {
    let changes: Vec<_> = SWITCHES.into_iter().map(|key| (key, "0")).collect();
    let (result, trace) = checks("release", true, None, &changes);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for expected in [
        "check_backend_consistency.sh",
        "check-hotspot",
        "small-file-results.json",
        "stream-path-results.json",
        "smoke-results.json",
        "profile=extended|simd=1|x86=1|arm=1",
    ] {
        assert!(trace.contains(expected), "{expected}");
    }
    let smoke = trace
        .lines()
        .find(|l| l.contains("benchmark_smoke_matrix_runs_and_exports_results"))
        .unwrap();
    assert!(smoke.contains("--release --features std simd-accel"));
    assert!(
        !trace
            .lines()
            .find(|l| l.contains("--test golden_vectors"))
            .unwrap()
            .contains("--ignored")
    );
}
#[test]
#[cfg(unix)]
fn failed_native_evidence_validation_precedes_all_other_gates() {
    let (result, trace) = checks(
        "release",
        true,
        None,
        &[
            ("RSE_RELEASE_EVIDENCE", "fixture"),
            ("FAIL_ON", "release-evidence materialize"),
        ],
    );
    assert_eq!(result.status.code(), Some(42));
    assert_eq!(trace.lines().count(), 1);
    assert!(trace.contains("release-evidence materialize"));
}
#[test]
#[cfg(unix)]
fn verified_campaign_results_are_reused_while_functional_gates_remain() {
    let (result, trace) = checks(
        "release",
        true,
        None,
        &[("RSE_RELEASE_EVIDENCE", "fixture")],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for name in [
        "benchmark_smoke_matrix_runs_and_exports_results",
        "benchmark_small_file_matrix_runs_and_exports_results",
        "benchmark_reconstruction_hotspots --",
        "benchmark_stream_path_matrix_runs_and_exports_results",
    ] {
        assert!(!trace.contains(name), "{name}");
    }
    assert_eq!(trace.matches("check-benchmark").count(), 3);
    assert_eq!(trace.matches("check-hotspot").count(), 1);
    assert!(trace.contains("check_backend_consistency.sh"));
    assert!(trace.contains("--test golden_vectors"));
    assert!(String::from_utf8_lossy(&result.stdout).contains("using verified controlled campaign"));
}
#[test]
#[cfg(unix)]
fn fast_checks_select_only_the_no_std_codec_and_keep_quick_optional_profiles() {
    let (result, trace) = checks("fast", false, None, &[]);
    assert!(result.status.success());
    assert!(trace.contains("profile=quick"));
    assert!(!trace.contains("check-benchmark"));
    let command = trace
        .lines()
        .find(|l| l.contains("--no-default-features"))
        .unwrap()
        .split('|')
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let index = command.iter().position(|t| *t == "--package").unwrap();
    assert_eq!(command[index + 1], "rustfs-erasure-codec");
}
#[test]
#[cfg(unix)]
fn a_failed_regression_stops_later_release_checks() {
    let (result, trace) = checks("release", true, None, &[("FAIL_ON", "check-benchmark")]);
    assert_eq!(result.status.code(), Some(42));
    assert!(!trace.contains("check_backend_consistency.sh"));
}
#[test]
fn native_controller_help_lists_real_required_arguments() {
    for (command, flag) in [
        ("capture-plan-campaign", "--baseline-sha256"),
        ("replay-plan-campaign", "--plan-sha256"),
        ("replay-plan-samples", "--manifest-sha256"),
        ("release-evidence", "--manifest"),
        ("release-sampling", "--baseline-ref"),
        ("diagnose-release-vm", "--native-service-clock"),
        ("diagnose-fixture-layout", "--check-dynamic"),
        ("lock-version", "--package"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout).unwrap().contains(flag));
    }
}
#[test]
#[cfg(unix)]
fn a_missing_native_status_writer_leaves_a_fixed_failure_journal() {
    use std::os::unix::fs::PermissionsExt;
    let d = Directory::new();
    let bin = d.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let cargo = bin.join("cargo");
    fs::write(&cargo, "#!/bin/sh\nexit 7\n").unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let output = d.0.join("status.json");
    fs::write(&output, "{\"status\":\"success\"}").unwrap();
    let result = Command::new("/bin/bash")
        .arg(root().join("scripts/write-ci-status.sh"))
        .arg("preflight")
        .arg(&output)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    assert!(!result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(value["status"], "failure");
    assert_eq!(value["registry_uploaded"], false);
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&path, &destination);
        } else {
            fs::copy(path, destination).unwrap();
        }
    }
}
#[test]
fn stale_native_code_overwrites_a_previous_passing_report_before_any_verification() {
    let d = Directory::new();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "xtask/Cargo.toml",
        "xtask/build.rs",
        "xtask/source_stamp.rs",
        "xtask/contracts/benchmark.json",
    ] {
        let destination = d.0.join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(root().join(name), destination).unwrap();
    }
    for name in [
        "xtask/src",
        "scripts/release-sampling",
        "scripts/diagnostics",
    ] {
        copy_tree(&root().join(name), &d.0.join(name));
    }
    let changed = d.0.join("xtask/src/main.rs");
    let original = fs::read(&changed).unwrap();
    let mut edited = original.clone();
    edited.extend_from_slice(b"\n//changed after compilation\n");
    fs::write(&changed, edited).unwrap();
    let report = d.0.join("report.json");
    fs::write(&report, "{\"status\":\"passed\"}").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["release-evidence", "verify", "--manifest"])
        .arg(d.0.join("missing.json"))
        .arg("--report")
        .arg(&report)
        .env("RSE_NATIVE_TOOL_ROOT", &d.0)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(value["status"], "failed");
    assert!(value["error"].as_str().unwrap().contains("rebuild"));
    fs::write(changed, original).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["release-evidence", "verify", "--manifest"])
        .arg(d.0.join("missing.json"))
        .arg("--report")
        .arg(&report)
        .env("RSE_NATIVE_TOOL_ROOT", &d.0)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert!(!value["error"].as_str().unwrap().contains("rebuild"));
}
