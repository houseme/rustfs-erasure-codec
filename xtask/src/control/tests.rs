use super::*;
use crate::artifact::snapshot::tests::Directory;
use std::fs;
fn stamp_root() -> Directory {
    let d = Directory::new();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "xtask/Cargo.toml",
        "xtask/build.rs",
        "xtask/source_stamp.rs",
        "xtask/contracts/benchmark.json",
        "xtask/src/main.rs",
        "scripts/release-sampling/worker.rs",
        "scripts/diagnostics/observer.rs",
    ] {
        let path = d.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, name).unwrap();
    }
    d
}
#[test]
fn compiled_identity_binds_every_native_source_and_asset_without_binding_docs() {
    let d = stamp_root();
    let original = source_stamp::digest(&d.0).unwrap();
    fs::write(d.0.join("README.md"), "docs").unwrap();
    assert_eq!(source_stamp::digest(&d.0).unwrap(), original);
    for name in [
        "xtask/src/main.rs",
        "scripts/release-sampling/worker.rs",
        "scripts/diagnostics/observer.rs",
        "Cargo.lock",
    ] {
        fs::write(d.0.join(name), "changed").unwrap();
        assert_ne!(source_stamp::digest(&d.0).unwrap(), original);
        fs::write(d.0.join(name), name).unwrap();
        assert_eq!(source_stamp::digest(&d.0).unwrap(), original);
    }
    fs::write(d.0.join("xtask/src/new.rs"), "new").unwrap();
    assert_ne!(source_stamp::digest(&d.0).unwrap(), original);
}
#[test]
#[cfg(unix)]
fn symlinked_controller_inputs_cannot_match_a_compiled_identity() {
    use std::os::unix::fs::symlink;
    let d = stamp_root();
    let path = d.0.join("scripts/diagnostics/observer.rs");
    fs::remove_file(&path).unwrap();
    symlink(d.0.join("xtask/src/main.rs"), path).unwrap();
    assert!(source_stamp::digest(&d.0).is_err());
}
#[test]
fn shared_command_grammar_rejects_duplicates_unknown_options_and_mode_leaks() {
    assert!(
        Options::command("release-sampling", vec!["--check".into(), "--check".into()]).is_err()
    );
    assert!(Options::command("release-sampling", vec!["--unknown".into()]).is_err());
    assert!(Options::command("release-sampling", vec!["--baseline-ref".into()]).is_err());
    let options = Options::command(
        "release-evidence",
        vec!["verify".into(), "--baseline".into(), "unused".into()],
    )
    .unwrap();
    assert!(
        options
            .only(&["--manifest", "--repository", "--report"])
            .is_err()
    );
}
