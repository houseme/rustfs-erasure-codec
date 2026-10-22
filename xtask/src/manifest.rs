use crate::Result;
use serde::Deserialize;
use std::path::Path;

const CORE: &str = "rustfs-erasure-codec";

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    source: Option<String>,
}
#[derive(Deserialize)]
struct Dependency {
    version: String,
}
#[derive(Deserialize)]
struct Dependencies {
    #[serde(rename = "rustfs-erasure-codec")]
    core: Dependency,
}
#[derive(Deserialize)]
struct Workspace {
    dependencies: Dependencies,
}
#[derive(Deserialize)]
struct Manifest {
    package: Package,
    workspace: Workspace,
}
#[derive(Deserialize)]
struct Lockfile {
    package: Vec<Package>,
}

fn validate(manifest: &str, lock: &str, tag: Option<&str>) -> Result<String> {
    let manifest: Manifest = toml::from_str(manifest)?;
    let lock: Lockfile = toml::from_str(lock)?;
    let package = manifest.package;
    if package.name != CORE || package.version.is_empty() {
        return Err("expected the core rustfs-erasure-codec package and a version".into());
    }
    if manifest.workspace.dependencies.core.version != package.version {
        return Err("core package and workspace dependency versions differ".into());
    }
    let locked: Vec<_> = lock
        .package
        .iter()
        .filter(|p| p.name == CORE && p.source.is_none())
        .map(|p| &p.version)
        .collect();
    if locked != [&package.version] {
        return Err("core package and lockfile versions differ".into());
    }
    if let Some(tag) = tag
        && tag != package.version
        && tag != format!("v{}", package.version)
    {
        return Err(format!(
            "release tag must be {} or v{}",
            package.version, package.version
        )
        .into());
    }
    Ok(package.version)
}

pub(crate) fn check(root: &Path, tag: Option<&str>) -> Result<String> {
    validate(
        &std::fs::read_to_string(root.join("Cargo.toml"))?,
        &std::fs::read_to_string(root.join("Cargo.lock"))?,
        tag,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn documents(version: &str, workspace: &str, locked: &str) -> (String, String) {
        (
            format!(
                "[package]\nname='{CORE}'\nversion='{version}'\n[workspace.dependencies]\nother='1'\n{CORE}={{version='{workspace}',path='.'}}"
            ),
            format!("[[package]]\nname='{CORE}'\nversion='{locked}'"),
        )
    }

    #[test]
    fn exact_tags_and_versions() {
        for version in ["9.1.0", "9.1.0-rc.1"] {
            let (manifest, lock) = documents(version, version, version);
            for tag in [None, Some(version), Some(format!("v{version}").as_str())] {
                assert_eq!(validate(&manifest, &lock, tag).unwrap(), version);
            }
            for tag in ["", "main", "9.1", "v9.1.0-extra", "prefix9.1.0"] {
                assert!(validate(&manifest, &lock, Some(tag)).is_err());
            }
        }
        for (version, workspace, lock) in [("9.1.0", "9.0.0", "9.1.0"), ("9.1.0", "9.1.0", "9.0.0")]
        {
            let (manifest, lock) = documents(version, workspace, lock);
            assert!(
                validate(&manifest, &lock, None)
                    .unwrap_err()
                    .to_string()
                    .contains("versions differ")
            );
        }
    }

    #[test]
    fn missing_or_duplicate_core_packages_fail() {
        let (manifest, lock) = documents("9.1.0", "9.1.0", "9.1.0");
        for broken in [String::new(), format!("{lock}\n{lock}")] {
            assert!(validate(&manifest, &broken, None).is_err());
        }
        assert!(validate("", &lock, None).is_err());
    }

    #[test]
    fn registry_baseline_is_distinct_from_local_candidate() {
        let (manifest, lock) = documents("9.1.0", "9.1.0", "9.1.0");
        let published = format!(
            "\n[[package]]\nname='{CORE}'\nversion='9.0.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'"
        );
        assert_eq!(
            validate(&manifest, &(lock + &published), None).unwrap(),
            "9.1.0"
        );
        let (_, wrong) = documents("9.1.0", "9.1.0", "9.0.0");
        assert!(validate(&manifest, &(wrong + &published), None).is_err());
    }
}
