use super::*;
use crate::artifact::snapshot::{digest, tests::Directory};
use std::fs;

fn fixture() -> (Directory, Value) {
    let directory = Directory::new();
    let revision = "a".repeat(40);
    let contexts: BTreeMap<_, _> = STAGES.into_iter().map(|phase| (phase,
        json!({"runtime":{"page_size":4096},"isolation":{"cpu_affinity":[0,1,2,3,4,5,6,7]}}))).collect();
    let mut document = json!({"sources":{"baseline":{"commit":revision},"candidate":{"commit":revision}},
        "contexts":contexts,"sampling":{"common_iterations":{}},"calibration":{},"files":{}});
    let mut summarized = json!({});
    let mut jobs = BTreeMap::new();
    for profile in PROFILES {
        let calibration = calibration_profile::tests::records(profile);
        let mut measured = calibration.clone();
        for row in &mut measured {
            row["kind"] = json!(if row["index"].as_u64().unwrap() < 3 {
                "warmup"
            } else {
                "sample"
            });
            row["calibration_batch"] = json!(0);
        }
        let counts: BTreeMap<_, _> = super::super::case_catalog()[profile]
            .iter()
            .map(|case| (case.clone(), 1))
            .collect();
        document["sampling"]["common_iterations"][profile] = json!(counts);
        for phase in [
            "calibration-baseline",
            "calibration-candidate",
            "A1",
            "B1",
            "B2",
            "A2",
        ] {
            let calibration_mode = phase.starts_with("calibration-");
            let rows = if calibration_mode {
                &calibration
            } else {
                &measured
            };
            let name = format!("{phase}/{profile}-samples.json");
            let path = directory.0.join(&name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let bytes = serde_json::to_vec(rows).unwrap();
            fs::write(path, &bytes).unwrap();
            let entry = json!({"path":name,"sha256":digest(&bytes)});
            if calibration_mode {
                document["calibration"][phase.trim_start_matches("calibration-")][profile] = entry;
            } else {
                document["files"][phase][profile]["samples"] = entry;
                let rows = rows
                    .iter()
                    .step_by(12)
                    .map(|row| row["rows"][0].clone())
                    .collect::<Vec<_>>();
                let pairs = release_rows::Request {
                    profile: profile.into(),
                    rows: json!(rows),
                    revision: revision.clone(),
                    iteration_counts: Some(counts.clone()),
                    default_iterations: None,
                    complete: true,
                }
                .execute()
                .unwrap();
                let map: BTreeMap<String, Value> =
                    serde_json::from_value::<Vec<(String, Value)>>(pairs)
                        .unwrap()
                        .into_iter()
                        .collect();
                summarized[phase][profile] = json!(map);
            }
            jobs.insert(
                (phase, profile),
                rows.iter()
                    .filter(|row| row["index"] == 0)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
    }
    let mut journal = String::new();
    let mut clock = 0;
    for phase in [
        "calibration-baseline",
        "calibration-candidate",
        "A1",
        "B1",
        "B2",
        "A2",
    ] {
        for profile in PROFILES {
            for row in &jobs[&(phase, profile)] {
                for event in ["start", "end"] {
                    clock += 1;
                    let mut value = json!({"phase":phase,"profile":profile,"mode":if phase.starts_with("calibration-") {"calibrate"} else {"sample"},
                        "case":row["case"],"iterations":row["iterations"],"calibration_batch":row["calibration_batch"],"event":event,"monotonic_ns":clock,"utc_ns":clock});
                    if event == "end" {
                        value["exit_code"] = json!(0);
                    }
                    journal.push_str(&value.to_string());
                    journal.push('\n');
                }
            }
        }
    }
    fs::write(directory.0.join("worker-journal.jsonl"), &journal).unwrap();
    document["worker_journal"] =
        json!({"path":"worker-journal.jsonl","sha256":digest(journal.as_bytes())});
    let request = json!({"capture":directory.0,"document":document,"summarized":summarized,"diagnostic":false});
    (directory, request)
}
fn replay(request: Value) -> Result<Value> {
    serde_json::from_value::<Request>(request)?.execute()
}
#[test]
fn complete_original_profiles_preserve_all_medians_spreads_and_worker_invocations() {
    let (directory, mut request) = fixture();
    let path = directory.0.join("A1/small_file-samples.json");
    // Original Python JSON uses final duplicate keys and integer negative zero.
    let text = fs::read_to_string(&path)
        .unwrap()
        .replacen("\"index\":0", "\"index\":17,\"index\":-0", 1)
        .replace("},{", "},\r\n{");
    fs::write(&path, &text).unwrap();
    request["document"]["files"]["A1"]["small_file"]["samples"]["sha256"] =
        json!(digest(text.as_bytes()));
    let result = replay(request.clone()).unwrap();
    assert_eq!(result["sample_spreads"].as_array().unwrap().len(), 504);
    assert_eq!(result["unstable"], false);
    assert!(result["worker_invocations"].is_null());
    request["diagnostic"] = json!(true);
    let diagnostic = replay(request).unwrap();
    assert_eq!(diagnostic["worker_invocations"], 756);
    assert_eq!(diagnostic["sample_spreads"].as_array().unwrap().len(), 504);
    assert_eq!(fs::read_to_string(path).unwrap(), text);
}
#[test]
fn declared_members_and_raw_samples_cannot_be_replaced_by_claimed_counts_or_summaries() {
    let (_directory, request) = fixture();
    for (field, value) in [
        ("path", json!("../samples.json")),
        ("sha256", json!(true)),
        ("sha256", json!("0".repeat(64))),
    ] {
        let mut invalid = request.clone();
        invalid["document"]["calibration"]["baseline"]["smoke"][field] = value;
        assert!(replay(invalid).is_err());
    }
    let mut invalid = request.clone();
    invalid["summarized"]["A1"]["small_file"]["encode:4:2:1024"]["ns_per_iter"] = json!(1);
    assert!(replay(invalid).unwrap_err().to_string().contains("median"));
    let mut invalid = request.clone();
    invalid["document"]["sampling"]["common_iterations"]["smoke"]["encode:4:2:65536"] = json!(2);
    assert!(
        replay(invalid)
            .unwrap_err()
            .to_string()
            .contains("common iterations")
    );
    let mut invalid = request;
    invalid["diagnostic"] = json!(true);
    invalid["document"]["worker_journal"]["sha256"] = json!("0".repeat(64));
    assert!(replay(invalid).unwrap_err().to_string().contains("digest"));
}
