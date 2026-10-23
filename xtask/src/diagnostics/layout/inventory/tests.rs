use super::*;

fn fixture() -> (BTreeMap<String, Vec<Value>>, Sources) {
    let mut data = BTreeMap::new();
    let catalog = sampling_contract::case_catalog();
    for phase in PHASES {
        let revision = if phase.starts_with('A') { "a" } else { "b" }.repeat(40);
        let mut records = Vec::new();
        for profile in ["small_file", "stream"] {
            for case in &catalog[profile] {
                let fields: Vec<_> = case.split(':').collect();
                for index in 0..12 {
                    let mut record = sampling_contract::tests::record(profile, case, index);
                    record["revision"] = json!(revision);
                    let mut row = json!({"schema_version":1,"benchmark_metrics_enabled":false,
                        "artifact_kind":if profile=="stream" {"stream-path-results"} else {"small-file-results"},
                        "git_revision":revision,"iterations":1,"profile":"fast","operation":fields[0],
                        "data_shards":fields[1].parse::<u64>().unwrap(),"parity_shards":fields[2].parse::<u64>().unwrap(),
                        "shard_size":fields[3].parse::<u64>().unwrap(),"throughput_mb_s":100.0,
                        "target_triple":"fixture-target","backend":"scalar","backend_id":"scalar",
                        "backend_kind":"scalar","backend_override":"auto","override_honored":true,"features":"simd-accel|std"});
                    if profile == "stream" {
                        row["stream_block_size"] = json!(fields[4].parse::<u64>().unwrap());
                        row["stream_io_mode"] = json!("serial");
                        row["ns_per_block"] = json!(200_000_000.0);
                    }
                    record["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .extend(row.as_object().unwrap().clone());
                    records.push(record);
                }
            }
        }
        data.insert(phase.into(), records);
    }
    (
        data,
        Sources {
            baseline: Source {
                commit: "a".repeat(40),
            },
            candidate: Source {
                commit: "b".repeat(40),
            },
        },
    )
}
fn analyze(data: &BTreeMap<String, Vec<Value>>, sources: &Sources) -> Result<Value> {
    retained(data, sources, Some(&[0, 1, 2, 3, 4, 5, 6, 7]), Some(4096))
}
#[test]
fn retained_inventory_preserves_all_pairs_sorted_fields_and_explicit_non_attribution() {
    let (mut data, sources) = fixture();
    let report = analyze(&data, &sources).unwrap();
    assert_eq!(report["case_count"], 103);
    assert_eq!(report["layout_checks"].as_array().unwrap().len(), 618);
    assert_eq!(report["layout_mismatches"], json!([]));
    for phase in ["B1", "B2"] {
        for record in data.get_mut(phase).unwrap() {
            if record["case"]
                .as_str()
                .unwrap()
                .starts_with("verify_with_buffer:")
            {
                for field in ["offset_64", "offset_4096", "offset_page"] {
                    record["fixture"]["scratch"][0][field] = json!(32);
                }
            }
        }
    }
    let report = analyze(&data, &sources).unwrap();
    let mismatches = report["layout_mismatches"].as_array().unwrap();
    assert_eq!(mismatches.len(), 44);
    assert_eq!(
        mismatches[0]["different_buffers"][0]["fields"],
        json!(["offset_4096", "offset_64", "offset_page"])
    );
    assert_eq!(report["release_approval"], false);
    assert!(report["candidate_attribution"].is_null());
}
#[test]
fn missing_phases_observations_and_invalid_nonmedian_provenance_are_refused() {
    let (data, sources) = fixture();
    let mut changed = data.clone();
    changed.remove("A2");
    assert!(analyze(&changed, &sources).is_err());
    let mut changed = data.clone();
    changed.get_mut("B2").unwrap().pop();
    assert!(analyze(&changed, &sources).is_err());
    for (field, value) in [
        ("features", json!("std")),
        ("iterations", json!(true)),
        ("ns_per_iter", json!(-1)),
    ] {
        let mut changed = data.clone();
        changed.get_mut("B1").unwrap()[4]["rows"][0][field] = value;
        assert!(analyze(&changed, &sources).is_err(), "{field}");
    }
}
#[test]
fn cross_phase_backend_and_constructor_changes_cannot_be_reported_as_geometry() {
    let (data, sources) = fixture();
    for field in ["backend_id", "constructor_budget"] {
        let mut changed = data.clone();
        for record in changed.get_mut("B1").unwrap() {
            if field == "constructor_budget" {
                record["placement"][field] = json!(7);
            } else {
                record["rows"][0][field] = json!("changed");
            }
        }
        assert!(analyze(&changed, &sources).is_err(), "{field}");
    }
}

#[test]
fn stream_preparation_retains_the_optional_cpu_policy_without_weakening_worker_checks() {
    let (mut data, sources) = fixture();
    let entries = views::Request::Entries.execute().unwrap();
    let mut records: Vec<_> = data
        .remove("A1")
        .unwrap()
        .into_iter()
        .filter(|r| cases("stream").contains(r["case"].as_str().unwrap()))
        .collect();
    for record in &mut records {
        let operation = record["case"]
            .as_str()
            .unwrap()
            .split(':')
            .next()
            .unwrap()
            .to_owned();
        for field in ["fixture", "workload"] {
            record[field]["call_entry"] = entries[&operation].clone();
        }
    }
    let request = json!({"operation":"stream-phase","records":records,"revision":sources.baseline.commit,
        "iterations":1,"cpus":null,"page_size":4096,"reference":null});
    let result = serde_json::from_value::<Request>(request.clone())
        .unwrap()
        .execute()
        .unwrap();
    assert_eq!(result.as_object().unwrap().len(), 15);
    let mut invalid = request;
    invalid["records"][4]["after"]["run_ns"] = json!(0);
    assert!(
        serde_json::from_value::<Request>(invalid)
            .unwrap()
            .execute()
            .is_err()
    );
}
fn view_reports() -> BTreeMap<String, Vec<views::Signature>> {
    let mut data = BTreeMap::new();
    for phase in PHASES {
        let mut reports = Vec::new();
        for case in cases("stream") {
            for observation in (0..12).step_by(4) {
                reports.push(serde_json::from_value(json!({"case":case,"observations":(observation..observation+4).collect::<Vec<_>>(),
                    "shapes":[["source_read_view",0,1024,null,[16,16,16]]]})).unwrap());
            }
        }
        data.insert(phase.into(), reports);
    }
    data
}
#[test]
fn stream_inventory_compares_only_shapes_across_observations() {
    let mut data = view_reports();
    let value = stream_comparison(&data).unwrap();
    assert_eq!(value["observations"], 720);
    assert_eq!(value["layout_mismatches"], json!([]));
    assert!(value.get("lifecycle_windows").is_none());
    data.get_mut("B1").unwrap()[0].shapes.insert(
        serde_json::from_value(json!(["read_scratch_view", 0, u64::MAX, null, null])).unwrap(),
    );
    let value = stream_comparison(&data).unwrap();
    assert_eq!(value["layout_mismatches"].as_array().unwrap().len(), 3);
    assert_eq!(value["layout_mismatches"][0]["different_view_shapes"], 1);
    assert_eq!(value["release_approval"], false);
}
#[test]
fn stream_inventory_requires_the_complete_phase_and_per_case_observation_schedule() {
    let mut data = view_reports();
    data.get_mut("A2").unwrap().pop();
    assert!(stream_comparison(&data).is_err());
    let mut data = view_reports();
    data.get_mut("B1").unwrap()[0].observations[0] = 1;
    assert!(stream_comparison(&data).is_err());
}
