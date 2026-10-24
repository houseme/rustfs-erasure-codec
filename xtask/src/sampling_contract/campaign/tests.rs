use super::*;
use crate::artifact::snapshot::{digest, tests::Directory};
use std::fs;

pub(crate) fn fixture() -> (Directory, Value) {
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
                .indexed()
                .unwrap();
                let map: BTreeMap<String, Value> = pairs.into_iter().collect();
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

pub(crate) fn originals(directory: &Directory, request: &mut Value) {
    for phase in STAGES {
        for profile in PROFILES {
            let filename = match profile {
                "smoke" => "smoke-results.json",
                "small_file" => "small-file-results.json",
                "hotspot" => "reconstruction-hotspot-results.json",
                _ => "stream-path-results.json",
            };
            let name = format!("{phase}/{filename}");
            let sample_path = request["document"]["files"][phase][profile]["samples"]["path"]
                .as_str()
                .unwrap();
            let records: Vec<Value> =
                serde_json::from_slice(&fs::read(directory.0.join(sample_path)).unwrap()).unwrap();
            let mut rows = records
                .iter()
                .step_by(12)
                .map(|r| r["rows"][0].clone())
                .collect::<Vec<_>>();
            rows.reverse(); // Deliberately retain a non-lexical first-seen case order.
            let bytes = serde_json::to_vec(&rows).unwrap();
            fs::write(directory.0.join(&name), &bytes).unwrap();
            request["document"]["files"][phase][profile]["path"] = json!(name);
            request["document"]["files"][phase][profile]["sha256"] = json!(digest(&bytes));
        }
    }
    request.as_object_mut().unwrap().remove("summarized");
}
#[test]
fn original_summaries_and_statistics_share_one_snapshot_and_preserve_case_order() {
    let (directory, mut request) = fixture();
    originals(&directory, &mut request);
    let result = replay(request.clone()).unwrap();
    assert_eq!(
        result["case_counts"],
        json!({"smoke":15,"small_file":88,"hotspot":8,"stream":15})
    );
    assert_eq!(
        result["analysis"]["repeat_checks"]
            .as_array()
            .unwrap()
            .len(),
        252
    );
    let raw: Value =
        serde_json::from_slice(&fs::read(directory.0.join("A1/smoke-results.json")).unwrap())
            .unwrap();
    let first = &raw[0];
    assert_eq!(
        result["analysis"]["repeat_checks"][0]["case"],
        format!(
            "{}:{}:{}:{}",
            first["operation"].as_str().unwrap(),
            first["data_shards"],
            first["parity_shards"],
            first["shard_size"]
        )
    );
    assert_eq!(
        result["layout_rows"]["A1"]["small_file"]
            .as_object()
            .unwrap()
            .len(),
        88
    );
    request["diagnostic"] = json!(true);
    let result = replay(request.clone()).unwrap();
    assert_eq!(
        result["analysis"]["sample_spreads"],
        result["sample_spreads"]
    );
    assert_eq!(
        result["analysis"]["unqualified_metric_comparisons"]
            .as_array()
            .unwrap()
            .len(),
        504
    );
    // A formal spread refusal must remain ahead of campaign ratio analysis.
    request["diagnostic"] = json!(false);
    let path = directory.0.join("A1/small_file-samples.json");
    let mut rows: Vec<Value> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for row in &mut rows {
        if row["case"] == "encode:4:2:1024" && row["index"].as_u64().unwrap() >= 9 {
            row["timed_ns"] = json!(400_000_000);
            row["rows"][0]["ns_per_iter"] = json!(400_000_000);
        }
    }
    let bytes = serde_json::to_vec(&rows).unwrap();
    fs::write(path, &bytes).unwrap();
    request["document"]["files"]["A1"]["small_file"]["samples"]["sha256"] = json!(digest(&bytes));
    let result = replay(request).unwrap();
    assert_eq!(result["unstable"], true);
    assert!(result.get("analysis").is_none());
}
#[test]
fn original_summary_declarations_and_selected_medians_remain_bound() {
    let (directory, mut request) = fixture();
    originals(&directory, &mut request);
    for (field, value) in [
        ("path", json!("../smoke-results.json")),
        ("sha256", json!("0".repeat(64))),
    ] {
        let mut bad = request.clone();
        bad["document"]["files"]["A1"]["smoke"][field] = value;
        assert!(replay(bad).is_err());
    }
    let mut bad = request.clone();
    bad["document"]["files"]["extra"] = json!({});
    assert!(replay(bad).is_err());
    let path = directory.0.join("B2/small-file-results.json");
    let mut rows: Vec<Value> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    rows[0]["ns_per_iter"] = json!(1);
    let bytes = serde_json::to_vec(&rows).unwrap();
    fs::write(path, &bytes).unwrap();
    request["document"]["files"]["B2"]["small_file"]["sha256"] = json!(digest(&bytes));
    assert!(replay(request).unwrap_err().to_string().contains("median"));
}
