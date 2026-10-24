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
