//! Enforce a single dependency catalog, including target-specific dependencies.
use crate::Result;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn validate(document: &toml::Value) -> Result<()> {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = document.get(section).and_then(toml::Value::as_table) {
            for (name, dependency) in dependencies {
                if dependency.get("workspace").and_then(toml::Value::as_bool) != Some(true)
                    || ["version", "path", "git", "branch", "tag", "rev", "registry"]
                        .iter()
                        .any(|key| dependency.get(key).is_some())
                {
                    return Err(format!(
                        "{section}.{name} must inherit the root workspace dependency"
                    )
                    .into());
                }
            }
        }
    }
    if let Some(targets) = document.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            validate(target)?;
        }
    }
    Ok(())
}

pub(crate) fn audit(root: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct Package {
        manifest_path: PathBuf,
    }
    #[derive(Deserialize)]
    struct Metadata {
        packages: Vec<Package>,
    }
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    for package in &metadata.packages {
        let document: toml::Value =
            toml::from_str(&std::fs::read_to_string(&package.manifest_path)?)?;
        validate(&document)
            .map_err(|error| format!("{}: {error}", package.manifest_path.display()))?;
    }
    println!(
        "all {} workspace members inherit the root dependency catalog",
        metadata.packages.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependencies_and_target_overrides_must_inherit() {
        for section in [
            "dependencies",
            "dev-dependencies",
            "build-dependencies",
            "target.'cfg(unix)'.dependencies",
        ] {
            for bad in [
                "'1'",
                "{ version='1' }",
                "{ workspace=true, path='../other' }",
            ] {
                assert!(
                    validate(
                        &toml::from_str::<toml::Value>(&format!("[{section}]\nserde={bad}"))
                            .unwrap()
                    )
                    .is_err()
                );
            }
            assert!(
                validate(
                    &toml::from_str::<toml::Value>(&format!(
                        "[{section}]\nserde={{workspace=true,features=['derive'],optional=true}}"
                    ))
                    .unwrap()
                )
                .is_ok()
            );
        }
    }
}
