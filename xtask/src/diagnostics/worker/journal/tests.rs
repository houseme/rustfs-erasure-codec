use super::*;
use serde_json::json;
use std::fs;
fn fixture() -> (Vec<Value>, Value, String) {
    let rows:Vec<_> = (0..12).map(|index| json!({"case":"fixture","index":index,"iterations":2,"wall_ns":3,"timed_ns":2,"fixture":{},"workload":{},"placement":{},"before":{},"after":{}})).collect();
    let ledger = json!({"schema":"dynamic-buffer-lifecycle-v1","page_size":4096,"errors":0,"calls":[2,2,2,2,2,2,2,2,2,2,2,2],"release_approval":false,"events":[]});
    let mut raw = String::new();
    for row in &rows {
        raw += &format!("RSE_TIMING {row}\nRSE_SAMPLE {row}\n");
    }
    raw += &format!("RSE_DYNAMIC_BUFFERS {ledger}\n");
    (rows, ledger, raw)
}
#[test]
fn journal_preserves_actual_raw_numeric_types_and_original_data() {
    let (rows, ledger, raw) = fixture();
    let before = rows.clone();
    let path = std::env::temp_dir().join(format!("rse-native-journal-{}", std::process::id()));
    // The old Python comparison accepted equivalent JSON values; the worker
    // must still validate the actual raw samples, rather than substituted rows.
    let text = raw.replace("\"iterations\":2", "\"iterations\":2.0") + "RSE_STREAM_VIEWS {\n";
    fs::write(&path, &text).unwrap();
    let journal = Journal {
        path: path.clone(),
        sha256: snapshot::digest(text.as_bytes()),
    };
    let mut data: Data =
        serde_json::from_value(json!({"kind":"dynamic","ledger":ledger,"legacy":false})).unwrap();
    let actual = journal.worker(&rows, &mut data).unwrap();
    assert!(actual[0]["iterations"].is_f64());
    assert!(rows[0]["iterations"].is_u64());
    assert_eq!(rows, before);
    let missing = text.replacen("RSE_TIMING ", "IGNORED ", 1);
    fs::write(&path, &missing).unwrap();
    let changed = Journal {
        path: path.clone(),
        sha256: snapshot::digest(missing.as_bytes()),
    };
    assert!(changed.worker(&rows, &mut data).is_err());
    assert!(journal.worker(&rows, &mut data).is_err());
    fs::remove_file(path).unwrap();
}
#[test]
fn supplement_headers_and_ledgers_keep_last_key_and_numeric_equality() {
    let (_, ledger, _) = fixture();
    let header = json!({"constructor_budget":2});
    let data = serde_json::from_value(ledger.clone()).unwrap();
    let path = std::env::temp_dir().join(format!("rse-native-supplement-{}", std::process::id()));
    let raw = format!(
        "RSE_DYNAMIC_SUPPLEMENT {{\"constructor_budget\":1,\"constructor_budget\":2.0}}\nRSE_DYNAMIC_BUFFERS {ledger}\n"
    );
    fs::write(&path, &raw).unwrap();
    let journal = Journal {
        path: path.clone(),
        sha256: snapshot::digest(raw.as_bytes()),
    };
    journal.supplement(&header, &data).unwrap();
    assert!(
        journal
            .supplement(&json!({"constructor_budget":3}), &data)
            .is_err()
    );
    fs::remove_file(path).unwrap();
}
