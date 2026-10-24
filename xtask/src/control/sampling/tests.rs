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

fn measured(profile: &str) -> Vec<Value> {
    let mut records = sampling_contract::calibration_profile::tests::records(profile);
    for record in &mut records {
        record["kind"] = if record["index"].as_u64().unwrap() < 3 {
            "warmup"
        } else {
            "sample"
        }
        .into();
        record["calibration_batch"] = 0.into();
        record["rows"][0]["origin"] = record["index"].clone();
    }
    records
}

#[test]
fn borrowed_selection_matches_frozen_wire_results_for_every_profile_without_changing_originals() {
    let revision = "a".repeat(40);
    let cpus = json!([0, 1, 2, 3, 4, 5, 6, 7]);
    for profile in sampling_contract::PROFILES {
        let records = measured(profile);
        let before = records.clone();
        let counts = serde_json::to_value(
            catalog(profile)
                .unwrap()
                .into_iter()
                .map(|case| (case, 1_u64))
                .collect::<BTreeMap<_, _>>(),
        )
        .unwrap();
        let wire = sampling_model("batch", json!({"request":{"profile":profile,"revision":revision,"records":records,"expected":catalog(profile).unwrap(),"counts":counts,"allowed_cpus":cpus,"expected_page_size":4096,"output":{"kind":"select","metric":crate::release_rows::metric(profile).unwrap(),"diagnostic":false}}})).unwrap();
        let indices: Vec<usize> = serde_json::from_value(wire["selected_indices"].clone()).unwrap();
        let expected = json!({"selected":indices.into_iter().map(|i|records[i]["rows"][0].clone()).collect::<Vec<_>>(),"spreads":wire["spreads"],"preparation":wire["preparation"],"worker_starts":wire["worker_starts"]});
        assert_eq!(
            select(
                &records,
                profile,
                &revision,
                &counts,
                &cpus,
                &json!(4096),
                false
            )
            .unwrap(),
            expected
        );
        assert_eq!(records, before);
        assert!(
            select(
                &records[..records.len() - 1],
                profile,
                &revision,
                &counts,
                &cpus,
                &json!(4096),
                false
            )
            .is_err()
        );
    }
}

#[test]
fn borrowed_selection_preserves_exact_integer_medians_and_diagnostic_short_samples() {
    let profile = "small_file";
    let revision = "a".repeat(40);
    let mut records = measured(profile);
    let counts = serde_json::to_value(
        catalog(profile)
            .unwrap()
            .into_iter()
            .map(|case| (case, 1_u64))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    for record in &mut records {
        let metric = if record["index"] == 3 {
            json!(9007199254740992.0)
        } else {
            json!(9007199254740993_u64)
        };
        record["rows"][0]["ns_per_iter"] = metric.clone();
        record["timed_ns"] = metric;
        record["wall_ns"] = json!(9007199254741993_u64);
    }
    let selected = select(
        &records,
        profile,
        &revision,
        &counts,
        &Value::Null,
        &json!(4096),
        false,
    )
    .unwrap();
    for row in selected["selected"].as_array().unwrap() {
        assert_eq!(row["ns_per_iter"].as_u64(), Some(9007199254740993));
        assert_eq!(row["origin"], 7);
    }
    for record in &mut records {
        record["wall_ns"] = 1.into();
    }
    assert!(
        select(
            &records,
            profile,
            &revision,
            &counts,
            &Value::Null,
            &json!(4096),
            false
        )
        .is_err()
    );
    let report = select(
        &records,
        profile,
        &revision,
        &counts,
        &Value::Null,
        &json!(4096),
        true,
    )
    .unwrap();
    for spread in report["spreads"].as_array().unwrap() {
        assert_eq!(spread["short_samples"].as_array().unwrap().len(), 9);
    }
}
