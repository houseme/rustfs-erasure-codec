use super::*;
fn fixture() -> Value {
    let mut workers = BTreeMap::new();
    let mut events = Vec::new();
    for phase in PHASES {
        let mut profiles = BTreeMap::new();
        for profile in crate::sampling_contract::PROFILES {
            let mut jobs = Vec::new();
            for batch in 1..=2 {
                let job = json!({"case":format!("{profile}:case"),"iterations":batch,"calibration_batch":batch});
                for event in ["start", "end"] {
                    let mut row = json!({"phase":phase,"profile":profile,
                        "mode":if phase.starts_with("calibration-") {"calibrate"} else {"sample"},
                        "case":job["case"],"iterations":batch,"calibration_batch":batch,"event":event,
                        "monotonic_ns":events.len(),"utc_ns":events.len()});
                    if event == "end" {
                        row["exit_code"] = json!(0);
                    }
                    events.push(row.to_string());
                }
                jobs.push(job);
            }
            profiles.insert(profile, jobs);
        }
        workers.insert(phase, profiles);
    }
    json!({"raw":events.join("\n")+"\n","workers":workers})
}
fn run(value: Value) -> Result<Value> {
    serde_json::from_value::<Request>(value)?.execute()
}
#[test]
fn every_rehearsal_start_and_measurement_worker_is_matched_in_original_order() {
    let value = fixture();
    let before = value.clone();
    assert_eq!(run(value.clone()).unwrap(), 48);
    assert_eq!(value, before);
    let mut crlf = value.clone();
    crlf["raw"] = json!(value["raw"].as_str().unwrap().replace('\n', "\r\n"));
    assert_eq!(run(crlf).unwrap(), 48);
    let mut duplicate = value.clone();
    duplicate["raw"] = json!(value["raw"].as_str().unwrap().replacen(
        "\"iterations\":1",
        "\"iterations\":9,\"iterations\":1",
        1
    ));
    assert_eq!(run(duplicate).unwrap(), 48);
    let mut numeric = value.clone();
    let mut rows: Vec<Value> = value["raw"]
        .as_str()
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    for row in &mut rows {
        row["iterations"] = if row["iterations"] == 1 {
            json!(true)
        } else {
            json!(2.0)
        };
        row["calibration_batch"] = json!(row["calibration_batch"].as_f64().unwrap());
        row["monotonic_ns"] = json!(u64::MAX);
        row["utc_ns"] = json!(u64::MAX);
    }
    numeric["raw"] = json!(
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    );
    assert_eq!(run(numeric).unwrap(), 48);
}
#[test]
fn missing_reordered_overlapping_failed_or_extra_events_are_refused() {
    let value = fixture();
    let rows: Vec<Value> = value["raw"]
        .as_str()
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for mode in 0..8 {
        let mut bad = rows.clone();
        match mode {
            0 => {
                bad.pop();
            }
            1 => bad.swap(0, 2),
            2 => bad[3]["monotonic_ns"] = json!(0),
            3 => bad[1]["exit_code"] = json!(1),
            4 => bad[0]["utc_ns"] = json!(true),
            5 => bad[0]["extra"] = json!(0),
            6 => bad[0]["exit_code"] = json!(0),
            _ => bad.push(bad[0].clone()),
        }
        let mut request = value.clone();
        request["raw"] = json!(
            bad.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        );
        assert!(run(request).is_err(), "{mode}");
    }
}
#[test]
fn incomplete_profile_or_phase_catalogs_cannot_validate_a_shortened_journal() {
    for key in ["A1", "unknown"] {
        let mut value = fixture();
        if key == "unknown" {
            value["workers"][key] = json!({});
        } else {
            value["workers"].as_object_mut().unwrap().remove(key);
        }
        assert!(run(value).is_err());
    }
    let mut value = fixture();
    value["workers"]["A1"]
        .as_object_mut()
        .unwrap()
        .remove("hotspot");
    assert!(run(value).is_err());
}
