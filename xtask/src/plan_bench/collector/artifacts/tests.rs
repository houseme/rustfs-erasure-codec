use super::*;
use crate::artifact::snapshot::tests::Directory;
fn member(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
#[test]
fn budget_scan_counts_all_members_but_only_retains_sorted_new_metadata() {
    let dir = Directory::new();
    for name in [
        "b/new/benchmark.json",
        "b/new/sample.json",
        "b/new/estimates.json",
        "b/base/benchmark.json",
        "a/new/benchmark.json",
        "ignored.txt",
    ] {
        member(&dir.0, name, b"x");
    }
    let result = check(&dir.0, 11).unwrap();
    assert_eq!((result.entries, result.files, result.bytes), (11, 6, 6));
    assert_eq!(
        metadata(&dir.0, 11).unwrap(),
        ["a/new/benchmark.json", "b/new/benchmark.json"]
    );
    assert!(check(&dir.0, 10).is_err());
    assert!(metadata(&dir.0, 10).is_err());
}
#[test]
fn member_total_and_depth_budgets_include_ignored_sidecars() {
    let dir = Directory::new();
    member(&dir.0, "ignored/a", b"123");
    member(&dir.0, "ignored/b", b"456");
    let limits = Limits {
        member: 3,
        total: 6,
        depth: 1,
    };
    let result = scan(&dir.0, 3, limits, |_| {}).unwrap();
    assert_eq!(result.bytes, 6);
    for limits in [
        Limits {
            member: 2,
            ..limits
        },
        Limits { total: 5, ..limits },
        Limits { depth: 0, ..limits },
    ] {
        assert!(scan(&dir.0, 3, limits, |_| {}).is_err());
    }
}
#[test]
fn growing_sidecars_do_not_grow_the_retained_metadata_catalog() {
    let dir = Directory::new();
    member(&dir.0, "case/new/benchmark.json", b"metadata");
    for index in 0..512 {
        member(&dir.0, &format!("case/base/extra-{index}.json"), b"sidecar");
    }
    assert_eq!(check(&dir.0, 1024).unwrap().files, 513);
    assert_eq!(metadata(&dir.0, 1024).unwrap(), ["case/new/benchmark.json"]);
}
#[cfg(unix)]
#[test]
fn symlink_roots_and_entries_are_rejected_without_following_the_link() {
    use std::os::unix::fs::symlink;
    let dir = Directory::new();
    let target = Directory::new();
    member(&target.0, "case/new/benchmark.json", b"metadata");
    symlink(&target.0, dir.0.join("root-link")).unwrap();
    assert!(check(&dir.0.join("root-link"), 100).is_err());
    assert!(metadata(&dir.0.join("root-link"), 100).is_err());
    assert!(check(&dir.0, 100).is_err());
}
#[cfg(target_os = "linux")]
#[test]
fn ignored_non_utf8_sidecars_are_still_refused() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let dir = Directory::new();
    fs::write(dir.0.join(OsString::from_vec(vec![0xff])), b"ignored").unwrap();
    assert!(check(&dir.0, 100).is_err());
    assert!(metadata(&dir.0, 100).is_err());
}
