//! Source fingerprints shared by native replay and the legacy capture bindings.
mod harness;
use crate::{Result, artifact};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn files(root: &Path, recursive: bool, extension: &str) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut result = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if recursive && kind.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|v| v == extension)
                && (kind.is_file() || kind.is_symlink())
            {
                result.push(path);
            }
        }
    }
    result.sort();
    Ok(result)
}
fn native_bytes(root: &Path, hasher: &mut Sha256) -> Result<()> {
    let mut paths = Vec::new();
    let legacy = root.join("scripts/native_tools.py");
    if legacy.is_file() {
        paths.push(legacy);
    }
    paths.extend(["Cargo.toml", "Cargo.lock", "xtask/Cargo.toml"].map(|name| root.join(name)));
    for name in ["xtask/build.rs", "xtask/source_stamp.rs"] {
        let path = root.join(name);
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.extend(files(&root.join("xtask/src"), true, "rs")?);
    paths.extend(files(&root.join("xtask/contracts"), false, "json")?);
    for path in paths {
        // Rebuild components with the platform separator, matching pathlib's
        // relative-path spelling even when joins used forward slashes.
        let relative: PathBuf = path.strip_prefix(root)?.components().collect();
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(path)?);
    }
    Ok(())
}
pub(crate) fn sampling(root: &Path) -> Result<String> {
    let native = root.join("xtask/src/control/sampling.rs");
    sampling_with(
        root,
        &if native.is_file() {
            native
        } else {
            root.join("scripts/release_sampling.py")
        },
    )
}
fn sampling_with(root: &Path, controller: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    let mut paths = vec![controller.to_owned()];
    let templates = if controller.extension().is_some_and(|e| e == "rs") {
        root.join("scripts/release-sampling")
    } else {
        controller
            .parent()
            .ok_or("missing sampler controller parent")?
            .join("release-sampling")
    };
    paths.extend(files(&templates, false, "rs")?);
    for path in paths {
        hash.update(
            path.file_name()
                .ok_or("missing sampling source name")?
                .to_string_lossy()
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(fs::read(path)?);
    }
    native_bytes(root, &mut hash)?;
    Ok(artifact::snapshot::hex(hash.finalize().as_slice()))
}
pub(crate) fn file(path: &Path) -> Result<String> {
    Ok(artifact::snapshot::digest(&fs::read(path)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Kind {
    Native,
    Sampling,
    Views,
    Harness,
}
/// Read the current source bytes on every call; source identities are never cached.
pub(crate) fn source(root: &Path, kind: Kind, controller: Option<&Path>) -> Result<String> {
    let mut hash = Sha256::new();
    match kind {
        Kind::Native => {}
        Kind::Sampling => {
            return if let Some(controller) = controller {
                sampling_with(root, controller)
            } else {
                sampling(root)
            };
        }
        Kind::Views => {
            let native = root.join("xtask/src/control/inventory.rs");
            let default = if native.is_file() {
                native
            } else {
                root.join("scripts/stream_view_audit.py")
            };
            hash.update(fs::read(controller.unwrap_or(&default))?);
            hash.update(crate::diagnostics::templates::native("views", root)?.as_bytes());
        }
        Kind::Harness => return harness::digest(root),
    }
    native_bytes(root, &mut hash)?;
    Ok(artifact::snapshot::hex(hash.finalize().as_slice()))
}
/// Match the original sorted filename/NUL/normalized-text overlay byte stream.
pub(crate) fn overlay(files: &BTreeMap<String, String>, root: &Path) -> Result<String> {
    let root = root.to_str().ok_or("invalid overlay source path")?;
    let mut hash = Sha256::new();
    for (name, text) in files {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(text.replace(root, "<SOURCE>").as_bytes());
    }
    Ok(artifact::snapshot::hex(hash.finalize().as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::snapshot::tests::Directory;
    #[test]
    fn native_and_sampler_hashes_match_original_byte_streams_and_are_never_cached() {
        let directory = Directory::new();
        let data = [
            ("scripts/native_tools.py", "binding\n"),
            ("Cargo.toml", "manifest\n"),
            ("Cargo.lock", "lock\n"),
            ("xtask/Cargo.toml", "tool\n"),
            ("xtask/src/z.rs", "last\n"),
            ("xtask/src/nested/a.rs", "nested\r\n"),
            ("xtask/contracts/benchmark.json", "{}\n"),
            ("scripts/release_sampling.py", "sampling\n"),
            ("scripts/release-sampling/z.rs", "tail\n"),
            ("scripts/release-sampling/a.rs", "head\n"),
        ];
        for (name, text) in data {
            let path = directory.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let (native, sampler) = if cfg!(windows) {
            (
                "f00fe74d41912120826769fc08267bf6c9319f2a6a5f4c38cacc26dfd59059d0",
                "53d846ff8a9be307fc3cb0aeee7d15a720963f6cacc579665ba6f7369f8769c1",
            )
        } else {
            (
                "f8973c886aa77fbd32c72604039ab2b52d9518b8d0c95951a1d47379b2ec0de9",
                "758e0598f9caa6b4978513d36025d8c402d4a612a972b0b51d2cf7c862f2e3ad",
            )
        };
        assert_eq!(source(&directory.0, Kind::Native, None).unwrap(), native);
        assert_eq!(source(&directory.0, Kind::Sampling, None).unwrap(), sampler);
        fs::write(directory.0.join("README.md"), "unrelated documentation").unwrap();
        assert_eq!(source(&directory.0, Kind::Native, None).unwrap(), native);
        let alternate = directory.0.join("alternate/release_sampling.py");
        fs::create_dir_all(alternate.parent().unwrap().join("release-sampling")).unwrap();
        for name in ["a.rs", "z.rs"] {
            fs::copy(
                directory.0.join("scripts/release-sampling").join(name),
                alternate
                    .parent()
                    .unwrap()
                    .join("release-sampling")
                    .join(name),
            )
            .unwrap();
        }
        fs::write(&alternate, "sampling\n").unwrap();
        assert_eq!(
            source(&directory.0, Kind::Sampling, Some(&alternate)).unwrap(),
            sampler
        );
        fs::write(&alternate, "changed caller source").unwrap();
        assert_ne!(
            source(&directory.0, Kind::Sampling, Some(&alternate)).unwrap(),
            sampler
        );
        fs::write(directory.0.join("xtask/src/z.rs"), "changed").unwrap();
        assert_ne!(source(&directory.0, Kind::Native, None).unwrap(), native);
        assert_ne!(source(&directory.0, Kind::Sampling, None).unwrap(), sampler);
        fs::write(directory.0.join("xtask/src/z.rs"), "last\n").unwrap();
        assert_eq!(source(&directory.0, Kind::Native, None).unwrap(), native);
    }
    #[test]
    fn overlay_hash_keeps_sorted_names_null_separators_and_original_text_bytes() {
        let mut files = BTreeMap::new();
        files.insert("z.rs".into(), "second /origin /origin".into());
        files.insert("a.rs".into(), "/origin value\r\n雪".into());
        let before = files.clone();
        assert_eq!(
            overlay(&files, Path::new("/origin")).unwrap(),
            "e9f97c1a09ca9d75f26ad1001cb8c2f4b494686cd23b8b82b008577e25db57d2"
        );
        assert_eq!(files, before);
        assert_ne!(
            overlay(&files, Path::new("/other")).unwrap(),
            overlay(&files, Path::new("/origin")).unwrap()
        );
    }
}
