//! Purpose-specific owned bytes share one strict frozen-manifest binding.
use super::*;

pub(super) struct CapsuleBytes {
    pub(super) manifest: Vec<u8>,
    pub(super) archive: Vec<u8>,
    pub(super) binary: Vec<u8>,
}
pub(super) struct Frozen<T> {
    pub(super) manifest: Manifest,
    pub(super) bytes: T,
    snapshot: Snapshot,
}
impl<T> Frozen<T> {
    /// End byte ownership before final revalidation or an owned worker run.
    pub(super) fn into_snapshot(self) -> Snapshot {
        drop(self.bytes);
        drop(self.manifest);
        self.snapshot
    }
    pub(super) fn finish(self) -> Result<()> {
        self.into_snapshot().finish()?;
        Ok(())
    }
}
fn load_with<T>(
    root: &Path,
    pin: &str,
    read: impl FnOnce(&mut Snapshot, &Manifest, Vec<u8>) -> Result<T>,
) -> Result<Frozen<T>> {
    valid_digest(pin)?;
    let mut snapshot = Snapshot::new(root)?;
    let manifest_bytes = snapshot.binary_bound("manifest.json", pin)?;
    let artifact::StrictJson(value) = serde_json::from_slice(&manifest_bytes)?;
    let manifest: Manifest = serde_json::from_value(value)?;
    manifest.validate()?;
    let bytes = read(&mut snapshot, &manifest, manifest_bytes)?;
    Ok(Frozen {
        manifest,
        bytes,
        snapshot,
    })
}
/// Installation retains exactly the original manifest, archive and binary bytes.
pub(super) fn load(root: &Path, pin: &str) -> Result<Frozen<CapsuleBytes>> {
    load_with(root, pin, |snapshot, manifest, manifest_bytes| {
        let archive = snapshot.binary_bound("source.tar.gz", &manifest.archive_sha256)?;
        let binary = snapshot.binary_bound("binary", &manifest.binary_sha256)?;
        Ok(CapsuleBytes {
            manifest: manifest_bytes,
            archive,
            binary,
        })
    })
}
/// Worker preparation copies a binary while still binding the source archive.
pub(super) fn load_binary(root: &Path, pin: &str) -> Result<Frozen<Vec<u8>>> {
    load_with(root, pin, |snapshot, manifest, manifest_bytes| {
        drop(manifest_bytes);
        snapshot.verify_binary("source.tar.gz", &manifest.archive_sha256)?;
        snapshot.binary_bound("binary", &manifest.binary_sha256)
    })
}
/// Already installed inputs retain identity and final-change checks, not bytes.
pub(super) fn load_verified(root: &Path, pin: &str) -> Result<Frozen<()>> {
    load_with(root, pin, |snapshot, manifest, manifest_bytes| {
        drop(manifest_bytes);
        snapshot.verify_binary("source.tar.gz", &manifest.archive_sha256)?;
        snapshot.verify_binary("binary", &manifest.binary_sha256)
    })
}
