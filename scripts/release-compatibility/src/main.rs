//! Functional compatibility and optional resource counts; never latency measurement.
mod allocation;

const RESOURCE_CASE_COUNT: usize = 45;

struct ResourceCase {
    id: String,
    counts: Vec<allocation::Counts>,
    parity: Vec<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    id: String,
    parity: Vec<Vec<u8>>,
}

trait ElementBytes: Copy {
    fn append_bytes(self, output: &mut Vec<u8>);
}

impl ElementBytes for u8 {
    fn append_bytes(self, output: &mut Vec<u8>) {
        output.push(self);
    }
}

impl ElementBytes for [u8; 2] {
    fn append_bytes(self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self);
    }
}

mod old {
    use codec_old as rustfs_erasure_codec;
    include!("probe.rs");
    mod resources {
        use super::rustfs_erasure_codec;
        include!("resource_probe.rs");
    }
    pub(super) fn resource_counts() -> Vec<super::ResourceCase> {
        resources::resources()
    }
}
mod new {
    use codec_new as rustfs_erasure_codec;
    include!("probe.rs");
    mod resources {
        use super::rustfs_erasure_codec;
        include!("resource_probe.rs");
    }
    pub(super) fn resource_counts() -> Vec<super::ResourceCase> {
        resources::resources()
    }
}

fn main() {
    let baseline = old::run();
    let candidate = new::run();
    assert_eq!(baseline.len(), 200);
    assert_eq!(candidate.len(), baseline.len());
    for (before, after) in baseline.iter().zip(&candidate) {
        assert_eq!(before, after, "parity compatibility: {}", before.id);
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.is_empty() {
        assert!(
            args.len() == 2 && args[0] == "--allocation-report",
            "usage: --allocation-report FILE"
        );
        resource_report(std::path::Path::new(&args[1]));
    }
    println!("200 published 9.0.0 -> candidate 9.1.0 non-fast parity/roundtrip cases passed");
}

fn resource_report(path: &std::path::Path) {
    let old = old::resource_counts();
    let new = new::resource_counts();
    assert_eq!(old.len(), RESOURCE_CASE_COUNT);
    assert_eq!(new.len(), old.len());
    let mut rows = Vec::with_capacity(old.len());
    for (before, after) in old.iter().zip(&new) {
        assert_eq!(before.id, after.id);
        assert!(
            before.parity == after.parity,
            "resource case parity: {}",
            before.id
        );
        assert_eq!(before.counts.len(), 3);
        assert_eq!(after.counts.len(), 3);
        let values = |counts: &[allocation::Counts]| {
            counts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        rows.push(format!(
            "{{\"case\":\"{}\",\"baseline\":[{}],\"candidate\":[{}]}}",
            before.id,
            values(&before.counts),
            values(&after.counts)
        ));
    }
    let report = format!(
        "{{\"schema_version\":1,\"baseline_version\":\"9.0.0\",\"candidate_version\":\"9.1.0\",\"architecture\":\"{}\",\"build_debug_assertions\":{},\"baseline_backend\":\"{}\",\"candidate_backend\":\"{}\",\"resource_parity_cases\":{},\"scope\":\"two warmups, all three repetitions; caller setup excluded; synchronous API requests across all threads\",\"bytes_policy\":\"sum of allocation sizes and full realloc target sizes; not RSS or peak live bytes\",\"latency_claim\":null,\"release_approval\":false,\"cases\":[{}]}}\n",
        std::env::consts::ARCH,
        cfg!(debug_assertions),
        codec_old::galois_8::active_backend_name(),
        codec_new::galois_8::active_backend_name(),
        RESOURCE_CASE_COUNT,
        rows.join(",")
    );
    use std::io::Write;
    let mut output = std::fs::File::create_new(path).expect("preserve an existing resource report");
    output.write_all(report.as_bytes()).unwrap();
}
