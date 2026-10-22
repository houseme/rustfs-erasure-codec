//! Artifact and CLI contracts retained when removing the Python entry points.
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rse-xtask-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn command(&self, command: &str) -> Command {
        let mut process = Command::new(env!("CARGO_BIN_EXE_xtask"));
        process.arg(command).current_dir(&self.0);
        process
    }
    fn analyze(&self, input: &str, threshold: &str) -> (Output, Value, String) {
        fs::write(self.0.join("input.csv"), input).unwrap();
        fs::write(
            self.0.join("summary.json"),
            r#"{"attribution_allowed":true}"#,
        )
        .unwrap();
        fs::write(self.0.join("summary.md"), "stale PASS report").unwrap();
        let output = self
            .command("analyze-abba")
            .args([
                "--csv",
                "input.csv",
                "--summary-json",
                "summary.json",
                "--summary-md",
                "summary.md",
                "--drift-threshold",
                threshold,
            ])
            .output()
            .unwrap();
        (
            output,
            serde_json::from_slice(&fs::read(self.0.join("summary.json")).unwrap()).unwrap(),
            fs::read_to_string(self.0.join("summary.md")).unwrap(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stream_view_cli_checks_a_complete_cursor_ledger_without_python_or_rewriting_inputs() {
    let f = Fixture::new();
    let mut events = Vec::new();
    for (call, role) in [
        (0, "retained_source"),
        (1, "cursor_before_missing"),
        (1, "cursor_missing"),
        (1, "cursor_after_recovery"),
    ] {
        for index in 0..2 {
            let size = if role == "cursor_missing" { 0 } else { 1024 };
            events.push(json!({"call":call,"role":role,"index":index,"len":size,"capacity":size,
                "offsets":if size>0 {json!([16,16,16])} else {Value::Null},"bytes":0,"same_address_as_previous_view":null}));
        }
    }
    let residency:Vec<_>=events.iter().map(|e|json!({"call":e["call"],"boundary":"view","role":e["role"],"index":e["index"],"bytes":e["len"],"offset_page":if e["len"]==0 {Value::Null} else {json!(16)},
        "snapshot":{"status":if e["len"]==0 {"empty"} else {"mapped"},"pages":if e["len"]==0 {0} else {1},"resident_pages":if e["len"]==0 {0} else {1},"errno":null}})).collect();
    let mut request = json!({"operation":"validate","summary":true,"observations":[{"case":"reconstruct_stream:1:1:1024:1024","iterations":1,"page_size":4096,"observation":0,
        "view":{"schema":"stream-buffer-views-v3","observation":0,"iterations":1,"calls":1,"completed":1,"page_size":4096,"overflow":0,"release_approval":false,
        "events":events,"residency":residency,"windows":[{"call":1,"phase":"call","index":null,"bytes":0,"start_ns":0,"end_ns":10,"before":null,"after":null}]}}]});
    let bytes = serde_json::to_vec(&request).unwrap();
    fs::write(f.0.join("request.json"), &bytes).unwrap();
    let output = f
        .command("validate-stream-views")
        .args(["--input", "request.json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value[0][0]["windows"]["call"]["wall_ns"], 10);
    assert!(value[0][0]["windows"]["call"]["process_minor_faults"].is_null());
    assert_eq!(fs::read(f.0.join("request.json")).unwrap(), bytes);
    request["observations"][0]["view"]["events"][0]["address"] = json!(123);
    fs::write(
        f.0.join("request.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    let output = f
        .command("validate-stream-views")
        .args(["--input", "request.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn smoke_report_keeps_plain_and_legacy_rows_separate_from_ab_and_profile_inputs() {
    let f = Fixture::new();
    let inputs = [
        (
            "leopard-encode-tiny.json",
            json!({"artifact_kind":"leopard-encode","case":"tiny","shard_size":1024,"throughput_mb_s":123.5}),
        ),
        (
            "leopard-encode-large.json",
            json!({"artifact_kind":"leopard-encode","case":"large","shard_size":1048576,"throughput_mb_s":1000.25}),
        ),
        (
            "leopard-encode-legacy.json",
            json!({"throughput_mb_s":12.5}),
        ),
        (
            "leopard-encode-ab-large.json",
            json!([{"artifact_kind":"leopard-encode-ab","case":"ab-only","throughput_mb_s":777777.0}]),
        ),
        (
            "leopard-encode-profile-large.json",
            json!({"artifact_kind":"leopard-encode-profile","case":"profile-only","shard_size":1048576,"throughput_mb_s":888888.0}),
        ),
        (
            "smoke-results.json",
            json!([{"backend":"scalar","backend_id":"scalar","target_triple":"recorded-target"}]),
        ),
    ];
    for (name, value) in &inputs {
        fs::write(f.0.join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }
    let output = f
        .command("summarize-smoke")
        .args(["--input", "."])
        .env("RSE_DIT4_STRATEGY", "direct-safe")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("legacy: shard_size=unknown, 12.50 MB/s"));
    assert!(text.contains("tiny: shard_size=1K, 123.50 MB/s"));
    assert!(text.contains("large: shard_size=1024K, 1000.25 MB/s"));
    assert!(text.find("tiny:").unwrap() < text.find("large:").unwrap());
    assert!(!text.contains("profile-only") && !text.contains("ab-only"));
    assert!(text.contains("backend: scalar") && text.contains("target: recorded-target"));
    assert!(text.contains("未记录实际策略"));
    assert!(!text.contains("→ direct") && !text.contains("→ decomposed"));
    for (name, value) in inputs {
        let current: Value = serde_json::from_slice(&fs::read(f.0.join(name)).unwrap()).unwrap();
        assert_eq!(current, value);
    }
}

#[test]
fn smoke_report_rejects_invalid_json_or_metrics_before_printing_partial_results() {
    let f = Fixture::new();
    fs::write(
        f.0.join("leopard-encode-first.json"),
        r#"{"throughput_mb_s":100}"#,
    )
    .unwrap();
    for invalid in [
        r#"{"throughput_mb_s":1,"throughput_mb_s":2}"#,
        r#"{"throughput_mb_s":NaN}"#,
        r#"{"throughput_mb_s":-1}"#,
        r#"{"throughput_mb_s":true}"#,
        r#"{"throughput_mb_s":0}"#,
        r#"{"throughput_mb_s":1,"shard_size":0}"#,
        r#"{"throughput_mb_s":1,"case":""}"#,
        r#"{}"#,
    ] {
        fs::write(f.0.join("leopard-encode-invalid.json"), invalid).unwrap();
        let output = f
            .command("summarize-smoke")
            .args(["--input", "."])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "accepted {invalid}");
        assert!(output.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(f.0.join("leopard-encode-invalid.json")).unwrap(),
            invalid
        );
    }
}

#[test]
fn smoke_report_handles_empty_input_without_inventing_observations_or_backend() {
    let f = Fixture::new();
    let output = f
        .command("summarize-smoke")
        .args(["--input", "missing"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("未找到普通 encode 记录"));
    assert!(text.contains("backend: unknown"));
    assert!(!f.0.join("missing").exists());
}

#[test]
fn sampling_contract_cli_exposes_the_full_catalog_and_rejects_duplicate_json_keys() {
    let f = Fixture::new();
    let bytes = br#"{"operation":"catalog"}"#;
    fs::write(f.0.join("request.json"), bytes).unwrap();
    let output = f
        .command("sampling-contract")
        .args(["--input", "request.json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        catalog["cases"]
            .as_object()
            .unwrap()
            .values()
            .map(|v| v.as_array().unwrap().len())
            .sum::<usize>(),
        126
    );
    assert_eq!(catalog["service_window"]["minimum_timed_ns"], 40_000_000);
    assert_eq!(fs::read(f.0.join("request.json")).unwrap(), bytes);
    fs::write(
        f.0.join("request.json"),
        br#"{"operation":"catalog","operation":"catalog"}"#,
    )
    .unwrap();
    let output = f
        .command("sampling-contract")
        .args(["--input", "request.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate"));
}

#[test]
fn memory_policy_cli_retains_nullable_interfaces_and_never_modifies_inputs() {
    let f = Fixture::new();
    let value = json!({"kernel_release":"fixture-linux","process_thp_disable":3,"thp":{"enabled":"madvise","defrag":"defer+madvise","shmem_enabled":null,"sizes_kib":{"16":{"anonymous":"inherit","shmem":null}}}});
    let input = serde_json::to_vec(&value).unwrap();
    fs::write(f.0.join("policy.json"), &input).unwrap();
    let output = f
        .command("memory-policy")
        .args(["--input", "policy.json", "--base-page-size", "4096"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        value
    );
    let output = f
        .command("memory-policy")
        .args(["--input", "policy.json", "--base-page-size", "16384"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mTHP size"));
    assert_eq!(fs::read(f.0.join("policy.json")).unwrap(), input);
}

#[test]
fn memory_policy_cli_rejects_ambiguous_or_incomplete_evidence() {
    let f = Fixture::new();
    for text in [
        r#"{"kernel_release":"x","kernel_release":"y"}"#,
        r#"{"kernel_release":"x","process_thp_disable":false,"thp":{}}"#,
        r#"{"kernel_release":"x","process_thp_disable":0,"thp":{"enabled":"never","defrag":"never","sizes_kib":{}}}"#,
        r#"{"kernel_release":"x","process_thp_disable":0,"thp":{"enabled":"never","defrag":"never","shmem_enabled":null,"sizes_kib":{"16":{"anonymous":"inherit"}}}}"#,
        "NaN",
    ] {
        fs::write(f.0.join("policy.json"), text).unwrap();
        let output = f
            .command("memory-policy")
            .args(["--input", "policy.json"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{text}");
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read_to_string(f.0.join("policy.json")).unwrap(), text);
    }
}

#[test]
fn native_sampler_accepts_crlf_and_cr_sources_like_the_original_python_reader() {
    let f = Fixture::new();
    let tool = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let names = [
        "tests/benchmark_smoke.rs",
        "tests/benchmark_small_files.rs",
        "tests/benchmark_stream_paths.rs",
        "src/tests/mod.rs",
    ];
    let request = json!({"kind":"sampler-render","root":f.0,"tool":tool});
    fs::write(f.0.join("request.json"), request.to_string()).unwrap();
    let mut first = None;
    for ending in ["\n", "\r\n", "\r"] {
        for name in names {
            let path = f.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let text = fs::read_to_string(tool.join(name))
                .unwrap()
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            fs::write(path, text.replace('\n', ending)).unwrap();
        }
        let output = f
            .command("native-helper")
            .args(["--input", "request.json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{ending:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let files: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(files.as_object().unwrap().len(), 11);
        if let Some(expected) = &first {
            assert_eq!(&files, expected, "{ending:?}");
        } else {
            first = Some(files);
        }
    }
}

const HEADER: &str =
    "stage,commit,backend_override,criterion_group,operation,length_bytes,mean_ns\n";
fn rows(values: [f64; 4], operation: &str) -> String {
    ["A1", "B1", "B2", "A2"].iter().zip(values).map(|(stage, value)| {
        let commit = if stage.starts_with('A') { "aaaa111" } else { "bbbb222" };
        format!("{stage},{commit},auto,galois_{operation}_neon_override_auto,{operation},65536,{value}\n")
    }).collect()
}
fn campaign() -> String {
    HEADER.to_owned() + &rows([100., 90., 91., 101.], "mul_slice")
}
fn failed(output: Output, report: Value, markdown: String) {
    assert!(!output.status.success());
    assert_eq!(report["attribution_allowed"], false);
    assert_eq!(report["drift_failed"], true);
    assert!(
        report["cases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["candidate_delta_vs_a_mean"].is_null())
    );
    assert!(markdown.contains("WITHHELD for the entire campaign"));
    assert!(!markdown.contains("stale PASS"));
}

#[test]
fn complete_campaigns_preserve_groups_metrics_and_drift_boundary() {
    let f = Fixture::new();
    let source = campaign() + &rows([200., 100., 100., 200.], "mul_slice_xor");
    let (output, report, markdown) = f.analyze(&source, "0.05");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report["cases"].as_array().unwrap().len(), 2);
    assert_eq!(report["cases"][0]["baseline_drift"], 0.01);
    assert_eq!(report["cases"][1]["candidate_delta_vs_a_mean"], -0.5);
    assert!(markdown.contains("Candidate attribution: PERMITTED"));
    let source = HEADER.to_owned() + &rows([100., 100., 95., 105.], "mul_slice");
    assert!(f.analyze(&source, "0.05").0.status.success());
    assert!(!f.analyze(&source, "0.049").0.status.success());
}

#[test]
fn any_repeat_failure_revokes_attribution_for_the_whole_campaign() {
    let f = Fixture::new();
    for values in [
        [100., 80., 80., 106.],
        [100., 80., 85., 100.],
        [100., 80., 74., 100.],
        [100., 80., 80., 94.],
    ] {
        let source = HEADER.to_owned()
            + &rows(values, "mul_slice")
            + &rows([200., 100., 100., 200.], "mul_slice_xor");
        let (output, report, markdown) = f.analyze(&source, "0.05");
        assert_eq!(report["cases"].as_array().unwrap().len(), 2);
        assert_eq!(report["drift_failures"], 1);
        failed(output, report, markdown);
    }
}

#[test]
fn invalid_thresholds_replace_old_pass_artifacts() {
    let f = Fixture::new();
    for threshold in ["nan", "inf", "-inf", "-0.01", "bad"] {
        let (output, report, markdown) = f.analyze(&campaign(), threshold);
        assert!(
            report["validation_errors"]
                .to_string()
                .contains("threshold must be finite and nonnegative")
        );
        failed(output, report, markdown);
    }
}

#[test]
fn malformed_csv_headers_rows_and_incomplete_stages_fail_closed() {
    let f = Fixture::new();
    for input in [
        "",
        HEADER,
        "stage,stage,commit,criterion_group,operation,length_bytes,mean_ns\n",
        "stage,commit\n",
        "\"unterminated\n",
    ] {
        let (o, r, m) = f.analyze(input, "0.05");
        failed(o, r, m);
    }
    for stage in ["A1", "B1", "B2", "A2"] {
        let input = campaign()
            .lines()
            .filter(|line| !line.starts_with(stage))
            .collect::<Vec<_>>()
            .join("\n");
        let (o, r, m) = f.analyze(&input, "0.05");
        failed(o, r, m);
    }
    let duplicate = campaign() + campaign().lines().nth(1).unwrap();
    let (o, r, m) = f.analyze(&duplicate, "0.05");
    failed(o, r, m);
}

#[test]
fn invalid_or_inconsistent_provenance_and_metrics_are_rejected() {
    let f = Fixture::new();
    for (from, to) in [
        ("A1,aaaa111", "A3,aaaa111"),
        ("B2,bbbb222", "B2,cccc333"),
        ("A1,aaaa111,auto", "A1,aaaa111,scalar"),
        (
            "galois_mul_slice_neon_override_auto",
            "galois_mul_slice_unknown",
        ),
        ("mul_slice,65536", "mul_slice_xor,65536"),
    ] {
        let (o, r, m) = f.analyze(&campaign().replace(from, to), "0.05");
        failed(o, r, m);
    }
    for bad in [
        "",
        "unknown",
        "main",
        "origin/main",
        "HEAD",
        "abc",
        "0000000",
        &"f".repeat(41),
        &"f".repeat(63),
    ] {
        let (o, r, m) = f.analyze(&campaign().replace("aaaa111", bad), "0.05");
        failed(o, r, m);
    }
    for bad in ["", "unknown", "none", "n/a", "auto other"] {
        let (o, r, m) = f.analyze(&campaign().replace(",auto,", &format!(",{bad},")), "0.05");
        failed(o, r, m);
    }
    for bad in ["0", "-1", "nan", "inf", "-inf", "true", "bad", ""] {
        for (from, to) in [
            ("65536,100", format!("65536,{bad}")),
            ("65536,100", format!("{bad},100")),
        ] {
            let (o, r, m) = f.analyze(&campaign().replace(from, &to), "0.05");
            failed(o, r, m);
        }
    }
}

#[test]
fn legacy_override_and_sha256_revision_case_remain_compatible() {
    let f = Fixture::new();
    let source = campaign()
        .replace(",backend_override", "")
        .replace(",auto,", ",");
    assert!(f.analyze(&source, "0.05").0.status.success());
    let source = campaign()
        .replace("aaaa111", &"A".repeat(64))
        .replace("bbbb222", &"B".repeat(64));
    let (output, report, _) = f.analyze(&source, "0.05");
    assert!(output.status.success());
    assert_eq!(report["commits"]["A1"], "a".repeat(64));
}

#[test]
fn extractor_roundtrip_filters_stale_results_and_preserves_csv_and_jsonl() {
    let f = Fixture::new();
    let estimate =
        f.0.join("criterion/galois_mul_slice_neon_override_auto/len_65536/new/estimates.json");
    fs::create_dir_all(estimate.parent().unwrap()).unwrap();
    for (i, stage) in ["A1", "B1", "B2", "A2"].iter().enumerate() {
        let value = [100., 90., 91., 101.][i];
        fs::write(&estimate, json!({"mean":{"point_estimate":value,"confidence_interval":{"lower_bound":value*0.99,"upper_bound":value*1.01}}}).to_string()).unwrap();
        let commit = if stage.starts_with('A') {
            "aaaa111"
        } else {
            "bbbb222"
        };
        let mut process = f.command("extract-criterion");
        process.args([
            "--criterion-root",
            "criterion",
            "--stage",
            stage,
            "--commit",
            commit,
            "--backend-override",
            "auto",
            "--csv",
            "estimates.csv",
            "--jsonl",
            "estimates.jsonl",
            "--updated-after-epoch",
            "0",
        ]);
        if i == 0 {
            process.arg("--write-header");
        }
        let output = process.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let csv = fs::read_to_string(f.0.join("estimates.csv")).unwrap();
    assert!(f.analyze(&csv, "0.05").0.status.success());
    let jsonl = fs::read_to_string(f.0.join("estimates.jsonl")).unwrap();
    let values: Vec<Value> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(values.len(), 4);
    assert_eq!(values[0]["length_bytes"], 65536);
    let output = f
        .command("extract-criterion")
        .args([
            "--criterion-root",
            "criterion",
            "--stage",
            "A1",
            "--commit",
            "aaaa111",
            "--backend-override",
            "auto",
            "--csv",
            "unused.csv",
            "--jsonl",
            "unused.jsonl",
            "--updated-after-epoch",
            "99999999999",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!f.0.join("unused.csv").exists());
}

const PERF_SOURCE: &str = "fn prepare_read_buffer() {\n    core::cmp::Ordering::Less => buf.resize(max_len, 0),\n}\npub fn encode_stream() {\n        buf.resize(actual_len, 0);\n    write_block_with_mode(\n    }\npub fn verify_stream() {}\n";
fn perf(f: &Fixture, text: &str, records: Value, log: &str) -> Output {
    for (name, value) in [
        ("perf.txt", text.to_owned()),
        ("records.json", json!({"record_types":records}).to_string()),
        ("source.rs", PERF_SOURCE.to_owned()),
        ("record.log", log.to_owned()),
    ] {
        fs::write(f.0.join(name), value).unwrap();
    }
    f.command("perf-stack-audit")
        .args([
            "--input",
            "perf.txt",
            "--records",
            "records.json",
            "--record-log",
            "record.log",
            "--record-exit-code",
            "0",
            "--source",
            "source.rs",
            "--event",
            "minor-faults:u",
            "--output",
            "inventory.json",
        ])
        .output()
        .unwrap()
}

#[test]
fn perf_cli_preserves_periods_sources_frames_and_existing_files() {
    let f = Fixture::new();
    let text = "1.000001: 1 minor-faults:u:\n\t abcdef [unknown] (/private/lib/libc.so)\n  ??:0\n\t 123456 alloc::raw_vec::RawVec::grow\n  /private/library/alloc/src/vec/mod.rs:345 (inlined)\n\t 001122 resize<u8>\n  /private/project/src/core/stream.rs:5 (inlined)\n\n1.000002: 1009 minor-faults:u:\n\n";
    let output = perf(
        &f,
        text,
        json!({"PERF_RECORD_SAMPLE":2}),
        "Captured and wrote",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = f.0.join("inventory.json");
    let original = fs::read_to_string(&path).unwrap();
    let report: Value = serde_json::from_str(&original).unwrap();
    assert_eq!(report["samples"], 2);
    assert_eq!(report["period_sum"], 1010);
    assert_eq!(report["empty_callchain_samples"], 1);
    assert_eq!(
        report["frames"][1]["source"],
        "library/alloc/src/vec/mod.rs"
    );
    assert!(report["frames"][1]["module"].is_null());
    assert_eq!(
        report["roles"]["parity_scratch_initialization"]["samples"],
        1
    );
    assert_eq!(report["recording_quality"]["returncode"], 0);
    for private in ["/private/", "abcdef", "123456", "001122"] {
        assert!(!original.contains(private));
    }
    assert!(
        !perf(
            &f,
            text,
            json!({"PERF_RECORD_SAMPLE":2}),
            "Captured and wrote"
        )
        .status
        .success()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[test]
fn perf_cli_rejects_invalid_counts_corruption_and_counter_overflow_without_output() {
    let f = Fixture::new();
    for counts in [
        json!({"PERF_RECORD_SAMPLE":true}),
        json!({"PERF_RECORD_SAMPLE":0}),
        json!({"PERF_RECORD_SAMPLE":1,"PERF_RECORD_COMM":-1}),
        json!({"PERF_RECORD_SAMPLE":1,"PERF_RECORD_LOST":1}),
    ] {
        assert!(
            !perf(
                &f,
                "1.000001: 1 minor-faults:u:\n\n",
                counts,
                "Captured and wrote"
            )
            .status
            .success()
        );
        assert!(!f.0.join("inventory.json").exists());
    }
    for log in [
        "",
        "Captured and wrote\nlost 2 chunks!",
        "Captured and wrote\nData corruption detected",
    ] {
        assert!(
            !perf(
                &f,
                "1.000001: 1 minor-faults:u:\n\n",
                json!({"PERF_RECORD_SAMPLE":1}),
                log
            )
            .status
            .success()
        );
    }
    let text = format!(
        "1.000001: {} minor-faults:u:\n\n1.000002: 1 minor-faults:u:\n\n",
        u64::MAX
    );
    let output = perf(
        &f,
        &text,
        json!({"PERF_RECORD_SAMPLE":2}),
        "Captured and wrote",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("overflow"));
    assert!(!f.0.join("inventory.json").exists());
}

#[test]
fn machine_report_preserves_schema_scores_overrides_and_raw_array_archives() {
    let f = Fixture::new();
    let smoke = f.0.join("target/benchmark-smoke");
    fs::create_dir_all(&smoke).unwrap();
    for (backend, value) in [("scalar", 100), ("rust-avx2", 200), ("auto", 200)] {
        let mut rows = String::from(
            "operation,data_shards,parity_shards,shard_size,backend_override,backend,override_honored,throughput_mb_s\n",
        );
        for op in ["encode", "verify", "reconstruct", "reconstruct_data"] {
            rows += &format!("{op},10,4,1048576,{backend},{backend},true,{value}\n");
        }
        rows += &format!("encode,10,4,1048576,{backend},{backend},false,9999\n");
        fs::write(
            smoke.join(format!("smoke-results-release-{backend}.csv")),
            rows,
        )
        .unwrap();
    }
    fs::write(
        smoke.join("smoke-results-release-unsupported.csv"),
        "malformed,ignored\n",
    )
    .unwrap();
    let args = [
        "--root",
        ".",
        "--machine-json",
        "machine.json",
        "--machine-slug",
        "fixture",
        "--date",
        "2026-10-22",
    ];
    let output = f.command("summarize-simd").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(f.0.join("machine.json")).unwrap()).unwrap();
    assert_eq!(
        report["recommended_default_priority"]["priority_order"],
        json!(["rust-avx2", "scalar"])
    );
    assert_eq!(
        report["rankings_10x4_1m"]["encode"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        report["adoption_decision_stub"]["override_mismatch_count"],
        3
    );
    assert_eq!(
        report["adoption_decision_stub"]["status"],
        "manual-review-required"
    );
    assert_eq!(report["release_smoke"].as_object().unwrap().len(), 3);
    let archive = f.0.join("benchmarks/x86_64-simd");
    fs::create_dir_all(&archive).unwrap();
    fs::copy(f.0.join("machine.json"), archive.join("machine.json")).unwrap();
    fs::write(archive.join("raw.json"), "[]").unwrap();
    fs::write(archive.join("machine.run-meta.json"), "{}").unwrap();
    let output = f
        .command("summarize-simd")
        .arg("--summary")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let archived: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(archived.as_array().unwrap().len(), 1);
    assert_eq!(archived[0]["encode"], "auto");
}

fn benchmark_row() -> Value {
    json!({"operation":"encode","data_shards":4,"parity_shards":2,"shard_size":1024,"throughput_mb_s":100,"ns_per_iter":1000})
}
fn run_gate(f: &Fixture, command: &str, baseline: Value, current: Value, extra: &[&str]) -> Output {
    fs::write(f.0.join("baseline.json"), baseline.to_string()).unwrap();
    fs::write(f.0.join("current.json"), current.to_string()).unwrap();
    f.command(command)
        .args(["--baseline", "baseline.json", "--current", "current.json"])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn benchmark_gate_preserves_failures_thresholds_case_requirements_and_legacy_metadata() {
    let f = Fixture::new();
    let row = benchmark_row();
    let output = run_gate(&f, "check-benchmark", json!([row]), json!([row]), &[]);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        !report["unchecked_provenance"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut slower = row.clone();
    slower["throughput_mb_s"] = 80.into();
    slower["ns_per_iter"] = 1200.into();
    for metric in ["throughput_mb_s", "ns_per_iter"] {
        let out = run_gate(
            &f,
            "check-benchmark",
            json!([row]),
            json!([slower]),
            &["--metric", metric],
        );
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["failures"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    assert!(
        run_gate(
            &f,
            "check-benchmark",
            json!([row]),
            json!([slower]),
            &["--threshold", "encode=0.25"]
        )
        .status
        .success()
    );
    let mut dropped = row.clone();
    dropped["operation"] = "reconstruct".into();
    assert!(
        !run_gate(
            &f,
            "check-benchmark",
            json!([row, dropped]),
            json!([row]),
            &[]
        )
        .status
        .success()
    );
    assert!(
        run_gate(
            &f,
            "check-benchmark",
            json!([row, dropped]),
            json!([row]),
            &["--allow-missing-cases"]
        )
        .status
        .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-benchmark",
            json!([row, dropped]),
            json!([row]),
            &[
                "--allow-missing-cases",
                "--require-case",
                "reconstruct:4:2:1024"
            ]
        )
        .status
        .success()
    );
    let mut specified = row.clone();
    specified["backend"] = "rust-avx2".into();
    assert!(
        !run_gate(&f, "check-benchmark", json!([row]), json!([specified]), &[])
            .status
            .success()
    );
    assert!(
        run_gate(
            &f,
            "check-benchmark",
            json!([row]),
            json!([specified]),
            &["--allow-missing-provenance"]
        )
        .status
        .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-benchmark",
            json!([row]),
            json!([specified]),
            &["--strict-provenance", "--allow-missing-provenance"]
        )
        .status
        .success()
    );
}

#[test]
fn regression_gates_validate_all_inputs_and_available_provenance() {
    let f = Fixture::new();
    let row = benchmark_row();
    for bad in [json!([]), json!({}), json!([null]), json!([row, row])] {
        assert!(
            !run_gate(&f, "check-benchmark", bad.clone(), json!([row]), &[])
                .status
                .success()
        );
        assert!(
            !run_gate(&f, "check-benchmark", json!([row]), bad, &[])
                .status
                .success()
        );
    }
    for field in [
        "backend",
        "cpu_model",
        "compiler",
        "features",
        "build_profile",
        "profile",
        "target_triple",
    ] {
        let mut before = row.clone();
        before[field] = "value-a".into();
        let mut after = row.clone();
        after[field] = "value-b".into();
        let out = run_gate(
            &f,
            "check-benchmark",
            json!([before]),
            json!([after]),
            &["--allow-missing-provenance"],
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stdout).contains("incompatible provenance"));
    }
    for bad in ["nan", "inf", "-1", "bad"] {
        assert!(
            !run_gate(
                &f,
                "check-benchmark",
                json!([row]),
                json!([row]),
                &["--threshold", &format!("encode={bad}")]
            )
            .status
            .success()
        );
    }
    let mut extra = row.clone();
    extra["shard_size"] = 2048.into();
    extra["ns_per_iter"] = true.into();
    assert!(
        !run_gate(
            &f,
            "check-benchmark",
            json!([row]),
            json!([row, extra]),
            &[]
        )
        .status
        .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-benchmark",
            json!([row]),
            json!([row]),
            &["--metric", "ns_per_block"]
        )
        .status
        .success()
    );
}

#[test]
fn benchmark_gate_keeps_json_csv_aliases_automatic_budgets_and_stream_dimensions() {
    let f = Fixture::new();
    let a = json!({"operation":"encode_stream","data_shards":4,"parity_shards":2,"shard_size":1024,"stream_block_size":4096,"throughput_mb_s":100,"ns_per_iter":1000,"ns_per_block":250,
        "target_triple":"x86_64-unknown-linux-gnu","features":"std|simd-accel","benchmark_metrics_enabled":false,"backend":"rust-avx2","cpu_model":"Example CPU 16C",
        "compiler":"rustc 1.98.1 (012345abc 2026-09-10)","build_profile":"release","rayon_threads":4,"available_parallelism":16,"max_parallel_jobs":0,"stream_io_backend":"memory","stream_io_mode":"serial"});
    fs::write(f.0.join("baseline.json"), json!([a]).to_string()).unwrap();
    let mut b = a.as_object().unwrap().clone();
    for (canonical, alias) in [
        ("backend", "backend_name"),
        ("cpu_model", "cpu"),
        ("compiler", "rustc_version"),
        ("build_profile", "cargo_profile"),
        ("rayon_threads", "rayon_num_threads"),
        ("max_parallel_jobs", "max_jobs"),
    ] {
        let value = b.remove(canonical).unwrap();
        b.insert(alias.into(), value);
    }
    b.insert("cpu".into(), " Example  CPU 16C ".into());
    b.insert(
        "rustc_version".into(),
        "1.98.1 (012345abc 2026-09-10)".into(),
    );
    b.insert("features".into(), "simd-accel|std|std".into());
    b.insert("max_jobs".into(), "000".into());
    let mut writer = csv::Writer::from_path(f.0.join("current.csv")).unwrap();
    writer.write_record(b.keys()).unwrap();
    writer
        .write_record(
            b.values()
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned)),
        )
        .unwrap();
    writer.flush().unwrap();
    let out = f
        .command("check-benchmark")
        .args([
            "--baseline",
            "baseline.json",
            "--current",
            "current.csv",
            "--strict-provenance",
            "--metric",
            "ns_per_block",
            "--require-case",
            "encode_stream:4:2:1024:4096",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["comparisons"][0]["regression_ratio"], 0.0);
}

#[test]
fn hotspot_gate_covers_all_scenarios_and_enforces_provenance_and_floors() {
    let f = Fixture::new();
    let row = json!({"scenario":"recover","candidate_mb_s":100,"baseline_mb_s":80,"speedup":1.25});
    let mut slow = row.clone();
    slow["scenario"] = "other".into();
    slow["candidate_mb_s"] = 50.into();
    let mut other = row.clone();
    other["scenario"] = "other".into();
    assert!(
        run_gate(&f, "check-hotspot", json!([row]), json!([row]), &[])
            .status
            .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-hotspot",
            json!([row, other]),
            json!([row, slow]),
            &["--require-scenario", "recover"]
        )
        .status
        .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-hotspot",
            json!([row]),
            json!([row]),
            &["--require-scenario", "absent"]
        )
        .status
        .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-hotspot",
            json!([row]),
            json!([row]),
            &["--min-speedup", "recover=2"]
        )
        .status
        .success()
    );
    assert!(
        !run_gate(
            &f,
            "check-hotspot",
            json!([row]),
            json!([row]),
            &["--min-speedup", "recover=1", "--min-speedup", "recover=2"]
        )
        .status
        .success()
    );
    let mut mismatch = row.clone();
    mismatch["cpu_model"] = "different".into();
    assert!(
        !run_gate(&f, "check-hotspot", json!([row]), json!([mismatch]), &[])
            .status
            .success()
    );
    let mut invalid = row.clone();
    invalid["speedup"] = "nan".into();
    assert!(
        !run_gate(&f, "check-hotspot", json!([row]), json!([invalid]), &[])
            .status
            .success()
    );
}

#[cfg(unix)]
fn telemetry_fixture(f: &Fixture, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    for path in ["tests", "src/tests", "scripts/diagnostics", "bin"] {
        fs::create_dir_all(f.0.join(path)).unwrap();
    }
    fs::write(
        f.0.join("src/tests/mod.rs"),
        "fn shard_index_pattern() {\n}\n",
    )
    .unwrap();
    for name in [
        "telemetry_allocations.rs",
        "stream_view_allocations.rs",
        "verify_scratch_allocations.rs",
        "stream_core_allocations.rs",
        "dynamic_buffer_allocations.rs",
    ] {
        fs::write(
            f.0.join("scripts/diagnostics").join(name),
            "// fixture counter",
        )
        .unwrap();
    }
    let path = f.0.join("bin/cargo");
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
#[cfg(unix)]
fn telemetry_command(f: &Fixture) -> Command {
    let mut command = f.command("check-sampler-telemetry");
    command.args(["--root", f.0.to_str().unwrap()]);
    command.env(
        "PATH",
        format!(
            "{}:{}",
            f.0.join("bin").display(),
            std::env::var("PATH").unwrap()
        ),
    );
    command
}
#[cfg(unix)]
#[test]
fn telemetry_controller_preserves_changed_sources_and_cleans_failed_owned_checks() {
    let f = Fixture::new();
    telemetry_fixture(&f, "exit 7");
    let target = f.0.join("tests/rse_sampler_telemetry_allocations.rs");
    let output = telemetry_command(&f).output().unwrap();
    assert!(!output.status.success());
    assert!(!target.exists());
    telemetry_fixture(
        &f,
        "printf changed > tests/rse_sampler_telemetry_allocations.rs\nexit 7",
    );
    let output = telemetry_command(&f).output().unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&target).unwrap(), "changed");
    assert!(String::from_utf8_lossy(&output.stderr).contains("preserve"));
    fs::write(&target, "unrelated").unwrap();
    let output = telemetry_command(&f).output().unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(target).unwrap(), "unrelated");
}
#[cfg(unix)]
#[test]
fn telemetry_controller_cancellation_stops_its_worker_and_restores_owned_source() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    let f = Fixture::new();
    telemetry_fixture(&f, "printf ready > started\nsleep 30");
    let mut child = telemetry_command(&f)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !f.0.join("started").exists() && Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("telemetry controller exited early: {status}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = f.0.join("started").exists();
    // SAFETY: this PID belongs to our still-live, unreaped child, not a supplied
    // task ID. SIGTERM exercises the controller's own cancellation handler.
    let signalled = unsafe { libc::kill(child.id().try_into().unwrap(), libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut status = None;
    while Instant::now() < deadline {
        status = child.try_wait().unwrap();
        if status.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if status.is_none() {
        child.kill().unwrap();
    }
    let final_status = child.wait().unwrap();
    assert!(ready, "diagnostic worker did not start");
    assert_eq!(signalled, 0);
    assert!(status.is_some(), "owned diagnostic did not terminate");
    assert!(!final_status.success());
    assert!(
        !f.0.join("tests/rse_sampler_telemetry_allocations.rs")
            .exists()
    );
}
