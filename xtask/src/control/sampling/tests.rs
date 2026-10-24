use super::*;

#[test]
fn complete_calibration_profiles_produce_the_frozen_case_map_without_changing_originals() {
    let context =
        json!({"runtime":{"page_size":4096},"isolation":{"cpu_affinity":[0,1,2,3,4,5,6,7]}});
    for profile in sampling_contract::PROFILES {
        let records = sampling_contract::calibration_profile::tests::records(profile);
        let original = records.clone();
        let result = calibration(&records, profile, &"a".repeat(40), &context).unwrap();
        let expected: BTreeMap<_, _> = catalog(profile)
            .unwrap()
            .into_iter()
            .map(|case| (case, 1_u64))
            .collect();
        assert_eq!(result["counts"], serde_json::to_value(expected).unwrap());
        assert_eq!(records, original);
        assert!(
            result["worker_starts"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        );
        assert!(
            calibration(
                &records[..records.len() - 1],
                profile,
                &"a".repeat(40),
                &context
            )
            .is_err()
        );
    }
}

#[test]
fn frozen_wire_results_keep_their_legacy_shape_and_only_counts_are_adapted() {
    let profile = "small_file";
    let records = sampling_contract::calibration_profile::tests::records(profile);
    let wire = sampling_model(
        "calibration-profile",
        json!({"profile":profile,"records":records,"revision":"a".repeat(40),"expected_page_size":4096,"allowed_cpus":null}),
    ).unwrap();
    assert!(wire["counts"].is_array());
    let mut expected = wire.clone();
    let mut adapted = calibrated_profile(wire).unwrap();
    let expected_counts: BTreeMap<_, _> = catalog(profile)
        .unwrap()
        .into_iter()
        .map(|case| (case, 1_u64))
        .collect();
    assert_eq!(
        adapted["counts"],
        serde_json::to_value(expected_counts).unwrap()
    );
    adapted.as_object_mut().unwrap().remove("counts");
    expected.as_object_mut().unwrap().remove("counts");
    assert_eq!(adapted, expected);
}

#[test]
fn frozen_calibration_counts_reject_missing_duplicates_and_non_bounded_integers() {
    for value in [
        Value::Null,
        json!("bad"),
        json!({}),
        json!({"counts":[]}),
        json!({"counts":[["case",1],["case",2]]}),
    ] {
        assert!(calibrated_profile(value).is_err());
    }
    for count in [json!(0), json!(1_000_000_001_u64), json!(true), json!(1.0)] {
        assert!(calibrated_profile(json!({"counts":[["case",count]]})).is_err());
    }
}
