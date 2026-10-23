use super::*;
fn request(diagnostic: bool) -> Request {
    let mut data = BTreeMap::new();
    let mut sample_spreads = Vec::new();
    for phase in STAGES {
        let mut profiles = BTreeMap::new();
        for profile in PROFILES {
            let metric = release_rows::metric(profile).unwrap();
            let mut rows = Vec::new();
            for case in &sampling_contract::case_catalog()[profile] {
                rows.push((case.clone(), json!({metric:100.0})));
                sample_spreads.push(json!({"phase":phase,"profile":profile,"case":case,"central_spread":0.0,"short_samples":[]}));
            }
            if phase == "B1" {
                rows.reverse();
            }
            profiles.insert(profile.to_owned(), rows);
        }
        data.insert(phase.to_owned(), profiles);
    }
    Request {
        data,
        sample_spreads,
        diagnostic,
    }
}
#[test]
fn complete_campaign_keeps_original_case_order_and_never_approves_a_diagnostic() {
    let req = request(true);
    let first = req.data["B1"]["smoke"][0].0.clone();
    let report = req.execute().unwrap();
    assert_eq!(report["repeat_checks"].as_array().unwrap().len(), 252);
    assert_eq!(
        report["unqualified_metric_comparisons"]
            .as_array()
            .unwrap()
            .len(),
        504
    );
    assert_eq!(report["repeat_checks"][15]["case"], first);
    assert_eq!(report["all_original_limits_passed"], true);
    assert_eq!(report["release_approval"], false);
    assert!(report["candidate_attribution"].is_null());
}
#[test]
fn exact_boundaries_and_all_simultaneous_failure_classes_are_preserved() {
    let mut req = request(true);
    req.data.get_mut("A2").unwrap().get_mut("smoke").unwrap()[0].1["throughput_mb_s"] =
        json!(105.0);
    req.sample_spreads[0]["central_spread"] = json!(0.1);
    let report = req.execute().unwrap();
    assert_eq!(report["repeat_failures"], json!([]));
    assert_eq!(report["distribution_failures"], json!([]));
    let mut req = request(true);
    req.data.get_mut("A2").unwrap().get_mut("smoke").unwrap()[0].1["throughput_mb_s"] =
        json!(106.0);
    req.sample_spreads[0]["central_spread"] = json!(0.11);
    req.sample_spreads[1]["short_samples"] = json!([{"index":3,"wall_ns":1,"timed_ns":2}]);
    let report = req.execute().unwrap();
    for key in ["repeat_failures", "distribution_failures", "short_samples"] {
        assert_eq!(report[key].as_array().unwrap().len(), 1);
    }
    assert_eq!(report["all_original_limits_passed"], false);
}
#[test]
fn finite_extremes_cannot_turn_repeat_or_candidate_quotients_into_infinity() {
    for repeat in [false, true] {
        let mut req = request(true);
        for phase in STAGES {
            req.data
                .get_mut(phase)
                .unwrap()
                .get_mut("small_file")
                .unwrap()[0]
                .1["ns_per_iter"] =
                json!(if phase.starts_with('A') && !(repeat && phase == "A2") {
                    1e-308
                } else {
                    1e308
                });
        }
        assert!(
            req.execute()
                .unwrap_err()
                .to_string()
                .contains("nonfinite campaign ratio")
        );
    }
}
#[test]
fn missing_duplicate_and_incomparable_rows_or_spreads_are_refused() {
    let mut req = request(false);
    req.data.remove("A2");
    assert!(req.execute().is_err());
    let mut req = request(false);
    req.data
        .get_mut("B1")
        .unwrap()
        .get_mut("stream")
        .unwrap()
        .pop();
    assert!(req.execute().is_err());
    let mut req = request(false);
    req.data
        .get_mut("B2")
        .unwrap()
        .get_mut("small_file")
        .unwrap()[0]
        .1["backend_id"] = json!("changed");
    assert!(req.execute().is_err());
    let mut req = request(true);
    req.sample_spreads.pop();
    assert!(req.execute().is_err());
    let mut req = request(true);
    req.sample_spreads[1] = req.sample_spreads[0].clone();
    assert!(req.execute().is_err());
}
