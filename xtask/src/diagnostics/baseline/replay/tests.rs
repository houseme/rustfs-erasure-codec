use super::*;
use crate::artifact::snapshot::tests::Directory;

pub(crate) fn fixture() -> Directory {
    let directory = Directory::new();
    for name in ["calibration", "A1", "A2"] {
        fs::create_dir(directory.0.join(name)).unwrap();
    }
    for profile in PROFILES {
        let mut rows = sampling_contract::calibration_profile::tests::records(profile);
        for row in &mut rows {
            row["revision"] = json!(BASELINE);
            row["rows"][0]["git_revision"] = json!(&BASELINE[..12]);
        }
        fs::write(
            directory
                .0
                .join(format!("calibration/{profile}-samples.json")),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
        for row in &mut rows {
            row["kind"] = json!(if row["index"].as_u64().unwrap() < 3 {
                "warmup"
            } else {
                "sample"
            });
            row["calibration_batch"] = json!(0);
            row["complete"] = json!(true);
        }
        for phase in ["A1", "A2"] {
            fs::write(
                directory.0.join(format!("{phase}/{profile}-samples.json")),
                serde_json::to_vec(&rows).unwrap(),
            )
            .unwrap();
        }
    }
    let provenance = json!({"schema_version":"baseline-basic-controls-v1","source":{"commit":BASELINE},"tool":{"commit":"fixture"},
        "protocol":sampling_contract::protocol_value(),"renderer_sha256":"e".repeat(64),
        "context":{"isolation":{"cpu_affinity":[0,1,2,3,4,5,6,7]},"runtime":{"page_size":4096}}});
    fs::write(
        directory.0.join("provenance.json"),
        serde_json::to_vec(&provenance).unwrap(),
    )
    .unwrap();
    directory
}
fn service_case(runs: [u64; 9]) -> Value {
    let directory = fixture();
    let path = directory.0.join("A1/small_file-samples.json");
    let mut value: Value = crate::artifact::read_json(&path).unwrap();
    for row in value.as_array_mut().unwrap() {
        if row["case"] != "encode:4:2:1024" {
            continue;
        }
        let index = row["index"].as_u64().unwrap() as usize;
        let timed = match index {
            3 => 1_400_000_000_u64,
            4 => 1_200_000_000,
            _ => 1_000_000_000,
        };
        row["wall_ns"] = json!(timed + 1000);
        row["timed_ns"] = json!(timed);
        row["rows"][0]["ns_per_iter"] = json!(timed as f64);
        row["after"]["run_ns"] = json!(
            row["before"]["run_ns"].as_u64().unwrap()
                + if index < 3 { timed } else { runs[index - 3] }
        );
        row["after"]["wait_ns"] = json!(row["before"]["wait_ns"].as_u64().unwrap() + 100);
    }
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&path, &bytes).unwrap();
    let result = Request {
        capture: directory.0.clone(),
        renderer_sha256: "e".repeat(64),
        analyzer_sha256: "a".repeat(64),
    }
    .execute()
    .unwrap();
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(result["release_approval"], false);
    assert_eq!(result["input_sha256"].as_object().unwrap().len(), 13);
    result["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["profile"] == "small_file" && c["case"] == "encode:4:2:1024")
        .unwrap()["phases"]["A1"]
        .clone()
}
#[test]
fn full_file_replay_keeps_every_service_transition_and_the_original_wide_case() {
    let detail = service_case([
        1_400_000_000,
        1_200_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
    ]);
    assert_eq!(
        detail["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        (0..12).collect::<Vec<_>>()
    );
    assert_eq!(detail["central_spread"], 0.2);
    assert_eq!(detail["measured_service"]["main_run_central_spread"], 0.2);
    assert_eq!(detail["measured_service"]["total_process_minor_faults"], 0);
    assert!(
        detail["measured_service"]["wall_minus_timed_fraction_range"][1]
            .as_f64()
            .unwrap()
            < 0.000002
    );
}
#[test]
fn full_file_replay_ranks_service_independently_and_keeps_ratios_above_one() {
    let detail = service_case([
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_000_000_000,
        1_300_000_000,
        900_000_000,
        1_000_000_000,
        1_000_000_000,
    ]);
    assert_eq!(detail["central_spread"], 0.2);
    assert_eq!(detail["measured_service"]["main_run_central_spread"], 0.0);
    assert!(
        detail["measured_service"]["main_run_to_wall_range"][1]
            .as_f64()
            .unwrap()
            > 1.0
    );
}
#[test]
fn full_file_replay_retains_unknown_service_spread_for_zero_runtime() {
    let detail = service_case([0; 9]);
    assert!(detail["measured_service"]["main_run_central_spread"].is_null());
    assert_eq!(
        detail["measured_service"]["main_run_to_wall_range"],
        json!([0.0, 0.0])
    );
}
