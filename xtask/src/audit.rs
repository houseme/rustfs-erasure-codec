use crate::Result;
use crate::artifact::StrictJson;
use serde_json::json;
use std::{collections::BTreeMap, path::Path, process::Command};

pub(crate) fn json(root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.json",
            "*.py",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("git ls-files failed".into());
    }
    let mut categories: BTreeMap<&str, usize> = BTreeMap::new();
    let mut contents: BTreeMap<Vec<u8>, Vec<String>> = BTreeMap::new();
    let mut python_files = Vec::new();
    let mut errors = Vec::new();
    let mut bytes = 0;
    for path in output.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = std::str::from_utf8(path)?;
        let absolute = root.join(path);
        // Deleted tracked files still appear until staged. Audit the worktree.
        if !absolute.exists() {
            continue;
        }
        if path.ends_with(".py") {
            python_files.push(path);
            continue;
        }
        let content = std::fs::read(&absolute)?;
        bytes += content.len();
        match serde_json::from_slice::<StrictJson>(&content) {
            Ok(StrictJson(value)) => {
                let category = if path.starts_with("xtask/contracts/") {
                    "repository tool contracts"
                } else if path.starts_with("wasm/") {
                    "wasm configuration and lockfiles"
                } else if path.ends_with("/estimates.json")
                    || path.ends_with("/sample.json")
                    || path.ends_with("/benchmark.json")
                    || path.ends_with("/tukey.json")
                {
                    "raw Criterion evidence"
                } else if value.get("schema_version").is_some() || value.get("schema").is_some() {
                    "versioned evidence"
                } else {
                    "historical reports and evidence"
                };
                *categories.entry(category).or_default() += 1;
            }
            Err(error) => errors.push(format!("{path}: {error}")),
        }
        contents.entry(content).or_default().push(path.to_owned());
    }
    python_files.sort_unstable();
    for paths in contents.values_mut() {
        paths.sort_unstable();
    }
    let mut identical: Vec<_> = contents
        .into_values()
        .filter(|paths| paths.len() > 1)
        .collect();
    identical.sort();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "json_files": categories.values().sum::<usize>() + errors.len(), "json_bytes": bytes,
            "categories": categories, "python_files": python_files, "identical_byte_groups": identical,
            "errors": errors, "action": "read-only; retain provenance-bound historical evidence"
        }))?
    );
    Ok(errors.is_empty())
}

/// Guard every tracked/non-ignored executable surface after retiring the interpreter.
pub(crate) fn no_python(root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("git source inventory failed".into());
    }
    let interpreter = regex::Regex::new(
        r#"(?i)(?:^|[\s\"'=;/|&(])(?:python[0-9]*(?:\.[0-9]+)?|pypy[0-9]*)(?:$|[\s\"'();|&])|actions/setup-python@"#,
    )?;
    let mut errors = Vec::new();
    let mut python_files = 0;
    for name in output.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let name = std::str::from_utf8(name)?;
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if [
            "py", "pyw", "pyx", "pyi", "pyc", "pyo", "pyd", "ipynb", "sage",
        ]
        .contains(&extension)
        {
            errors.push(name.to_owned());
            python_files += 1;
            continue;
        }
        if ["sh", "yml", "yaml"].contains(&extension) {
            let text = std::fs::read_to_string(path)?;
            for (index, line) in text.lines().enumerate() {
                if line.trim_start().starts_with('#') && !line.trim_start().starts_with("#!") {
                    continue;
                }
                if interpreter.is_match(line) {
                    errors.push(format!("{name}:{}: interpreter dependency", index + 1));
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"python_files":python_files,"errors":errors,"scope":"tracked and non-ignored sources, CI and shell entry points"})
        )?
    );
    Ok(errors.is_empty())
}

#[derive(Clone, Copy)]
enum Crate {
    Codec,
    Wasm,
}
impl Crate {
    fn name(self) -> &'static str {
        match self {
            Self::Codec => "rustfs-erasure-codec",
            Self::Wasm => "rustfs-erasure-codec-wasm",
        }
    }
}

fn validate_package(files: &str, kind: Crate) -> Result<usize> {
    let mut allowed = vec![
        ".cargo_vcs_info.json",
        "Cargo.toml",
        "Cargo.toml.orig",
        "Cargo.lock",
        "README.md",
    ];
    let mut required = vec!["Cargo.toml", "Cargo.lock", "README.md", "src/lib.rs"];
    if matches!(kind, Crate::Codec) {
        allowed.extend(["LICENSE", "build.rs"]);
        required.extend(["LICENSE", "build.rs"]);
    }
    let mut count = 0;
    for path in files.lines() {
        let source = path.starts_with("src/") && path.ends_with(".rs");
        if !allowed.contains(&path) && !source {
            return Err(format!("{}: unexpected distributable member: {path}", kind.name()).into());
        }
        count += 1;
    }
    for path in required {
        if !files.lines().any(|file| file == path) {
            return Err(format!("{}: missing distributable member: {path}", kind.name()).into());
        }
    }
    Ok(count)
}

pub(crate) fn package(root: &Path) -> Result<()> {
    for kind in [Crate::Codec, Crate::Wasm] {
        let output = Command::new("cargo")
            .args([
                "package",
                "--list",
                "--allow-dirty",
                "--locked",
                "--offline",
                "--package",
                kind.name(),
            ])
            .current_dir(root)
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        let count = validate_package(std::str::from_utf8(&output.stdout)?, kind)?;
        println!(
            "{} allowlist verified: {count} members; repository tooling/evidence excluded",
            kind.name()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_or_invalid_json_is_rejected_recursively() {
        for source in [
            r#"{"a":1,"a":2}"#,
            r#"[{"a":1,"a":2}]"#,
            "NaN",
            "1e999",
            "{} garbage",
        ] {
            assert!(
                serde_json::from_str::<StrictJson>(source).is_err(),
                "{source}"
            );
        }
        assert!(serde_json::from_str::<StrictJson>(r#"{"a":[1,true,null,{"b":"ok"}]}"#).is_ok());
    }
    #[test]
    fn package_requires_build_inputs_and_rejects_repository_files() {
        let good = "Cargo.toml\nCargo.lock\nREADME.md\nLICENSE\nbuild.rs\nsrc/lib.rs\n";
        assert_eq!(validate_package(good, Crate::Codec).unwrap(), 6);
        for member in [
            "scripts/tool.py",
            "xtask/src/main.rs",
            "benchmarks/result.json",
            "tests/benchmark_smoke.rs",
            ".idea/workspace.xml",
            "src/diagnostics.py",
            "src/benchmark.json",
            "src/README.md",
        ] {
            assert!(validate_package(&(good.to_owned() + member), Crate::Codec).is_err());
        }
        assert!(validate_package(&good.replace("build.rs\n", ""), Crate::Codec).is_err());
    }
    #[test]
    fn wasm_crate_excludes_javascript_build_and_browser_assets() {
        let good = "Cargo.toml\nCargo.lock\nREADME.md\nsrc/lib.rs\n";
        assert_eq!(validate_package(good, Crate::Wasm).unwrap(), 4);
        for member in [
            "package.json",
            "package-lock.json",
            "tsconfig.json",
            "src/index.ts",
            "scripts/build.mjs",
            "tests/index.test.mjs",
            "playwright.config.mjs",
        ] {
            assert!(validate_package(&(good.to_owned() + member), Crate::Wasm).is_err());
        }
    }
}
