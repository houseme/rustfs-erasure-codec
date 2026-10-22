use super::*;
use serde_json::json;

fn grid(page: Option<u64>) -> Vec<Value> {
    let mut rows = Vec::new();
    for len in [1024, 4096] {
        for pass in 0..2 {
            for cell in 0..16 {
                let cell = if pass == 0 { cell } else { 15 - cell };
                let (a, b) = ((cell / 4) * 16, (cell % 4) * 16);
                for index in 0..12 {
                    let mut row = json!({"len":len,"pass":pass,"source_offset":a,"scratch_offset":b,"index":index,"iterations":1000000,
                "kind":if index<3 {"warmup"} else {"sample"},"wall_ns":2000000000_u64,"ns_per_iter":2000.0,
                "entry":"slice_views","fixture_lifetime":"same_backing_per_length","input_page_offsets":vec![a;6],"scratch_page_offsets":vec![b;2]});
                    if let Some(page) = page {
                        row.as_object_mut().unwrap().extend(json!({"page_size":page,"cpu":2,"backend":"scalar","backend_id":"Scalar","backend_kind":"Scalar",
                "features":"simd-accel|std","target_arch":"fixture","target_os":"linux","input_4096_offsets":vec![a;6],"scratch_4096_offsets":vec![b;2]}).as_object().unwrap().clone());
                    }
                    rows.push(row);
                }
            }
        }
    }
    rows
}
fn phases() -> BTreeMap<String, Vec<Value>> {
    ["A1", "B1", "B2", "A2"]
        .into_iter()
        .map(|p| (p.into(), grid(Some(4096))))
        .collect()
}
#[test]
fn complete_grid_preserves_order_samples_and_diagnostic_fence() {
    let data = grid(None);
    let original = data.clone();
    let report = verify::analyze(&data, 1000000, None, None).unwrap();
    assert_eq!(report["case_count"], 64);
    assert_eq!(report["formal_samples"], 576);
    assert_eq!(report["repeat_checks"].as_array().unwrap().len(), 32);
    assert_eq!(report["cases"][16]["source_offset"], 48);
    assert_eq!(report["cases"][16]["scratch_offset"], 48);
    assert_eq!(report["release_approval"], false);
    assert!(report["candidate_attribution"].is_null());
    assert_eq!(data, original);
}
#[test]
fn incomplete_reordered_untyped_and_unbound_grid_rows_are_refused() {
    let original = grid(Some(4096));
    for (key, value) in [
        ("index", json!(true)),
        ("kind", json!("sample")),
        ("entry", json!("Vec")),
        ("iterations", json!(1)),
        ("wall_ns", json!(0)),
        ("cpu", json!(3)),
        ("features", json!("std")),
        ("input_4096_offsets", json!(vec![1; 6])),
        ("input_page_offsets", json!(vec![4096; 6])),
    ] {
        let mut rows = original.clone();
        rows[0][key] = value;
        assert!(
            verify::analyze(&rows, 1000000, Some(4096), Some(2)).is_err(),
            "{key}"
        );
    }
    let mut rows = original;
    rows.pop();
    assert!(verify::analyze(&rows, 1000000, Some(4096), Some(2)).is_err());
}
#[test]
fn reverse_pass_must_reuse_the_same_original_layout() {
    let mut rows = grid(None);
    for row in &mut rows {
        if row["pass"] == 1 {
            for n in row["input_page_offsets"].as_array_mut().unwrap() {
                *n = json!(n.as_u64().unwrap() + 64);
            }
        }
    }
    assert!(
        verify::analyze(&rows, 1000000, None, None)
            .unwrap_err()
            .to_string()
            .contains("reuse")
    );
}
#[test]
fn every_large_page_position_and_provenance_field_is_checked() {
    for page in [4096, 16384, 65536] {
        let mut rows = grid(Some(page));
        verify::analyze(&rows, 1000000, Some(page), Some(2)).unwrap();
        rows[700]["backend"] = json!("foreign");
        assert!(verify::analyze(&rows, 1000000, Some(page), Some(2)).is_err());
    }
    assert!(verify::analyze(&grid(Some(4096)), 1000000, Some(4096), None).is_err());
}
#[test]
fn drift_and_regression_limits_are_inclusive_per_cell() {
    let mut phases = phases();
    for r in phases.get_mut("A2").unwrap() {
        r["wall_ns"] = json!(2100000000_u64);
        r["ns_per_iter"] = json!(2100.0);
    }
    for phase in ["B1", "B2"] {
        for r in phases.get_mut(phase).unwrap() {
            r["wall_ns"] = json!(2240000000_u64);
            r["ns_per_iter"] = json!(2240.0);
        }
    }
    let report = verify::paired(&phases, 1000000, 4096, 2).unwrap();
    assert_eq!(report["layout_gate_passed"], true);
    assert_eq!(
        report["candidate_attribution"].as_array().unwrap().len(),
        256
    );
    assert_eq!(report["repeat_checks"].as_array().unwrap().len(), 128);
    assert_eq!(report["release_approval"], false);
}
#[test]
fn one_regressing_cell_is_not_pooled_with_faster_cells() {
    let mut phases = phases();
    for phase in ["B1", "B2"] {
        for r in phases.get_mut(phase).unwrap() {
            if r["len"] == 1024 && r["source_offset"] == 16 && r["scratch_offset"] == 32 {
                r["wall_ns"] = json!(2260000000_u64);
                r["ns_per_iter"] = json!(2260.0);
            }
        }
    }
    let report = verify::paired(&phases, 1000000, 4096, 2).unwrap();
    assert_eq!(report["layout_quality_passed"], true);
    assert_eq!(report["layout_gate_passed"], false);
    assert!(report["candidate_attribution"].is_null());
    assert_eq!(report["regression_failures"].as_array().unwrap().len(), 8);
}
#[test]
fn short_wide_or_drifting_phases_withhold_all_attribution() {
    for failure in ["short", "wide", "repeat"] {
        let mut phases = phases();
        for row in phases.get_mut("B1").unwrap() {
            let factor = match failure {
                "short" => 0.1,
                "wide" if row["index"] == 3 || row["index"] == 4 => 0.8,
                "repeat" => 1.2,
                _ => 1.0,
            };
            row["ns_per_iter"] = json!(2000.0 * factor);
            row["wall_ns"] = json!((2000000000.0 * factor) as u64);
        }
        let report = verify::paired(&phases, 1000000, 4096, 2).unwrap();
        assert_eq!(report["layout_gate_passed"], false, "{failure}");
        assert!(report["candidate_attribution"].is_null());
    }
}
#[test]
fn integer_medians_and_ratios_keep_precision_above_f64_integer_range() {
    let a = json!(9007199254740993_u64);
    let b = json!(9007199254740992.0);
    let values = [
        a.clone(),
        b.clone(),
        a.clone(),
        a.clone(),
        a.clone(),
        a.clone(),
        a.clone(),
        a.clone(),
        a.clone(),
    ];
    let (median, spread) = numeric::center(values.iter()).unwrap();
    assert_eq!(median, a);
    assert_eq!(spread, 0.0);
    assert_eq!(
        numeric::ratio(&json!(1), &json!(3)).unwrap().to_bits(),
        (1.0_f64 / 3.0).to_bits()
    );
    assert_eq!(
        numeric::ratio(&json!(u64::MAX), &json!(1)).unwrap(),
        18446744073709551616.0
    );
}
fn cpu_rows() -> Vec<Value> {
    let mut fixture =
        crate::sampling_contract::fixture_contract("small_file", "encode:10:4:524288").unwrap();
    let p =
        json!({"len":524288,"capacity":524288,"offset_64":16,"offset_4096":16,"offset_page":16});
    fixture.as_object_mut().unwrap().extend(json!({"entry":"Vec<Vec<u8>>","lifetime":"worker","page_size":4096,"source":vec![p;14],"scratch":[]}).as_object().unwrap().clone());
    (0..24).map(|n|json!({"epoch":n/12,"cpu":2,"index":n%12,"iterations":384,"kind":if n%12<3 {"warmup"} else {"sample"},"case":"encode:10:4:524288","call_entry":"Vec<Vec<u8>>","backend":"scalar","fixture":fixture,
        "ns_per_iter":400000.0,"timed_ns":153600000.0,"wall_ns":160000000,"before":{"cpu":2,"run_ns":1,"wait_ns":1,"switches":1,"process_minor_faults":0,"process_major_faults":0},
        "after":{"cpu":2,"run_ns":1000,"wait_ns":2,"switches":3,"process_minor_faults":0,"process_major_faults":0},"faults_before":{"cpu":2,"minor":1,"major":0},"faults_after":{"cpu":2,"minor":3,"major":0}})).collect()
}
#[test]
fn cpu_plan_preserves_both_orders_and_both_fresh_waves() {
    let plan = cpu::plan(vec![2, 3, 4]);
    assert_eq!(
        plan[0],
        json!(["same-buffer-forward-reverse", [2, 3, 4, 4, 3, 2]])
    );
    assert_eq!(
        plan[1],
        json!(["same-buffer-reverse-forward", [4, 3, 2, 2, 3, 4]])
    );
    assert_eq!(plan.as_array().unwrap().len(), 8);
}
#[test]
fn cpu_worker_keeps_fault_counts_repeats_and_failed_quality_visible() {
    let rows = cpu_rows();
    let report = cpu::analyze(&rows, &[2, 2], 384, 4096).unwrap();
    assert_eq!(report["groups"][0]["measured_minor_faults"], 18);
    assert_eq!(report["same_worker_repeats"][0]["absolute_drift"], 0.0);
    assert_eq!(report["release_approval"], false);
    for (field, value) in [
        ("index", json!(true)),
        ("timed_ns", json!(1)),
        ("backend", json!("foreign")),
    ] {
        let mut bad = rows.clone();
        bad[23][field] = value;
        assert!(cpu::analyze(&bad, &[2, 2], 384, 4096).is_err(), "{field}");
    }
    let mut bad = rows;
    bad[0]["faults_after"]["minor"] = json!(0);
    assert!(cpu::analyze(&bad, &[2, 2], 384, 4096).is_err());
}

#[test]
fn cpu_quality_failures_and_repeated_cpu_drift_are_preserved() {
    let mut rows = cpu_rows();
    for row in &mut rows[12..] {
        row["ns_per_iter"] = json!(480000.0);
        row["timed_ns"] = json!(480000.0 * 384.0);
        row["wall_ns"] = json!(1);
    }
    for row in &mut rows[21..23] {
        row["ns_per_iter"] = json!(624000.0);
        row["timed_ns"] = json!(624000.0 * 384.0);
    }
    let report = cpu::analyze(&rows, &[2, 2], 384, 4096).unwrap();
    assert_eq!(report["groups"][1]["short_samples"], 9);
    assert!(report["groups"][1]["central_spread"].as_f64().unwrap() > 0.1);
    assert!(
        report["same_worker_repeats"][0]["absolute_drift"]
            .as_f64()
            .unwrap()
            > 0.05
    );
    assert_eq!(report["release_approval"], false);
}
fn slot_rows() -> Vec<Value> {
    let p = json!({"len":65536,"capacity":65536,"offset_64":16,"offset_4096":16,"offset_page":16});
    vec![
        json!({"entry":"Vec<ShardSlot<Vec<u8>>>","source_lifetime":"worker","clone_lifetime":"iteration","page_size":4096,"iterations":4096,"phase_ns":{"clone":10,"reconstruct":20,"drop":5},"source":vec![p.clone();6],"transitions":1,"overflow":0,"layouts":[{"count":4096,"buffers":vec![p;6]}]});
        12
    ]
}
#[test]
fn slot_histograms_conserve_every_iteration_and_keep_overflow_explicit() {
    let mut rows = slot_rows();
    assert_eq!(
        slots::analyze(&rows, 4096, 4096).unwrap()["histogram_complete"],
        true
    );
    rows[0]["layouts"][0]["count"] = json!(4095);
    rows[0]["overflow"] = json!(1);
    assert_eq!(
        slots::analyze(&rows, 4096, 4096).unwrap()["histogram_complete"],
        false
    );
    rows[0]["source"][0]["offset_page"] = json!(4096);
    assert!(slots::analyze(&rows, 4096, 4096).is_err());
}

#[test]
fn slot_observation_counts_phase_extents_and_owner_geometry_are_required() {
    let original = slot_rows();
    for (key, value) in [
        ("iterations", json!(true)),
        ("page_size", json!(16384)),
        ("transitions", json!(4096)),
        ("clone_lifetime", json!("worker")),
    ] {
        let mut rows = original.clone();
        rows[0][key] = value;
        assert!(slots::analyze(&rows, 4096, 4096).is_err(), "{key}");
    }
    let mut rows = original.clone();
    rows[0]["phase_ns"].as_object_mut().unwrap().remove("drop");
    assert!(slots::analyze(&rows, 4096, 4096).is_err());
    let mut rows = original.clone();
    rows[0]["layouts"][0]["count"] = json!(4095);
    assert!(slots::analyze(&rows, 4096, 4096).is_err());
    let mut rows = original;
    rows[0]["layouts"][0]["buffers"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(slots::analyze(&rows, 4096, 4096).is_err());
}
