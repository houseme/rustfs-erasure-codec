use super::*;
use sha2::{Digest, Sha256};

#[test]
fn inventory_reuses_validation_and_deduplicates_iteration_shapes_without_losing_summaries() {
    for case in crate::sampling_contract::case_catalog()["stream"].iter() {
        let mut shapes = None;
        for iterations in [1, 2] {
            let view = fixture(case, iterations, 4096);
            let summary = observe(case, view.clone(), iterations, 4096, true).unwrap();
            let input = json!({"operation":"inventory","observations":[{"case":case,"iterations":iterations,
                "page_size":4096,"observation":0,"view":view,"require_counters":true}]});
            let report = serde_json::from_value::<Request>(input.clone())
                .unwrap()
                .execute()
                .unwrap();
            assert_eq!(report[0]["calls"], summary[0]);
            assert_eq!(report[0]["case"], *case);
            if let Some(prior) = shapes {
                assert_eq!(report[0]["shapes"], prior);
            }
            shapes = Some(report[0]["shapes"].clone());
            let mut invalid = input;
            invalid["observations"][0]["view"]["completed"] = json!(0);
            assert!(
                serde_json::from_value::<Request>(invalid)
                    .unwrap()
                    .execute()
                    .is_err()
            );
        }
    }
}

fn event(
    events: &mut Vec<Value>,
    call: u64,
    role: &str,
    index: u64,
    len: u64,
    capacity: Option<u64>,
    bytes: u64,
) {
    let storage = capacity.unwrap_or(len) > 0;
    events.push(json!({"call":call,"role":role,"index":index,"len":len,"capacity":capacity,
        "offsets":if storage {json!([16,16,16])} else {Value::Null},"bytes":bytes,"same_address_as_previous_view":null}));
}
fn resident(call: u64, boundary: &str, role: &str, index: u64, size: u64, page: u64) -> Value {
    let pages = if size > 0 {
        (16 + size).div_ceil(page)
    } else {
        0
    };
    json!({"call":call,"boundary":boundary,"role":role,"index":index,"bytes":size,"offset_page":if size>0 {json!(16)} else {Value::Null},
        "snapshot":{"status":if size>0 {"mapped"} else {"empty"},"pages":pages,"resident_pages":pages,"errno":null}})
}
fn fixture(case: &str, iterations: u64, page: u64) -> Value {
    let s = Shape::parse(case).unwrap();
    let mut events = Vec::new();
    for index in 0..s.sources as u64 {
        event(
            &mut events,
            0,
            "retained_source",
            index,
            s.size,
            Some(s.size),
            0,
        );
    }
    for call in 1..=iterations {
        if s.operation == "reconstruct_stream" {
            for role in [
                "cursor_before_missing",
                "cursor_missing",
                "cursor_after_recovery",
            ] {
                for index in 0..s.sources as u64 {
                    let size = if role == "cursor_missing" && [0, s.data].contains(&index) {
                        0
                    } else {
                        s.size
                    };
                    event(&mut events, call, role, index, size, Some(size), 0);
                }
            }
            continue;
        }
        for index in 0..s.sources as u64 {
            let mut remaining = s.size;
            while remaining > 0 {
                let n = remaining.min(s.block);
                event(
                    &mut events,
                    call,
                    "source_read_view",
                    index,
                    remaining,
                    None,
                    n,
                );
                event(
                    &mut events,
                    call,
                    "read_scratch_view",
                    index,
                    s.block,
                    None,
                    n,
                );
                remaining -= n;
            }
            event(&mut events, call, "source_read_view", index, 0, None, 0);
            event(
                &mut events,
                call,
                "read_scratch_view",
                index,
                s.block,
                None,
                0,
            );
        }
        if s.operation == "encode_stream" {
            for index in 0..s.parity {
                let mut used = 0;
                while used < s.size {
                    let n = s.block.min(s.size - used);
                    event(&mut events, call, "parity_write_view", index, n, None, n);
                    event(
                        &mut events,
                        call,
                        "writer_before",
                        index,
                        used,
                        Some(s.size),
                        0,
                    );
                    event(
                        &mut events,
                        call,
                        "writer_after",
                        index,
                        used + n,
                        Some(s.size),
                        n,
                    );
                    used += n;
                }
            }
        }
    }
    let mut windows = Vec::new();
    let (mut clock, mut counter) = (0, 0);
    for call in 1..=iterations {
        let (start, before) = (clock, counter);
        if s.operation == "encode_stream" {
            let descriptors = std::iter::once(("reserve", None, s.parity * s.size))
                .chain(
                    events
                        .iter()
                        .filter(|e| e["call"] == call && e["role"] == "parity_write_view")
                        .map(|e| ("append", e["index"].as_u64(), e["bytes"].as_u64().unwrap())),
                )
                .chain(std::iter::once(("drop", None, s.parity * s.size)));
            for (phase, index, bytes) in descriptors {
                clock += 10;
                counter += 2;
                windows.push(json!({"call":call,"phase":phase,"index":index,"bytes":bytes,"start_ns":clock,"end_ns":clock+5,"before":[counter,counter,counter,counter],"after":[counter+1,counter+1,counter+1,counter+1]}));
                clock += 10;
                counter += 2;
            }
        }
        clock += 10;
        counter += 2;
        windows.push(json!({"call":call,"phase":"call","index":null,"bytes":0,"start_ns":start,"end_ns":clock,"before":[before,before,before,before],"after":[counter,counter,counter,counter]}));
    }
    let mut residency = Vec::new();
    for call in 0..=iterations {
        let current: Vec<_> = events.iter().filter(|e| e["call"] == call).collect();
        if call > 0 && s.operation == "encode_stream" {
            for index in 0..s.parity {
                residency.push(resident(
                    call,
                    "after_reserve",
                    "writer_before",
                    index,
                    s.size,
                    page,
                ));
            }
        }
        for (i, e) in current.iter().enumerate() {
            let role = e["role"].as_str().unwrap();
            let index = e["index"].as_u64().unwrap();
            if role == "source_read_view" {
                residency.push(resident(
                    call,
                    "before_read",
                    "read_scratch_view",
                    index,
                    current[i + 1]["len"].as_u64().unwrap(),
                    page,
                ));
            }
            let size = e["capacity"].as_u64().unwrap_or(e["len"].as_u64().unwrap());
            residency.push(resident(call, "view", role, index, size, page));
        }
        if call > 0 && s.operation == "encode_stream" {
            for role in ["read_scratch_view", "parity_write_view", "writer_after"] {
                let mut last = BTreeMap::new();
                for e in &current {
                    if e["role"] == role {
                        last.insert(e["index"].as_u64().unwrap(), e);
                    }
                }
                for (index, e) in last {
                    residency.push(resident(
                        call,
                        if role == "writer_after" {
                            "after_writer_drop"
                        } else {
                            "after_api_return"
                        },
                        role,
                        index,
                        e["capacity"].as_u64().unwrap_or(e["len"].as_u64().unwrap()),
                        page,
                    ));
                }
            }
        }
    }
    json!({"schema":"stream-buffer-views-v3","observation":0,"iterations":iterations,"calls":iterations,"completed":iterations,"page_size":page,"overflow":0,"release_approval":false,"events":events,"windows":windows,"residency":residency})
}
fn observe(case: &str, view: Value, iterations: u64, page: u64, native: bool) -> Result<Value> {
    let request: Request = serde_json::from_value(
        json!({"operation":"validate","summary":true,"observations":[{"case":case,"view":view,"iterations":iterations,"page_size":page,"observation":0,"require_counters":native}]}),
    )?;
    request.execute()
}
#[test]
fn all_stream_shapes_preserve_old_window_summary_values() {
    let catalog = crate::sampling_contract::Request::Catalog
        .execute()
        .unwrap();
    let mut reports = BTreeMap::new();
    for case in catalog["cases"]["stream"].as_array().unwrap() {
        let case = case.as_str().unwrap();
        for iterations in [1, 2] {
            for page in [4096, 65536] {
                let v = fixture(case, iterations, page);
                let summary = observe(case, v.clone(), iterations, page, true).unwrap();
                let direct:Request=serde_json::from_value(json!({"operation":"summarize","view":{"calls":iterations,"windows":v["windows"]}})).unwrap();
                assert_eq!(summary[0], direct.execute().unwrap());
                reports.insert(format!("{case}/{iterations}/{page}"), summary[0].clone());
            }
        }
    }
    // Golden value set captured from the Python implementation before migration.
    let digest = Sha256::digest(serde_json::to_vec(&reports).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "2bde578abb58247a6b58b42d49e0d6b321b255d5070189c54c9cd09e08eeb0cf"
    );
}
#[test]
fn missing_nullable_fields_and_unknown_fields_cannot_fabricate_observations() {
    let case = "encode_stream:4:2:65536:65536";
    let original = fixture(case, 2, 4096);
    for (section, keys) in [
        (
            "events",
            vec!["capacity", "offsets", "same_address_as_previous_view"],
        ),
        ("windows", vec!["index", "before", "after"]),
    ] {
        for key in keys {
            let mut v = original.clone();
            v[section][0].as_object_mut().unwrap().remove(key);
            assert!(observe(case, v, 2, 4096, false).is_err());
        }
    }
    for key in ["pages", "resident_pages", "errno"] {
        let mut v = original.clone();
        v["residency"][0]["snapshot"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(observe(case, v, 2, 4096, false).is_err());
    }
    for section in ["events", "windows", "residency"] {
        let mut v = original.clone();
        v[section][0]["address"] = json!(123);
        assert!(observe(case, v, 2, 4096, false).is_err());
    }
}
#[test]
fn eof_cursor_transitions_capacity_and_source_geometry_remain_mandatory() {
    for (case, role, field, value) in [
        (
            "encode_stream:4:2:65536:65536",
            "read_scratch_view",
            "capacity",
            json!(65536),
        ),
        (
            "encode_stream:4:2:65536:65536",
            "writer_after",
            "capacity",
            json!(65537),
        ),
        (
            "reconstruct_stream:4:2:65536:65536",
            "cursor_missing",
            "len",
            json!(65536),
        ),
        (
            "verify_stream:4:2:65536:65536",
            "source_read_view",
            "offsets",
            json!([32, 32, 32]),
        ),
    ] {
        let mut v = fixture(case, 2, 4096);
        let e = v["events"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["role"] == role)
            .unwrap();
        e[field] = value;
        assert!(observe(case, v, 2, 4096, false).is_err());
    }
    let case = "encode_stream:4:2:65536:65536";
    let mut v = fixture(case, 2, 4096);
    v["events"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["role"] != "source_read_view" || e["bytes"] != 0);
    assert!(observe(case, v, 2, 4096, false).is_err());
}
#[test]
fn window_partition_and_explicit_unknown_counters_keep_their_meaning() {
    let case = "encode_stream:4:2:65536:65536";
    let mut original = fixture(case, 2, 4096);
    for (field, value) in [
        ("start_ns", json!(0)),
        ("index", json!(9)),
        ("bytes", json!(1)),
        ("after", json!([0, 0, 0, 0])),
        ("before", Value::Null),
    ] {
        let mut v = original.clone();
        v["windows"][1][field] = value;
        assert!(observe(case, v, 2, 4096, false).is_err());
    }
    for w in original["windows"].as_array_mut().unwrap() {
        w["before"] = Value::Null;
        w["after"] = Value::Null;
    }
    let summary = observe(case, original.clone(), 2, 4096, false).unwrap();
    assert!(summary[0][0]["windows"]["call"]["process_minor_faults"].is_null());
    assert!(
        observe(case, original, 2, 4096, true)
            .unwrap_err()
            .to_string()
            .contains("Linux lifecycle counters")
    );
}
#[test]
fn residency_requires_all_live_and_former_ranges_without_fabricated_page_identity() {
    let case = "encode_stream:4:2:65536:65536";
    let original = fixture(case, 2, 4096);
    let mut former = original.clone();
    let r = former["residency"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["boundary"] == "after_api_return")
        .unwrap();
    r["snapshot"]["status"] = json!("not_fully_mapped");
    r["snapshot"]["resident_pages"] = Value::Null;
    r["snapshot"]["errno"] = json!(12);
    observe(case, former, 2, 4096, true).unwrap();
    for (field, value) in [
        ("resident_pages", json!(999999)),
        ("status", json!("query_failed")),
        ("pages", Value::Null),
    ] {
        let mut v = original.clone();
        v["residency"][0]["snapshot"][field] = value;
        assert!(observe(case, v, 2, 4096, true).is_err());
    }
    let mut v = original;
    for r in v["residency"].as_array_mut().unwrap() {
        r["snapshot"] =
            json!({"status":"unsupported","pages":null,"resident_pages":null,"errno":null});
    }
    observe(case, v.clone(), 2, 4096, false).unwrap();
    assert!(observe(case, v, 2, 4096, true).is_err());
}
#[test]
fn absurd_declared_calls_fail_before_allocating_per_call_state() {
    let case = "encode_stream:4:2:65536:65536";
    let mut v = fixture(case, 2, 4096);
    for key in ["calls", "iterations", "completed"] {
        v[key] = json!(u64::MAX);
    }
    assert!(observe(case, v, u64::MAX, 4096, false).is_err());
}

#[test]
fn native_source_positions_must_match_the_retained_caller_fixture() {
    let case = "encode_stream:4:2:65536:65536";
    let view = fixture(case, 2, 4096);
    let source:Vec<_>=view["events"].as_array().unwrap().iter().filter(|e|e["call"]==0)
        .map(|e|json!({"len":e["len"],"capacity":e["capacity"],"offset_64":e["offsets"][0],"offset_4096":e["offsets"][1],"offset_page":e["offsets"][2]})).collect();
    let mut request = json!({"operation":"validate","observations":[{"case":case,"iterations":2,"page_size":4096,"observation":0,"view":view,"source_fixture":source}]});
    serde_json::from_value::<Request>(request.clone())
        .unwrap()
        .execute()
        .unwrap();
    request["observations"][0]["source_fixture"][0]["capacity"] = json!(65537);
    assert!(
        serde_json::from_value::<Request>(request)
            .unwrap()
            .execute()
            .unwrap_err()
            .to_string()
            .contains("source differs")
    );
}

#[test]
fn underlying_adapter_preserves_original_records_and_declares_actual_entries() {
    let original = json!({"case":"encode_stream:4:2:65536:65536","fixture":{"call_entry":ENTRIES[0].1},
        "workload":{"call_entry":ENTRIES[0].1,"seed":u64::MAX},"rows":[{"metric":8.344125545873929e-19}]});
    let result = Request::Underlying {
        records: vec![original.clone()],
    }
    .execute()
    .unwrap();
    assert_eq!(original["fixture"]["call_entry"], ENTRIES[0].1);
    assert_eq!(
        result[0]["fixture"]["call_entry"],
        "Vec<&[u8]> -> Vec<Vec<u8>>"
    );
    assert_eq!(result[0]["workload"]["seed"], u64::MAX);
    assert_eq!(result[0]["rows"], original["rows"]);
    let mut wrong = original;
    wrong["workload"]["call_entry"] = json!("unobserved API");
    assert!(
        Request::Underlying {
            records: vec![wrong]
        }
        .execute()
        .is_err()
    );
}
