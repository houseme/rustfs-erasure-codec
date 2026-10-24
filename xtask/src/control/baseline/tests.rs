use super::*;
use crate::artifact::snapshot::tests::Directory;
#[test]
fn cached_archive_checks_original_bytes_not_a_later_member_read() {
    let directory = Directory::new();
    let file = directory.0.join("member.json");
    fs::write(&file, b"{\"value\":1}").unwrap();
    let bytes = BTreeMap::from([("prefix/member.json".into(), fs::read(&file).unwrap())]);
    let archive = Archive {
        root: &directory.0,
        cached: Some((&bytes, "prefix")),
    };
    fs::write(&file, b"{\"value\":2}").unwrap();
    let sha = digest(&bytes["prefix/member.json"]);
    assert_eq!(archive.json("member.json", Some(&sha)).unwrap()["value"], 1);
    assert!(archive.json("member.json", Some(&"0".repeat(64))).is_err());
}
fn digest(raw: &[u8]) -> String {
    crate::artifact::snapshot::digest(raw)
}
#[test]
fn resource_completion_requires_strict_counts_and_all_restore_flags() {
    let status = json!({"status":"collected-diagnostic-only","error":null,"release_approval":false,"isolation_checks":1});
    let cleanup = json!({"child_removed":true,"controller_restored":true,"parent_affinities_restored":true,"error":null});
    resources(&status, &cleanup).unwrap();
    for key in [
        "child_removed",
        "controller_restored",
        "parent_affinities_restored",
    ] {
        let mut invalid = cleanup.clone();
        invalid[key] = false.into();
        assert!(resources(&status, &invalid).is_err());
    }
    for value in [json!(0), json!(true), json!(1.0)] {
        let mut invalid = status.clone();
        invalid["isolation_checks"] = value;
        assert!(resources(&invalid, &cleanup).is_err());
    }
}
#[test]
fn json_identity_is_strict_for_predeclared_plans() {
    assert!(same(&json!({"a":[1,2]}), &json!({"a":[1,2]})));
    assert!(!same(&json!({"a":1}), &json!({"a":1.0})));
    assert!(!same(&json!({"a":false}), &json!({"a":0})));
}
