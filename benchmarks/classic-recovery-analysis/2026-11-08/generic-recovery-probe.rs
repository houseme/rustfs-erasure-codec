use rustfs_erasure_codec::{
    CodecOptions,
    galois_8::{ReedSolomon, active_backend_name},
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
fn number(name: &str) -> usize {
    std::env::var(name).unwrap().parse().unwrap()
}
fn main() {
    let data = number("PROBE_DATA");
    let parity = number("PROBE_PARITY");
    let size = number("PROBE_SIZE");
    let missing = number("PROBE_MISSING");
    let mode = std::env::var("PROBE_MODE").unwrap();
    let backend = active_backend_name();
    assert_eq!(backend, std::env::var("RSE_EXPECT_BACKEND").unwrap());
    let codec = ReedSolomon::with_options(
        data,
        parity,
        CodecOptions::builder().max_parallel_jobs(1).build(),
    )
    .unwrap();
    let mut original: Vec<Vec<u8>> = (0..data + parity)
        .map(|i| {
            (0..size)
                .map(|j| (i * 31 + j * 17 + j / 251) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut original).unwrap();
    let missing_indices: Vec<usize> = (0..missing).map(|i| i * 2).collect();
    assert!(missing_indices.iter().all(|&i| i < data));
    let targets: Vec<usize> = if mode == "generic_some" {
        assert_eq!(missing, 4);
        vec![2, 6]
    } else {
        missing_indices.clone()
    };
    let mut required = vec![false; data + parity];
    for &i in &targets {
        required[i] = true;
    }
    let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
    for &i in &missing_indices {
        shards[i] = None;
    }
    let run = |shards: &mut Vec<Option<Vec<u8>>>| match mode.as_str() {
        "generic_data" => codec.reconstruct_data(black_box(shards)).unwrap(),
        "generic_some" => codec
            .reconstruct_some(black_box(shards), black_box(&required))
            .unwrap(),
        "opt_data" => codec.reconstruct_data_opt(black_box(shards)).unwrap(),
        _ => panic!("invalid mode"),
    };
    run(&mut shards);
    for &i in &targets {
        assert_eq!(shards[i].as_ref().unwrap(), &original[i]);
    }
    if mode == "generic_some" {
        for &i in &missing_indices {
            if !required[i] {
                assert!(shards[i].is_none());
            }
        }
    }
    for _ in 0..8 {
        for &i in &targets {
            drop(shards[i].take());
        }
        run(&mut shards);
    }
    let mut samples = [0f64; 15];
    let mut iterations = [0u64; 15];
    for (sample, count) in samples.iter_mut().zip(iterations.iter_mut()) {
        let begin = Instant::now();
        let mut elapsed = Duration::ZERO;
        loop {
            for &i in &targets {
                drop(shards[i].take());
            }
            let t = Instant::now();
            run(&mut shards);
            elapsed += t.elapsed();
            *count += 1;
            if begin.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = elapsed.as_secs_f64() * 1e9 / *count as f64;
    }
    for &i in &targets {
        assert_eq!(shards[i].as_ref().unwrap(), &original[i]);
    }
    println!(
        "{{\"data\":{data},\"parity\":{parity},\"size\":{size},\"missing\":{missing},\"mode\":\"{mode}\",\"backend\":\"{backend}\",\"samples_ns\":{samples:?},\"iterations\":{iterations:?}}}"
    );
}
