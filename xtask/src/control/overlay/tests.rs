use super::*;
use crate::artifact::snapshot::tests::Directory;
use std::sync::atomic::AtomicBool;
fn files() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("root.rs".into(), "entry\n".into()),
        ("worker.rs".into(), "worker\n".into()),
    ])
}
#[test]
fn generated_overlay_restores_owned_files_after_normal_and_error_paths() {
    let d = Directory::new();
    fs::create_dir(d.0.join("tests")).unwrap();
    let flag = AtomicBool::new(false);
    let context = Context::new(&flag);
    {
        let _owned = Overlay::generated(&context, &d.0, files()).unwrap();
        assert!(d.0.join("tests/rse_release_sampling.rs").is_file());
    }
    assert!(!io::present(&d.0.join("tests/rse_release_sampling.rs")));
    assert!(!io::present(&d.0.join("tests/rse_release_sampling")));
}
#[test]
fn edited_generated_input_preserves_the_entire_transaction() {
    let d = Directory::new();
    fs::create_dir(d.0.join("tests")).unwrap();
    let flag = AtomicBool::new(false);
    let context = Context::new(&flag);
    let owned = Overlay::generated(&context, &d.0, files()).unwrap();
    fs::write(d.0.join("tests/rse_release_sampling/worker.rs"), "edited").unwrap();
    assert!(owned.finish().is_err());
    assert!(d.0.join("tests/rse_release_sampling.rs").is_file());
    assert_eq!(
        fs::read_to_string(d.0.join("tests/rse_release_sampling/worker.rs")).unwrap(),
        "edited"
    );
}
#[test]
fn extra_files_and_unreaped_workers_prevent_cleanup() {
    for unreaped in [false, true] {
        let d = Directory::new();
        fs::create_dir(d.0.join("tests")).unwrap();
        let flag = AtomicBool::new(false);
        let context = Context::new(&flag);
        let owned = Overlay::generated(&context, &d.0, files()).unwrap();
        if unreaped {
            context.unreaped.set(true);
        } else {
            fs::write(
                d.0.join("tests/rse_release_sampling/foreign.rs"),
                "preserve",
            )
            .unwrap();
        }
        assert!(owned.finish().is_err());
        assert!(d.0.join("tests/rse_release_sampling.rs").is_file());
    }
}
#[test]
fn source_overlay_restores_exact_original_newlines() {
    let d = Directory::new();
    fs::create_dir(d.0.join("src")).unwrap();
    fs::write(d.0.join("src/lib.rs"), b"original\r\n").unwrap();
    let flag = AtomicBool::new(false);
    let context = Context::new(&flag);
    let owned = Overlay::source(
        &context,
        &d.0,
        BTreeMap::from([("src/lib.rs".into(), "original\n".into())]),
        BTreeMap::from([
            ("src/lib.rs".into(), "changed\n".into()),
            ("src/observer.rs".into(), "observer\n".into()),
        ]),
    )
    .unwrap();
    owned.finish().unwrap();
    assert_eq!(fs::read(d.0.join("src/lib.rs")).unwrap(), b"original\r\n");
    assert!(!io::present(&d.0.join("src/observer.rs")));
}
#[test]
fn changed_source_is_refused_before_mutating_any_input() {
    let d = Directory::new();
    fs::create_dir(d.0.join("src")).unwrap();
    fs::write(d.0.join("src/lib.rs"), "actual\n").unwrap();
    let flag = AtomicBool::new(false);
    let context = Context::new(&flag);
    assert!(
        Overlay::source(
            &context,
            &d.0,
            BTreeMap::from([("src/lib.rs".into(), "stale\n".into())]),
            BTreeMap::from([("src/lib.rs".into(), "changed\n".into())])
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(d.0.join("src/lib.rs")).unwrap(),
        "actual\n"
    );
}
#[cfg(unix)]
#[test]
fn symlinked_generated_inputs_and_parent_directories_are_never_followed() {
    use std::os::unix::fs::symlink;
    let d = Directory::new();
    let target = Directory::new();
    symlink(&target.0, d.0.join("tests")).unwrap();
    let flag = AtomicBool::new(false);
    let context = Context::new(&flag);
    assert!(Overlay::generated(&context, &d.0, files()).is_err());
    assert!(fs::read_dir(&target.0).unwrap().next().is_none());
    fs::remove_file(d.0.join("tests")).unwrap();
    fs::create_dir(d.0.join("tests")).unwrap();
    symlink(
        d.0.join("absent"),
        d.0.join("tests/rse_release_sampling.rs"),
    )
    .unwrap();
    assert!(Overlay::generated(&context, &d.0, files()).is_err());
}

#[test]
fn unexpected_overlay_inputs_survive_the_enclosing_private_source_owner() {
    for explicit in [false, true] {
        let flag = AtomicBool::new(false);
        let context = Context::new(&flag);
        let private = io::Temporary::new(&context, "overlay-retention-test").unwrap();
        let cleanup = Directory(private.path.clone());
        fs::create_dir(private.path.join("tests")).unwrap();
        let owned = Overlay::generated(&context, &private.path, files()).unwrap();
        let edited = private.path.join("tests/rse_release_sampling/worker.rs");
        fs::write(&edited, b"unexpected original bytes").unwrap();
        if explicit {
            assert!(owned.finish().is_err());
        } else {
            drop(owned);
        }
        drop(private);
        assert_eq!(fs::read(&edited).unwrap(), b"unexpected original bytes");
        assert!(cleanup.0.join("tests/rse_release_sampling.rs").is_file());
    }
}

#[test]
fn a_restored_overlay_allows_the_enclosing_private_source_to_be_removed() {
    let flag = AtomicBool::new(false);
    let context = Context::new(&flag);
    let private = io::Temporary::new(&context, "overlay-cleanup-test").unwrap();
    let path = private.path.clone();
    fs::create_dir(path.join("tests")).unwrap();
    Overlay::generated(&context, &path, files())
        .unwrap()
        .finish()
        .unwrap();
    assert!(!context.preserve_inputs.get());
    drop(private);
    assert!(!path.exists());
}
