//! Build and runtime share the same repository-only source/asset fingerprint.
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
fn rust_files(root: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_symlink() {
            return Err(io::Error::other(
                "symlinked native source or asset is not a stable input",
            ));
        }
        if kind.is_dir() {
            rust_files(&path, out)?;
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
pub fn digest(root: &Path) -> io::Result<String> {
    let mut files = [
        "Cargo.toml",
        "Cargo.lock",
        "xtask/Cargo.toml",
        "xtask/build.rs",
        "xtask/source_stamp.rs",
        "xtask/contracts/benchmark.json",
    ]
    .map(|p| root.join(p))
    .to_vec();
    for directory in [
        "xtask/src",
        "scripts/release-sampling",
        "scripts/diagnostics",
    ] {
        rust_files(&root.join(directory), &mut files)?;
    }
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        for component in relative.components() {
            digest.update(component.as_os_str().as_encoded_bytes());
            digest.update(b"/");
        }
        digest.update([0]);
        digest.update(fs::read(path)?);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
