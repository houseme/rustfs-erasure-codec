//! Source fingerprints shared by native replay and the legacy capture bindings.
use crate::{Result, artifact};
use sha2::{Digest, Sha256};
use std::{
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
    let mut paths = [
        "scripts/native_tools.py",
        "Cargo.toml",
        "Cargo.lock",
        "xtask/Cargo.toml",
    ]
    .map(|n| root.join(n))
    .to_vec();
    paths.extend(files(&root.join("xtask/src"), true, "rs")?);
    paths.extend(files(&root.join("xtask/contracts"), false, "json")?);
    for path in paths {
        hasher.update(path.strip_prefix(root)?.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(path)?);
    }
    Ok(())
}
pub(crate) fn sampling(root: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    let mut paths = vec![root.join("scripts/release_sampling.py")];
    paths.extend(files(&root.join("scripts/release-sampling"), false, "rs")?);
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
