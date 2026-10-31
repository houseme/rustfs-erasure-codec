use rustfs_erasure_codec::{CodecOptions, diagnostic_row, galois_8::ReedSolomon};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn measure(mut op: impl FnMut()) -> ([f64; 3], [u64; 3]) {
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_secs(1) {
        op();
    }
    let mut samples = [0.; 3];
    let mut counts = [0; 3];
    for (sample, count) in samples.iter_mut().zip(&mut counts) {
        let start = Instant::now();
        let mut iters = 0u64;
        loop {
            for _ in 0..4 {
                op();
            }
            iters += 4;
            if start.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / iters as f64;
        *count = iters;
    }
    (samples, counts)
}

fn main() {
    if let Ok(expected) = std::env::var("RSE_DIAG_EXPECT_BACKEND") {
        assert_eq!(
            rustfs_erasure_codec::galois_8::active_backend_name(),
            expected
        );
    }
    assert_eq!(rayon::current_num_threads(), 4);
    let available = std::thread::available_parallelism().unwrap().get();
    for (data_count, parity_count, len, missing) in [
        (10, 4, 1_048_576, 0),
        (10, 4, 1_048_577, 3),
        (4, 2, 1_048_576, 0),
    ] {
        let codec = ReedSolomon::with_options(
            data_count,
            parity_count,
            CodecOptions::builder().max_parallel_jobs(4).build(),
        )
        .unwrap();
        let mut original: Vec<Vec<u8>> = (0..data_count + parity_count)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut original).unwrap();
        let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
        shards[missing] = None;
        let (indices, row) = diagnostic_row::recovery_row(&codec, &mut shards).unwrap();
        let inputs: Vec<&[u8]> = indices.iter().map(|&i| original[i].as_slice()).collect();
        let decision = codec
            .reconstruct_stage_policies_for_bench(true)
            .0
            .decide(len, data_count, 1, available);
        assert!(decision.use_parallel && decision.jobs == 4);
        let mut output = vec![0xa5; len];
        let mut op = || {
            diagnostic_row::run_row(
                &codec,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        };
        let a1 = measure(&mut op);
        assert_eq!(output, original[missing]);
        let b1 = measure(|| {
            diagnostic_row::run_row(
                &codec,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        });
        assert_eq!(output, original[missing]);
        let b2 = measure(|| {
            diagnostic_row::run_row(
                &codec,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        });
        assert_eq!(output, original[missing]);
        let a2 = measure(|| {
            diagnostic_row::run_row(
                &codec,
                &row,
                &inputs,
                black_box(&mut output),
                decision.chunk_len,
                decision.jobs,
            )
        });
        assert_eq!(output, original[missing]);
        println!(
            "{{\"case_id\":\"retained_recovery_row/{data_count}x{parity_count}/{len}/missing{missing}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"chunk_len\":{},\"jobs\":{},\"available_parallelism\":{available}}}",
            a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1, decision.chunk_len, decision.jobs
        );
    }
}
