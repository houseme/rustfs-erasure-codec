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

#[test]
fn scoped_original_budget_refuses_oversized_input_and_restores_the_outer_limit() {
    let directory = Directory::new();
    fs::write(directory.0.join("large"), b"123456").unwrap();
    fs::write(directory.0.join("small"), b"ok").unwrap();
    let mut snapshot = Snapshot::with_budget(&directory.0, 32).unwrap();
    assert!(snapshot.scoped(5, |s| s.original("large")).is_err());
    assert_eq!(snapshot.original("small").unwrap(), b"ok");
    assert_eq!(snapshot.original("large").unwrap(), b"123456");
    assert_eq!(snapshot.finish().unwrap().len(), 2);
}

#[test]
fn streaming_revalidation_matches_original_digests_across_chunk_boundaries() {
    let dir = Directory::new();
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    for size in [
        0,
        1,
        VERIFY_BUFFER_LEN - 1,
        VERIFY_BUFFER_LEN,
        VERIFY_BUFFER_LEN + 1,
        2 * VERIFY_BUFFER_LEN + 3,
    ] {
        let bytes: Vec<_> = (0..size).map(|i| (i % 251) as u8).collect();
        let name = format!("chunk-{size}");
        fs::write(dir.0.join(&name), &bytes).unwrap();
        assert_eq!(snapshot.raw_bound(&name, &digest(&bytes)).unwrap(), bytes);
    }
    snapshot.unchanged().unwrap();
    assert_eq!(snapshot.finish().unwrap().len(), 6);
}
#[test]
fn streaming_reader_retries_interrupts_and_rejects_errors_and_growth() {
    struct Interrupt {
        inner: std::io::Cursor<Vec<u8>>,
        first: bool,
    }
    impl Read for Interrupt {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.first {
                self.first = false;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            self.inner.read(buf)
        }
    }
    let mut buffer = [0_u8; 3];
    let source = b"bounded original";
    let reader = Interrupt {
        inner: std::io::Cursor::new(source.to_vec()),
        first: true,
    };
    assert_eq!(
        digest_reader(reader, source.len() as u64, &mut buffer).unwrap(),
        digest(source)
    );
    assert!(
        digest_reader(
            std::io::Cursor::new(source),
            source.len() as u64 - 1,
            &mut buffer
        )
        .is_err()
    );
    struct Failed;
    impl Read for Failed {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::Other.into())
        }
    }
    assert!(digest_reader(Failed, 10, &mut buffer).is_err());
    assert!(digest_reader(std::io::Cursor::new(source), 100, &mut []).is_err());
}
#[test]
fn streaming_revalidation_preserves_empty_and_overbudget_mutation_refusals() {
    let dir = Directory::new();
    fs::write(dir.0.join("original"), b"original").unwrap();
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    snapshot.original("original").unwrap();
    fs::write(dir.0.join("original"), b"").unwrap();
    assert!(snapshot.unchanged().is_err());
    fs::File::create(dir.0.join("original"))
        .unwrap()
        .set_len(MEMBER_LIMIT + 1)
        .unwrap();
    assert!(snapshot.unchanged().is_err());
    fs::write(dir.0.join("empty"), b"").unwrap();
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    snapshot.raw_bound("empty", &digest(b"")).unwrap();
    snapshot.unchanged().unwrap();
    fs::write(dir.0.join("empty"), b"late").unwrap();
    assert!(snapshot.finish().is_err());
}
#[test]
fn component_iteration_keeps_path_and_leaf_type_boundaries() {
    let dir = Directory::new();
    fs::create_dir(dir.0.join("nested")).unwrap();
    fs::write(dir.0.join("nested/leaf"), b"original").unwrap();
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    assert_eq!(snapshot.original("nested/leaf").unwrap(), b"original");
    for name in [
        "",
        ".",
        "..",
        "../outside",
        "/absolute",
        "nested/../leaf",
        "nested",
        "nested/leaf/child",
    ] {
        assert!(snapshot.original(name).is_err(), "{name}");
    }
    assert_eq!(snapshot.finish().unwrap().len(), 1);
}

#[test]
fn digest_only_binding_matches_owned_reads_and_reuses_lazy_scratch() {
    let dir = Directory::new();
    let bytes = vec![7_u8; VERIFY_BUFFER_LEN + 3];
    fs::write(dir.0.join("opaque"), &bytes).unwrap();
    fs::write(dir.0.join("next"), b"next").unwrap();
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    assert!(snapshot.verification_buffer.is_empty());
    snapshot.verify_binary("opaque", &digest(&bytes)).unwrap();
    assert_eq!(snapshot.total, bytes.len() as u64);
    let buffer = snapshot.verification_buffer.as_ptr();
    assert_eq!(snapshot.verification_buffer.len(), VERIFY_BUFFER_LEN);
    snapshot.verify_raw("next", &digest(b"next")).unwrap();
    snapshot.unchanged().unwrap();
    assert_eq!(snapshot.verification_buffer.as_ptr(), buffer);
    assert_eq!(snapshot.finish().unwrap().len(), 2);
}
#[test]
fn digest_only_binding_preserves_empty_duplicate_pin_and_budget_refusals() {
    let dir = Directory::new();
    fs::write(dir.0.join("empty"), b"").unwrap();
    fs::write(dir.0.join("data"), b"original").unwrap();
    assert!(
        Snapshot::new(&dir.0)
            .unwrap()
            .verify_binary("empty", &digest(b""))
            .is_err()
    );
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    snapshot.verify_raw("empty", &digest(b"")).unwrap();
    assert!(snapshot.verify_raw("empty", &digest(b"")).is_err());
    let mut snapshot = Snapshot::with_budget(&dir.0, 7).unwrap();
    assert!(
        snapshot
            .verify_binary("data", &digest(b"original"))
            .is_err()
    );
    assert_eq!(snapshot.total, 8);
    assert!(snapshot.member_digest("data").is_none());
    let mut snapshot = Snapshot::new(&dir.0).unwrap();
    assert!(snapshot.verify_binary("data", &"0".repeat(64)).is_err());
    assert_eq!(snapshot.total, 8);
    assert!(snapshot.member_digest("data").is_none());
}
#[test]
fn mixed_owned_and_digest_only_members_share_scope_and_final_mutation_checks() {
    let dir = Directory::new();
    fs::write(dir.0.join("json"), b"{}").unwrap();
    fs::write(dir.0.join("raw"), b"raw").unwrap();
    fs::write(dir.0.join("tail"), b"x").unwrap();
    let mut snapshot = Snapshot::with_budget(&dir.0, 10).unwrap();
    snapshot.json("json").unwrap();
    snapshot.verify_binary("raw", &digest(b"raw")).unwrap();
    assert!(
        snapshot
            .scoped(0, |s| s.verify_raw("tail", &digest(b"x")))
            .is_err()
    );
    assert_eq!(snapshot.total, 6);
    snapshot.verify_raw("tail", &digest(b"x")).unwrap();
    assert_eq!(snapshot.total, 7);
    fs::write(dir.0.join("raw"), b"changed").unwrap();
    assert!(snapshot.finish().is_err());
}
