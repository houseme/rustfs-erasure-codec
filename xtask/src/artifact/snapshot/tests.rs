use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
pub(crate) struct Directory(pub PathBuf);
impl Directory {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "rse-snapshot-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn hashes_cover_original_encoded_bytes_for_all_legacy_json_encodings() {
    let directory = Directory::new();
    let text = "{\"name\":\"雪😀\",\"count\":7}";
    let expected: Value = serde_json::from_str(text).unwrap();
    let mut versions = vec![
        text.as_bytes().to_vec(),
        [&[239, 187, 191], text.as_bytes()].concat(),
    ];
    for little in [false, true] {
        let u16bytes: Vec<_> = text
            .encode_utf16()
            .flat_map(|v| {
                if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                }
            })
            .collect();
        let u32bytes: Vec<_> = text
            .chars()
            .flat_map(|v| {
                if little {
                    (v as u32).to_le_bytes()
                } else {
                    (v as u32).to_be_bytes()
                }
            })
            .collect();
        versions.push(u16bytes.clone());
        versions.push(u32bytes.clone());
        versions.push(
            [
                if little {
                    &[255, 254][..]
                } else {
                    &[254, 255][..]
                },
                &u16bytes,
            ]
            .concat(),
        );
        versions.push(
            [
                if little {
                    &[255, 254, 0, 0][..]
                } else {
                    &[0, 0, 254, 255][..]
                },
                &u32bytes,
            ]
            .concat(),
        );
    }
    for (index, bytes) in versions.iter().enumerate() {
        let name = format!("{index}.json");
        fs::write(directory.0.join(&name), bytes).unwrap();
        let mut snapshot = Snapshot::new(&directory.0).unwrap();
        assert_eq!(
            snapshot.json_bound(&name, Some(&digest(bytes))).unwrap(),
            expected
        );
        assert_eq!(snapshot.finish().unwrap()[&name], digest(bytes));
    }
}
#[test]
fn malformed_duplicate_nonfinite_and_incomplete_unicode_members_are_refused() {
    let directory = Directory::new();
    for bytes in [
        b"{\"a\":1,\"a\":2}".as_slice(),
        b"{\"a\":NaN}",
        b"{\"a\":1e999}",
        b"[] trailing",
        &[255, 254, 123],
        &[0, 0, 254, 255, 0, 0, 216, 0],
    ] {
        fs::write(directory.0.join("bad.json"), bytes).unwrap();
        assert!(
            Snapshot::new(&directory.0)
                .unwrap()
                .json("bad.json")
                .is_err()
        );
    }
}
#[test]
fn changed_replaced_empty_and_oversized_members_cannot_complete_a_snapshot() {
    let directory = Directory::new();
    let path = directory.0.join("input.json");
    fs::write(&path, b"{}").unwrap();
    let mut snapshot = Snapshot::new(&directory.0).unwrap();
    snapshot.json("input.json").unwrap();
    fs::write(&path, b"[]").unwrap();
    assert!(snapshot.finish().is_err());
    fs::write(&path, b"").unwrap();
    assert!(
        Snapshot::new(&directory.0)
            .unwrap()
            .json("input.json")
            .is_err()
    );
    fs::File::create(&path)
        .unwrap()
        .set_len(MEMBER_LIMIT + 1)
        .unwrap();
    assert!(
        Snapshot::new(&directory.0)
            .unwrap()
            .json("input.json")
            .is_err()
    );
}
#[test]
fn snapshot_budget_and_member_path_rules_apply_before_a_success_report() {
    let directory = Directory::new();
    fs::write(directory.0.join("input.json"), b"{}").unwrap();
    for name in ["../input.json", "/input.json", ""] {
        assert!(Snapshot::new(&directory.0).unwrap().json(name).is_err());
    }
    let mut snapshot = Snapshot::new(&directory.0).unwrap();
    snapshot.total = SNAPSHOT_LIMIT - 1;
    assert!(snapshot.json("input.json").is_err());
    let mut snapshot = Snapshot::new(&directory.0).unwrap();
    snapshot.json("input.json").unwrap();
    assert!(snapshot.json("input.json").is_err());
}
#[cfg(unix)]
#[test]
fn linked_members_and_directories_are_refused_without_following_the_link() {
    let directory = Directory::new();
    fs::write(directory.0.join("original.json"), b"{}").unwrap();
    std::os::unix::fs::symlink("original.json", directory.0.join("linked.json")).unwrap();
    assert!(
        Snapshot::new(&directory.0)
            .unwrap()
            .json("linked.json")
            .is_err()
    );
    fs::create_dir(directory.0.join("original")).unwrap();
    fs::write(directory.0.join("original/input.json"), b"{}").unwrap();
    std::os::unix::fs::symlink("original", directory.0.join("linked")).unwrap();
    assert!(
        Snapshot::new(&directory.0)
            .unwrap()
            .json("linked/input.json")
            .is_err()
    );
}

#[test]
fn bound_digest_refuses_changed_bytes_before_recording_or_decoding_a_member() {
    let directory = Directory::new();
    fs::write(directory.0.join("input.json"), b"{}").unwrap();
    let mut snapshot = Snapshot::new(&directory.0).unwrap();
    assert!(
        snapshot
            .json_bound("input.json", Some(&digest(b"[]")))
            .is_err()
    );
    assert!(snapshot.members.is_empty());
    assert_eq!(
        snapshot
            .json_bound("input.json", Some(&digest(b"{}")))
            .unwrap(),
        serde_json::json!({})
    );
}

#[test]
fn fixed_catalog_budgets_preserve_inclusive_member_limits() {
    let directory = Directory::new();
    for name in ["a.json", "b.json"] {
        fs::write(directory.0.join(name), b"{}").unwrap();
    }
    let mut snapshot = Snapshot::with_budget(&directory.0, 4).unwrap();
    snapshot.json("a.json").unwrap();
    snapshot.json("b.json").unwrap();
    snapshot.finish().unwrap();
    let mut snapshot = Snapshot::with_budget(&directory.0, 3).unwrap();
    snapshot.json("a.json").unwrap();
    assert!(
        snapshot
            .json("b.json")
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
}
