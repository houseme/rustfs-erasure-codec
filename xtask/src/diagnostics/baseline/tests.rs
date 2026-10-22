use super::*;
fn rows() -> Vec<Value> {
    (0..12).map(|i|json!({"index":i,"kind":if i<3 {"warmup"} else {"sample"},"iterations":100,"wall_ns":2000,"timed_ns":1900,"rows":[{"ns_per_iter":100.0}],"fixture":null,
        "before":{"cpu":0,"run_ns":100,"wait_ns":10,"switches":0,"process_minor_faults":0,"process_major_faults":0},
        "after":{"cpu":0,"run_ns":1100,"wait_ns":20,"switches":1,"process_minor_faults":2,"process_major_faults":0}})).collect()
}
#[test]
fn service_ranks_are_independent_and_ratios_are_never_clamped() {
    let mut rows = rows();
    for (i, row) in rows.iter_mut().enumerate() {
        if i == 3 || i == 4 {
            row["rows"][0]["ns_per_iter"] = json!(120.0);
        }
        if i == 10 {
            row["after"]["run_ns"] = json!(3100);
        }
    }
    let detail = describe(&rows, "ns_per_iter", None).unwrap();
    assert_eq!(detail["central_spread"], 0.2);
    assert_eq!(detail["measured_service"]["main_run_central_spread"], 0.0);
    assert_eq!(
        detail["measured_service"]["main_run_to_wall_range"],
        json!([0.5, 1.5])
    );
    assert_eq!(detail["trace"].as_array().unwrap().len(), 12);
    assert_eq!(detail["measured_service"]["total_process_minor_faults"], 18);
    assert!(detail["caller_matches_calibration"].is_null());
}
#[test]
fn zero_service_time_has_unknown_spread_and_signed_timer_gaps_remain_visible() {
    let mut rows = rows();
    for r in &mut rows {
        r["after"]["run_ns"] = r["before"]["run_ns"].clone();
        r["timed_ns"] = json!(2001);
    }
    let detail = describe(&rows, "ns_per_iter", None).unwrap();
    assert!(detail["measured_service"]["main_run_central_spread"].is_null());
    assert_eq!(detail["trace"][3]["wall_minus_timed_ns"], -1);
    assert_eq!(
        detail["measured_service"]["main_run_to_wall_range"],
        json!([0.0, 0.0])
    );
}
fn group(phase: &str, median: u64, cpus: Value) -> Group {
    let mut detail = describe(&rows(), "ns_per_iter", None).unwrap();
    detail["median"] = json!(median);
    detail["measured_endpoint_cpus"] = cpus;
    Group {
        profile: "small_file".into(),
        case: "encode:4:2:1024".into(),
        phase: phase.into(),
        detail,
        window: None,
    }
}
#[test]
fn repeat_categories_and_exact_boundary_retain_the_complete_cohort() {
    for (cpus, category) in [
        (json!([0]), "single_endpoint_cpu_same"),
        (json!([1]), "single_endpoint_cpu_changed"),
        (json!([0, 1]), "multiple_endpoint_cpus"),
    ] {
        let output = report(
            &PHASES.map(String::from),
            vec![group("A1", 2000, json!([0])), group("A2", 2100, cpus)],
            None,
        )
        .unwrap();
        assert_eq!(output["cases"][0]["repeat"]["absolute_metric_drift"], 0.05);
        assert_eq!(
            output["repeat_endpoint_counts"][category],
            json!({"cases":1,"repeat_failures":0})
        );
    }
    let output = report(&["A1".into()], vec![group("A1", 2000, json!([0]))], None).unwrap();
    assert!(output["cases"][0]["repeat"].is_null());
}
#[test]
fn missing_duplicate_phases_and_frozen_statistic_disagreement_are_refused() {
    assert!(
        report(
            &PHASES.map(String::from),
            vec![group("A1", 2000, json!([0]))],
            None
        )
        .is_err()
    );
    assert!(
        report(
            &["A1".into()],
            vec![group("A1", 2000, json!([0])), group("A1", 2000, json!([0]))],
            None
        )
        .is_err()
    );
    assert!(
        report(
            &PHASES.map(String::from),
            vec![group("A1", 2000, json!([0])), group("A2", 2400, json!([0]))],
            Some(Frozen {
                drift_failures: 0,
                spread_failures: 0
            })
        )
        .is_err()
    );
}
#[test]
fn adjacent_bounds_cover_extreme_placements_and_validate_actual_worker_budgets() {
    let first = budget(
        Interval {
            start_ns: 0,
            end_ns: 100,
        },
        &[json!({"kind":"sample","wall_ns":80})],
    )
    .unwrap();
    let second = budget(
        Interval {
            start_ns: 105,
            end_ns: 225,
        },
        &[
            json!({"kind":"warmup","wall_ns":20}),
            json!({"kind":"sample","wall_ns":70}),
        ],
    )
    .unwrap();
    let report = window(&first, &second).unwrap();
    assert_eq!(report["first_observation_gap_upper_ns"], 55);
    assert_eq!(report["first_sample_gap_upper_ns"], 75);
    assert_eq!(report["pair_envelope_ns"], 225);
    let observation = report["first_observation_gap_upper_ns"].as_u64().unwrap();
    let sample = report["first_sample_gap_upper_ns"].as_u64().unwrap();
    for tail in 0..=20 {
        for head in 0..=30 {
            assert!(tail + 5 + head <= observation);
            assert!(tail + 5 + head + 20 <= sample);
        }
    }
    assert!(
        budget(
            Interval {
                start_ns: 0,
                end_ns: 79
            },
            &[json!({"kind":"sample","wall_ns":80})]
        )
        .is_err()
    );
    assert!(
        budget(
            Interval {
                start_ns: 0,
                end_ns: 100
            },
            &[json!({"kind":"sample","wall_ns":true})]
        )
        .is_err()
    );
    assert!(
        window(
            &first,
            &Budget {
                start_ns: 99,
                end_ns: 200,
                wall: 1,
                warmup: 0
            }
        )
        .is_err()
    );
}
#[test]
fn adjacent_plan_is_the_complete_original_catalog_and_preserves_profile_order() {
    let counts = sampling_contract::case_catalog()
        .into_iter()
        .map(|(p, c)| (p.into(), c.into_iter().map(|c| (c, 1)).collect()))
        .collect();
    let steps = Request::Schedule { counts }.execute().unwrap();
    assert_eq!(steps.as_array().unwrap().len(), 252);
    assert_eq!(steps[0]["profile"], "smoke");
    for pair in steps.as_array().unwrap().as_chunks::<2>().0 {
        assert_eq!(pair[0]["phase"], "A1");
        assert_eq!(pair[1]["phase"], "A2");
        assert_eq!(pair[0]["case"], pair[1]["case"]);
    }
}

#[test]
fn adjacent_counts_reject_boolean_fractional_unbounded_and_incomplete_values() {
    let counts = sampling_contract::case_catalog()
        .into_iter()
        .map(|(p, c)| (p, c.into_iter().map(|c| (c, 1)).collect::<BTreeMap<_, _>>()))
        .collect::<BTreeMap<_, _>>();
    let original = json!({"operation":"schedule","counts":counts});
    let key = counts["smoke"].keys().next().unwrap();
    for value in [
        json!(0),
        json!(true),
        json!(1.0),
        json!(-1),
        json!(1000000001_u64),
    ] {
        let mut request = original.clone();
        request["counts"]["smoke"][key] = value;
        assert!(
            serde_json::from_value::<Request>(request)
                .and_then(|r| r.execute().map_err(serde::de::Error::custom))
                .is_err()
        );
    }
    let mut request = original;
    request["counts"]["smoke"]
        .as_object_mut()
        .unwrap()
        .remove(key);
    assert!(
        serde_json::from_value::<Request>(request)
            .unwrap()
            .execute()
            .is_err()
    );
}
