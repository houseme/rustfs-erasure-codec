use super::*;
use serde_json::json;

#[test]
fn calibration_counts_are_bounded_actual_integers_from_both_lengths() {
    let counts = json!({CASES[0]:3,CASES[1]:7});
    assert_eq!(iterations(&counts).unwrap(), 7);
    for wrong in [
        Value::Null,
        json!(true),
        json!(0),
        json!(1_000_000_001u64),
        json!(3.0),
    ] {
        let mut changed = counts.clone();
        changed[CASES[0]] = wrong;
        assert!(iterations(&changed).is_err());
    }
}

#[test]
fn complete_binding_preserves_legacy_context_equality_and_refuses_independent_mismatches() {
    let sources =
        json!({"baseline":{"commit":"a"},"candidate":{"commit":"b","tree":"c","lock_sha256":"d"}});
    let native = json!({"backend":"fixture","backend_id":"fixture","backend_kind":"fixture", "features":"simd-accel|std","target_arch":"x86_64","target_os":"linux"});
    let mut row = native.clone();
    row["target_triple"] = json!("x86_64-linux-gnu");
    let binding = json!({"sources":sources,"context":{"flag":false},"counts":{CASES[0]:3,CASES[1]:7},"rows":{CASES[0]:row,CASES[1]:row}});
    let layout = json!({"sources":sources,"tool":sources["candidate"],"context":{"flag":0},"iterations":7,"phase_reports":{"A1":{"provenance":native}}});
    let before = binding.clone();
    serde_json::from_value::<Binding>(binding.clone())
        .unwrap()
        .validate(&layout)
        .unwrap();
    assert_eq!(binding, before);
    for pointer in [
        "/sources/candidate/commit",
        "/context/flag",
        "/counts/verify_with_buffer:4:2:1024",
        "/rows/verify_with_buffer:4:2:4096/backend",
        "/rows/verify_with_buffer:4:2:1024/target_triple",
    ] {
        let mut changed = binding.clone();
        *changed.pointer_mut(pointer).unwrap() = json!("different");
        assert!(
            serde_json::from_value::<Binding>(changed)
                .unwrap()
                .validate(&layout)
                .is_err(),
            "{pointer}"
        );
    }
    for key in ["tree", "lock_sha256"] {
        let mut changed = layout.clone();
        changed["tool"][key] = json!("different");
        assert!(
            serde_json::from_value::<Binding>(binding.clone())
                .unwrap()
                .validate(&changed)
                .unwrap_err()
                .to_string()
                .contains("frozen candidate")
        );
    }
    let mut changed = layout.clone();
    changed["iterations"] = json!(8);
    assert!(
        serde_json::from_value::<Binding>(binding)
            .unwrap()
            .validate(&changed)
            .unwrap_err()
            .to_string()
            .contains("frozen full-campaign")
    );
}
