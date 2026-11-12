use std::time::{Duration, Instant};

fn prepare(shards: &mut [(Vec<u8>, bool)], data: usize, cold: bool, iteration: usize) {
    let pattern = if cold { iteration % 20 } else { 0 };
    let missing = pattern % data;
    shards[missing].0.fill(0);
    shards[missing].1 = false;
    if cold {
        let parity = data + (pattern / data) % 4;
        shards[parity].0.fill(0);
        shards[parity].1 = false;
    }
}

fn phase(
    shards: &mut [(Vec<u8>, bool)],
    expected: &[Vec<u8>],
    data: usize,
    cold: bool,
    mut reconstruct: impl FnMut(&mut [(Vec<u8>, bool)]),
) -> f64 {
    for iteration in 0..128 {
        prepare(shards, data, cold, iteration);
        reconstruct(shards);
    }
    let start = Instant::now();
    let mut calls = 0usize;
    while start.elapsed() < Duration::from_millis(400) {
        for _ in 0..32 {
            prepare(shards, data, cold, calls + 128);
            reconstruct(shards);
            calls += 1;
        }
    }
    let elapsed = start.elapsed().as_nanos() as f64 / calls as f64;
    for (shard, expected) in shards.iter().zip(expected) {
        assert!(shard.1);
        assert_eq!(&shard.0, expected);
    }
    elapsed
}

fn main() {
    for data in [10usize, 300] {
        for bytes in [64usize, 4096] {
            for cold in [false, true] {
                let base = base_codec::galois_8::ReedSolomon::with_options(
                    data, 4,
                    base_codec::CodecOptions::builder()
                        .codec_family(base_codec::CodecFamily::LeopardGF16)
                        .max_parallel_jobs(1).build(),
                ).unwrap();
                let next = next_codec::galois_8::ReedSolomon::with_options(
                    data, 4,
                    next_codec::CodecOptions::builder()
                        .codec_family(next_codec::CodecFamily::LeopardGF16)
                        .max_parallel_jobs(1).build(),
                ).unwrap();
                let mut expected: Vec<Vec<u8>> = (0..data + 4).map(|shard| {
                    (0..bytes).map(|byte| (shard * 31 + byte * 17 + byte / 11) as u8).collect()
                }).collect();
                base.encode(&mut expected).unwrap();
                assert!(next.verify(&expected).unwrap());
                let mut shards: Vec<_> = expected.iter().cloned().map(|bytes| (bytes, true)).collect();
                let mut samples = Vec::new();
                for index in 0..4 {
                    let sample = if index == 0 || index == 3 {
                        phase(&mut shards, &expected, data, cold, |shards| base.reconstruct(shards).unwrap())
                    } else {
                        phase(&mut shards, &expected, data, cold, |shards| next.reconstruct(shards).unwrap())
                    };
                    samples.push(sample);
                }
                let base_ns = (samples[0] + samples[3]) / 2.0;
                let next_ns = (samples[1] + samples[2]) / 2.0;
                let base_drift = (samples[0] - samples[3]).abs() / base_ns;
                let next_drift = (samples[1] - samples[2]).abs() / next_ns;
                let gate = base_drift <= 0.05 && next_drift <= 0.05;
                println!("{{\"data\":{data},\"bytes\":{bytes},\"cold\":{cold},\"phase_ns\":{samples:?},\"base_drift\":{base_drift},\"next_drift\":{next_drift},\"candidate_delta_pct\":{},\"gate\":{gate}}}", (next_ns / base_ns - 1.0) * 100.0);
                if !gate {
                    std::process::exit(2);
                }
            }
        }
    }
}
