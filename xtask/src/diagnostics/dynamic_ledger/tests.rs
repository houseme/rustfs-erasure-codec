use super::*;

fn event(events: &mut Vec<Value>, kind: &str, call: u64, fields: Value) {
    let mut value = json!({"kind":kind,"observation":(call.saturating_sub(1))/2,"call":call,"thread":0,"region":"caller",
        "allocation":null,"allocation_offset":null,"previous":null,"role":"","stage":"","index":0,
        "len":0,"capacity":null,"element_size":1,"shape":null,"storage":"","occupied":false,"present":null,
        "bytes":0,"align":0,"offsets":null,"elapsed_ns":events.len(),"faults":null,
        "snapshot":{"status":"empty","pages":0,"resident_pages":0,"errno":null}});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    events.push(value);
}
fn storage(ident: u64) -> Value {
    let p = (ident * 16) % 4096;
    json!({"allocation":ident,"allocation_offset":0,"bytes":65536,"offsets":[p%64,p,p],
        "snapshot":{"status":"unsupported","pages":null,"resident_pages":null,"errno":null}})
}
fn fixture() -> Value {
    // Independent generated-source encode: six owners move through two views
    // and are all freed inside each of the original twenty-four calls.
    let mut events = Vec::new();
    let mut ident = 0;
    for call in 1..=24 {
        event(&mut events, "begin", call, json!({}));
        let first = ident + 1;
        for _ in 0..6 {
            ident += 1;
            let mut fields = storage(ident);
            fields["align"] = json!(1);
            event(&mut events, "alloc", call, fields);
        }
        for stage in ["created", "encoded"] {
            for index in 0..6 {
                let mut fields = storage(first + index);
                fields.as_object_mut().unwrap().extend(json!({"role":"source","stage":stage,"index":index,"len":65536,"capacity":65536,"storage":"heap","occupied":true,"present":true}).as_object().unwrap().clone());
                event(&mut events, "view", call, fields);
            }
        }
        for id in first..first + 6 {
            let mut fields = storage(id);
            fields["align"] = json!(1);
            event(&mut events, "free", call, fields);
        }
        event(&mut events, "end", call, json!({}));
    }
    event(&mut events, "finish", 0, json!({"region":"between"}));
    json!({"schema":"dynamic-buffer-lifecycle-v1","page_size":4096,"errors":0,"calls":vec![2;12],"release_approval":false,"events":events})
}
fn validate(value: Value, native: bool) -> Result<Value> {
    serde_json::from_value::<Ledger>(value)?.validate(
        "smoke",
        "encode:4:2:65536",
        4096,
        native,
        false,
    )
}

#[test]
fn generated_owner_lifetimes_preserve_the_complete_original_summary() {
    let data = fixture();
    let original = data.clone();
    assert_eq!(
        validate(data.clone(), false).unwrap(),
        json!({"iterations":24,"events":625,"allocation_generations":144,"retained_requests":0,"payload_views":288,"decoder_matrix_views":0,"bounded_residency_queries":0})
    );
    assert_eq!(data, original);
}
#[test]
fn dead_foreign_and_reused_generations_cannot_replace_generated_source() {
    let mut data = fixture();
    data["events"][13]["allocation"] = json!(2);
    data["events"][13]["offsets"] = json!([32, 32, 32]);
    assert!(
        validate(data, false)
            .unwrap_err()
            .to_string()
            .contains("replaced generated source")
    );
    let mut data = fixture();
    data["events"].as_array_mut().unwrap().swap(13, 19);
    for (i, e) in data["events"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        e["elapsed_ns"] = json!(i);
    }
    assert!(
        validate(data, false)
            .unwrap_err()
            .to_string()
            .contains("live allocation")
    );
}
#[test]
fn complete_geometry_scope_presence_and_explicit_nulls_are_required() {
    let original = fixture();
    for (index, key, value) in [
        (1, "address", json!(1234)),
        (1, "align", json!(3)),
        (1, "allocation", json!(2)),
        (7, "len", json!(true)),
        (7, "capacity", json!(65535)),
        (7, "present", json!(false)),
        (7, "allocation_offset", json!(1)),
        (7, "offsets", json!([0, 16, 16])),
        (0, "role", json!("source")),
        (0, "region", json!([])),
    ] {
        let mut data = original.clone();
        data["events"][index][key] = value;
        assert!(validate(data, false).is_err(), "{index}/{key}");
    }
    for key in original["events"][0].as_object().unwrap().keys() {
        let mut data = original.clone();
        data["events"][0].as_object_mut().unwrap().remove(key);
        assert!(validate(data, false).is_err(), "{key}");
    }
    let mut data = original;
    data["events"].as_array_mut().unwrap().pop();
    assert!(validate(data, false).is_err());
}
#[test]
fn portable_rows_cannot_supply_native_residency_or_faults() {
    assert!(validate(fixture(), true).is_err());
    let mut data = fixture();
    data["events"][7]["faults"] = json!([1, 0]);
    assert!(validate(data, false).is_err());
}
fn internal(fields: Value) -> Event {
    let mut events = Vec::new();
    event(&mut events, "view", 1, fields);
    serde_json::from_value(events.pop().unwrap()).unwrap()
}
#[test]
fn leopard_lane_extents_and_chunk_initialization_are_typed() {
    let contract = Contract::parse("decoder", "leopard_gf16:10:4:65600", false).unwrap();
    let fields = json!({"role":"leopard_work","shape":[16,32,32768],"capacity":524288,"len":524288,"element_size":2,"stage":"chunk_coded","index":0,"present":null,"occupied":true,"storage":"heap"});
    contract.internal(&internal(fields.clone())).unwrap();
    for (key, value) in [
        ("shape", json!([16, 32769, 32768])),
        ("capacity", json!(524287)),
        ("len", json!(0)),
        ("element_size", json!(1)),
        ("occupied", json!(false)),
        ("stage", json!("unknown")),
    ] {
        let mut bad = fields.clone();
        bad[key] = value;
        assert!(contract.internal(&internal(bad)).is_err(), "{key}");
    }
}
#[test]
fn legacy_all_shard_decode_owners_are_distinct_from_missing_only_owners() {
    let e = internal(
        json!({"role":"leopard_decoded","index":1,"len":32768,"capacity":32768,"element_size":1,"stage":"decoded","present":null,"occupied":true,"storage":"heap"}),
    );
    Contract::parse("decoder", "leopard_gf8:10:4:32768", true)
        .unwrap()
        .internal(&e)
        .unwrap();
    assert!(
        Contract::parse("decoder", "leopard_gf8:10:4:32768", false)
            .unwrap()
            .internal(&e)
            .is_err()
    );
}

#[test]
fn leopard_gf16_public_bytes_and_internal_lanes_have_different_widths() {
    let contract = Contract::parse("decoder", "leopard_gf16:10:4:65600", false).unwrap();
    let mut fields = json!({"role":"matrix","capacity":100,"len":100,"element_size":1,"stage":"constructed","index":0,"present":null,"occupied":true,"storage":"heap"});
    contract.internal(&internal(fields.clone())).unwrap();
    fields["element_size"] = json!(2);
    assert!(contract.internal(&internal(fields)).is_err());
}
#[test]
fn bounded_queries_require_the_actual_page_extent_and_native_platform() {
    let snap: Snapshot = serde_json::from_value(
        json!({"status":"over_budget","pages":4098,"resident_pages":null,"errno":null}),
    )
    .unwrap();
    snap.validate_dynamic(4098 * 4096, Some(0), 4096, true, false)
        .unwrap();
    assert!(
        snap.validate_dynamic(4098 * 4096, Some(0), 4096, false, false)
            .is_err()
    );
    assert!(
        snap.validate_dynamic(4097 * 4096, Some(0), 4096, true, false)
            .is_err()
    );
    assert!(snap.validate_dynamic(0, None, 4096, true, false).is_err());
}
#[test]
fn dynamic_scope_markers_and_regions_cannot_escape_their_iteration() {
    for (index, key, value) in [
        (0, "call", json!(2)),
        (0, "thread", json!(1)),
        (1, "observation", json!(2)),
        (25, "kind", json!("leave")),
        (25, "kind", json!("finish")),
        (1, "thread", json!(64)),
    ] {
        let mut data = fixture();
        data["events"][index][key] = value;
        assert!(validate(data, false).is_err(), "{index}/{key}");
    }
}

#[test]
fn reallocation_replaces_only_a_live_generation_with_the_same_alignment() {
    let mut data = fixture();
    for event in data["events"].as_array_mut().unwrap() {
        if let Some(id) = event["allocation"].as_u64() {
            event["allocation"] = json!(id + 2);
        }
    }
    let mut prefix = Vec::new();
    for (kind, id, previous, bytes) in [
        ("alloc", 1, Value::Null, 32),
        ("realloc", 2, json!(1), 64),
        ("free", 2, Value::Null, 64),
    ] {
        event(
            &mut prefix,
            kind,
            0,
            json!({"region":"setup","allocation":id,"allocation_offset":0,"previous":previous,"bytes":bytes,"align":8,"offsets":[0,0,0],"snapshot":{"status":"unsupported","pages":null,"resident_pages":null,"errno":null}}),
        );
    }
    prefix.append(data["events"].as_array_mut().unwrap());
    for (index, event) in prefix.iter_mut().enumerate() {
        event["elapsed_ns"] = json!(index);
    }
    data["events"] = json!(prefix);
    assert_eq!(
        validate(data.clone(), false).unwrap()["allocation_generations"],
        146
    );
    let mut bad = data.clone();
    bad["events"][1]["previous"] = json!(3);
    assert!(validate(bad, false).is_err());
    let mut bad = data;
    bad["events"][1]["align"] = json!(16);
    assert!(validate(bad, false).is_err());
}

#[test]
fn native_fault_sequences_are_monotonic_per_observed_thread() {
    let mut data = fixture();
    for event in data["events"].as_array_mut().unwrap() {
        if let Some(size) = event["bytes"].as_u64().filter(|n| *n > 0) {
            let pages = (size + event["offsets"][2].as_u64().unwrap()).div_ceil(4096);
            event["snapshot"] =
                json!({"status":"mapped","pages":pages,"resident_pages":pages,"errno":null});
        }
        if event["kind"] == "view" {
            event["faults"] = json!([event["elapsed_ns"], 0]);
        }
    }
    for index in [7, 8] {
        data["events"][index]["thread"] = json!(1);
        data["events"][index]["faults"] = json!([1, 0]);
    }
    validate(data.clone(), true).unwrap();
    data["events"][8]["faults"] = json!([0, 0]);
    assert!(validate(data, true).is_err());
}

#[test]
fn decoder_header_preserves_byte_width_constructor_budget_and_diagnostic_fence() {
    let case = "leopard_gf16:10:4:65600";
    let header = json!({"schema":"decoder-owner-supplement-v1","observer":"dynamic-source-copy-v1","case":case,"data_shards":10,"parity_shards":4,
        "shard_size_elements":65600,"element_size":1,"family":"LeopardGF16","inversion_cache":true,"fast_one_parity":false,"source_lifetime":"worker",
        "construction":"codec-then-source-then-explicit-pool-preparation","output_policy":"iteration-cloned-options-drop-missing-reconstruct-all",
        "missing_mask":1025,"required_mask":16383,"pool_threads":4,"native_placement_checked":false,"normal_outputs_match_source":true,"release_approval":false,"constructor_budget":4});
    supplement_header(&header, case, None).unwrap();
    for (field, value) in [
        ("element_size", json!(2)),
        ("release_approval", json!(true)),
        ("constructor_budget", json!(true)),
        ("family", json!("Classic")),
        ("extra", json!(0)),
    ] {
        let mut bad = header.clone();
        bad[field] = value;
        assert!(supplement_header(&bad, case, None).is_err(), "{field}");
    }
    for field in header.as_object().unwrap().keys() {
        let mut bad = header.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(supplement_header(&bad, case, None).is_err(), "{field}");
    }
    let mut native = header;
    native["native_placement_checked"] = json!(true);
    supplement_header(&native, case, Some(&[0, 1, 2, 3])).unwrap();
    assert!(supplement_header(&native, case, Some(&[0, 1])).is_err());
}

fn reconstruction_fixture() -> Value {
    let mut events = Vec::new();
    let mut generation = 0;
    for call in 1..=24 {
        event(&mut events, "begin", call, json!({}));
        let mut owners = [None; 6];
        for owner in &mut owners {
            generation += 1;
            *owner = Some(generation);
            let mut fields = storage(generation);
            fields["align"] = json!(1);
            event(&mut events, "alloc", call, fields);
        }
        let views = |events: &mut Vec<Value>, role, stage, owners: &[Option<u64>; 6]| {
            for (index, owner) in owners.iter().enumerate() {
                let mut fields = owner.map_or(json!({}), storage);
                let length = if owner.is_some() { 65536 } else { 0 };
                fields.as_object_mut().unwrap().extend(json!({"role":role,"stage":stage,"index":index,
                    "len":length,"capacity":length,"storage":if owner.is_some() {"heap"} else {"empty"},
                    "occupied":owner.is_some(),"present":owner.is_some()}).as_object().unwrap().clone());
                event(events, "view", call, fields);
            }
        };
        views(&mut events, "source", "created", &owners);
        views(&mut events, "source", "encoded", &owners);
        views(&mut events, "dynamic", "moved", &owners);
        for index in [0, 4] {
            let mut fields = storage(owners[index].take().unwrap());
            fields["align"] = json!(1);
            event(&mut events, "free", call, fields);
        }
        views(&mut events, "dynamic", "missing", &owners);
        event(
            &mut events,
            "view",
            call,
            json!({"role":"matrix","stage":"decode_ready","region":"decode",
            "len":16,"capacity":16,"storage":"inline","occupied":true,"bytes":16,"offsets":[0,0,0],
            "snapshot":{"status":"unsupported","pages":null,"resident_pages":null,"errno":null}}),
        );
        for index in [0, 4] {
            generation += 1;
            owners[index] = Some(generation);
            let mut fields = storage(generation);
            fields["align"] = json!(1);
            event(&mut events, "alloc", call, fields);
        }
        views(&mut events, "dynamic", "recovered", &owners);
        for owner in owners {
            let mut fields = storage(owner.unwrap());
            fields["align"] = json!(1);
            event(&mut events, "free", call, fields);
        }
        event(&mut events, "end", call, json!({}));
    }
    event(&mut events, "finish", 0, json!({"region":"between"}));
    json!({"schema":"dynamic-buffer-lifecycle-v1","page_size":4096,"errors":0,"calls":vec![2;12],"release_approval":false,"events":events})
}

#[test]
fn decoder_evidence_must_belong_to_the_observed_iteration() {
    let original = reconstruction_fixture();
    let validate = |value| -> Result<Value> {
        serde_json::from_value::<Ledger>(value)?.validate(
            "smoke",
            "reconstruct:4:2:65536",
            4096,
            false,
            false,
        )
    };
    let summary = validate(original.clone()).unwrap();
    assert_eq!(summary["iterations"], 24);
    assert_eq!(summary["decoder_matrix_views"], 24);
    for call in [0, 25] {
        let mut data = original.clone();
        for event in data["events"].as_array_mut().unwrap() {
            if event["role"] == "matrix" {
                event["call"] = json!(call);
            }
        }
        assert!(validate(data).is_err(), "decoder call {call}");
    }
    let mut data = original;
    for event in data["events"].as_array_mut().unwrap() {
        if event["role"] == "matrix" {
            event["observation"] = Value::Null;
        }
    }
    assert!(validate(data).is_err());
}

#[test]
fn unscoped_leopard_work_cannot_supply_iteration_evidence() {
    let contract = Contract::parse("decoder", "leopard_gf16:10:4:65600", false).unwrap();
    let mut fields = json!({"role":"leopard_work","shape":[16,32,32768],"capacity":524288,"len":524288,
        "element_size":2,"stage":"chunk_coded","region":"decode","index":0,"present":null,"occupied":true,"storage":"heap"});
    fields["call"] = json!(0);
    let error = contract
        .iteration(&[internal(fields)], &BTreeMap::new(), &mut BTreeMap::new())
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("iteration lacks actual Leopard decode work")
    );
}
