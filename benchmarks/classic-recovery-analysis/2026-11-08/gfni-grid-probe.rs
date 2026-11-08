use rustfs_erasure_codec::galois_8::{ReedSolomon, active_backend_name};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
fn value(key: &str) -> usize {
    std::env::var(key).unwrap().parse().unwrap()
}
fn main() {
    let data = value("GRID_DATA");
    let parity = value("GRID_PARITY");
    let size = value("GRID_SIZE");
    let sample_ms: u64 = std::env::var("GRID_MS")
        .unwrap_or_else(|_| "500".into())
        .parse()
        .unwrap();
    assert!(sample_ms > 0);
    let backend = active_backend_name();
    assert_eq!(backend, std::env::var("RSE_EXPECT_BACKEND").unwrap());
    let codec = ReedSolomon::new(data, parity).unwrap();
    let inputs: Vec<Vec<u8>> = (0..data)
        .map(|i| {
            (0..size)
                .map(|j| (i * 31 + j * 17 + j / 251) as u8)
                .collect()
        })
        .collect();
    let mut expected = vec![vec![0u8; size]; parity];
    for (i, input) in inputs.iter().enumerate() {
        codec.encode_single_sep(i, input, &mut expected).unwrap();
    }
    let mut out = vec![vec![0u8; size]; parity];
    codec.encode_sep(&inputs, &mut out).unwrap();
    assert_eq!(out, expected);
    for _ in 0..8 {
        codec
            .encode_sep(black_box(&inputs), black_box(&mut out))
            .unwrap();
    }
    let mut samples = [0.0; 3];
    for sample in &mut samples {
        let start = Instant::now();
        let mut n = 0u64;
        loop {
            codec
                .encode_sep(black_box(&inputs), black_box(&mut out))
                .unwrap();
            n += 1;
            if start.elapsed() >= Duration::from_millis(sample_ms) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / n as f64;
    }
    assert_eq!(out, expected);
    println!(
        "{{\"data\":{data},\"parity\":{parity},\"size\":{size},\"backend\":\"{backend}\",\"samples_ns\":{samples:?}}}"
    );
}
