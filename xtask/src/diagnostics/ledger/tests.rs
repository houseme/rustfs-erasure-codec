use super::*;
use serde_json::json;

fn validate(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.execute()
}

fn scratch(case: &str, iterations: u64, page: u64) -> Value {
    let parts: Vec<_> = case.split(':').collect();
    let length = parts[2].parse::<u64>().unwrap() * parts[3].parse::<u64>().unwrap();
    let capacity = length.max(4096);
    let pages = (16 + capacity).div_ceil(page);
    let mut events = Vec::new();
    for observation in 0..12 {
        for call in 1..=iterations {
            for phase in ["reserved", "initialized", "verified", "released"] {
                let ready = matches!(phase, "verified" | "released");
                events.push(json!({"observation":observation,"call":call,"phase":phase,
                    "len":if phase=="reserved" {0} else {length},"capacity":capacity,
                    "offsets":[16,16,16],"inline":length<=4096,"elapsed_ns":events.len(),
                    "faults":[events.len()*2,0],"snapshot":{"status":"mapped","pages":pages,"resident_pages":pages,"errno":null},
                    "valid":if ready {json!(true)} else {Value::Null}}));
            }
        }
    }
    json!({"kind":"verify-scratch","case":case,"iterations":iterations,"page":page,"native":true,
        "data":{"schema":"verify-scratch-lifecycle-v1","page_size":page,"errors":0,"calls":vec![iterations;12],"release_approval":false,"events":events}})
}

// Independent one-block source contract, rather than the validator's scheduler.
fn stream() -> Value {
    let mut events = Vec::new();
    for observation in 0..12 {
        for call in 1..=2 {
            let mut event = |phase: &str,
                             block,
                             slot: Option<u64>,
                             len,
                             capacity,
                             visible,
                             complete| {
                let present = capacity > 0;
                events.push(json!({"observation":observation,"call":call,"scope":observation*2+call,"parent":0,
                    "api":"verify_stream","variant":"retained-scratch","block":block,"phase":phase,"slot":slot,
                    "len":len,"capacity":capacity,"visible_len":visible,"occupied":slot.is_some(),
                    "offsets":if present {json!([16,16,16])} else {Value::Null},"elapsed_ns":events.len(),"faults":[events.len(),0],
                    "snapshot":{"status":if present {"mapped"} else {"empty"},"pages":if present {17} else {0},"resident_pages":if present {17} else {0},"errno":null},
                    "complete":complete,"valid":if complete {json!(true)} else {Value::Null}}));
            };
            event("enter", 0, None, 0, 0, 0, false);
            for (phase, block) in [
                ("reserved", 0),
                ("read", 1),
                ("ready", 1),
                ("coded", 1),
                ("read", 2),
            ] {
                for slot in 0..6 {
                    event(
                        phase,
                        block,
                        Some(slot),
                        if phase == "reserved" { 0 } else { 65536 },
                        65536,
                        if phase == "reserved" || block == 2 {
                            0
                        } else {
                            65536
                        },
                        false,
                    );
                }
                for slot in [48, 49] {
                    let length = if matches!(phase, "ready" | "coded") || block == 2 {
                        65536
                    } else {
                        0
                    };
                    event(phase, block, Some(slot), length, length, length, false);
                }
            }
            for slot in [0, 1, 2, 3, 4, 5, 48, 49] {
                event(
                    "released",
                    2,
                    Some(slot),
                    65536,
                    65536,
                    if slot < 6 { 0 } else { 65536 },
                    true,
                );
            }
            event("exit", 2, None, 0, 0, 0, true);
        }
    }
    json!({"kind":"stream-core","case":"verify_stream:4:2:65536:65536","iterations":2,"page":4096,"current":true,"native":true,
        "data":{"schema":"stream-core-lifecycle-v1","page_size":4096,"errors":0,"calls":vec![2;12],"release_approval":false,"events":events}})
}

#[test]
fn all_original_verify_cases_keep_every_observation_at_each_page_size() {
    let catalog = crate::sampling_contract::Request::Catalog
        .execute()
        .unwrap();
    let mut cases = 0;
    for profile in ["smoke", "small_file"] {
        for case in catalog["cases"][profile].as_array().unwrap() {
            let case = case.as_str().unwrap();
            if !case.starts_with("verify:") {
                continue;
            }
            cases += 1;
            for page in [4096, 16384, 65536] {
                for iterations in [1, 2] {
                    let result = validate(scratch(case, iterations, page)).unwrap();
                    assert_eq!(result["calls"], 12 * iterations);
                    assert_eq!(result["events"], 48 * iterations);
                }
            }
        }
    }
    assert_eq!(cases, 14);
}

#[test]
fn scratch_order_storage_results_counters_and_required_fields_are_strict() {
    let original = scratch("verify:4:2:1024", 2, 4096);
    let event = &original["data"]["events"][1];
    for field in event.as_object().unwrap().keys() {
        let mut request = original.clone();
        request["data"]["events"][1]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(validate(request).is_err(), "{field}");
    }
    for (field, value) in [
        ("capacity", json!(8192)),
        ("offsets", json!([32, 32, 32])),
        ("faults", Value::Null),
        ("inline", json!(0)),
        ("elapsed_ns", json!(-1)),
        ("valid", json!(true)),
        ("address", json!(1234)),
    ] {
        let mut request = original.clone();
        request["data"]["events"][1][field] = value;
        assert!(validate(request).is_err(), "{field}");
    }
    let mut request = original.clone();
    request["data"]["events"].as_array_mut().unwrap().reverse();
    assert!(validate(request).is_err());
    let mut request = original;
    request["data"]["events"][2]["valid"] = json!(false);
    assert!(validate(request).is_err());
}

#[test]
fn scratch_allows_partial_unmapping_only_after_release() {
    let mut request = scratch("verify:4:2:65536", 2, 4096);
    for event in request["data"]["events"].as_array_mut().unwrap() {
        if event["phase"] == "released" {
            event["snapshot"]["status"] = json!("not_fully_mapped");
            event["snapshot"]["resident_pages"] = Value::Null;
            event["snapshot"]["errno"] = json!(12);
        }
    }
    validate(request.clone()).unwrap();
    request["data"]["events"][1]["snapshot"] = request["data"]["events"][3]["snapshot"].clone();
    assert!(
        validate(request)
            .unwrap_err()
            .to_string()
            .contains("live scratch")
    );
}

#[test]
fn stream_keeps_observation_coordinates_and_source_version() {
    let request = stream();
    let original = request.clone();
    assert_eq!(
        validate(request.clone()).unwrap(),
        json!({"calls":24,"events":1200,"nested_verify_calls":0})
    );
    assert_eq!(request, original);
    let mut old = request;
    old["current"] = json!(false);
    assert!(validate(old).is_err());
}

#[test]
fn stream_refuses_shape_scope_dangling_positions_and_former_owner_changes() {
    let original = stream();
    for (index, field, value) in [
        (0, "parent", json!(8)),
        (0, "complete", json!(true)),
        (1, "len", json!(1)),
        (1, "capacity", json!(1)),
        (1, "visible_len", json!(65536)),
        (1, "occupied", json!(false)),
        (1, "offsets", json!([1, 2, 3])),
        (1, "faults", Value::Null),
        (1199, "valid", json!(false)),
        (1198, "offsets", json!([32, 32, 32])),
    ] {
        let mut request = original.clone();
        request["data"]["events"][index][field] = value;
        assert!(validate(request).is_err(), "{index}/{field}");
    }
    for field in original["data"]["events"][0].as_object().unwrap().keys() {
        let mut request = original.clone();
        request["data"]["events"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(validate(request).is_err(), "{field}");
    }
}

#[test]
fn stream_live_residency_cannot_claim_a_released_range() {
    let mut request = stream();
    for event in request["data"]["events"].as_array_mut().unwrap() {
        if event["phase"] == "released" {
            event["snapshot"]["status"] = json!("not_fully_mapped");
            event["snapshot"]["resident_pages"] = Value::Null;
            event["snapshot"]["errno"] = json!(12);
        }
    }
    validate(request.clone()).unwrap();
    request["data"]["events"][1]["snapshot"] = request["data"]["events"][41]["snapshot"].clone();
    assert!(
        validate(request)
            .unwrap_err()
            .to_string()
            .contains("live source")
    );
}

#[test]
fn unsupported_platforms_keep_null_counts_and_empty_storage_explicit() {
    for mut request in [scratch("verify:4:2:1024", 2, 4096), stream()] {
        request["native"] = json!(false);
        for event in request["data"]["events"].as_array_mut().unwrap() {
            event["faults"] = Value::Null;
            if event["capacity"].as_u64().unwrap() > 0 {
                event["snapshot"] =
                    json!({"status":"unsupported","pages":null,"resident_pages":null,"errno":null});
            }
        }
        validate(request.clone()).unwrap();
        let mut native = request.clone();
        native["native"] = json!(true);
        assert!(validate(native).is_err());
        request["data"]["events"][1]["snapshot"]["resident_pages"] = json!(0);
        assert!(validate(request).is_err());
    }
}

#[test]
fn residency_page_arithmetic_does_not_overflow_u64() {
    let snapshot: Snapshot = serde_json::from_value(
        json!({"status":"mapped","pages":4503599627370497_u64,"resident_pages":0,"errno":null}),
    )
    .unwrap();
    snapshot
        .validate(u64::MAX, Some(4095), 4096, true, false, false)
        .unwrap();
    assert!(
        snapshot
            .validate(u64::MAX, Some(4096), 4096, true, false, false)
            .is_err()
    );
}

#[test]
fn negative_first_timestamps_and_out_of_range_counts_are_refused() {
    for mut request in [scratch("verify:4:2:1024", 2, 4096), stream()] {
        let original = request.clone();
        request["data"]["events"][0]["elapsed_ns"] = json!(-1);
        assert!(validate(request).is_err());
        let mut overflow = original;
        overflow["data"]["events"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()["elapsed_ns"] = json!(18446744073709551616.0_f64);
        assert!(validate(overflow).is_err());
    }
}
