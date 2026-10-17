//! Functional compatibility with the published 9.0.0 crate; no timing measurement.

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
}
mod new {
    use codec_new as rustfs_erasure_codec;
    include!("probe.rs");
}

fn main() {
    let baseline = old::run();
    let candidate = new::run();
    assert_eq!(baseline.len(), 200);
    assert_eq!(candidate.len(), baseline.len());
    for (before, after) in baseline.iter().zip(&candidate) {
        assert_eq!(before, after, "parity compatibility: {}", before.id);
    }
    println!("200 published 9.0.0 -> candidate 9.1.0 non-fast parity/roundtrip cases passed");
}
