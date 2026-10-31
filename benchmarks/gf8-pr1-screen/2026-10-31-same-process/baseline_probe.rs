use rustfs_erasure_codec::{CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn measure(
    codec: &ReedSolomon,
    shards: &mut [Option<Vec<u8>>],
    missing: usize,
) -> ([f64; 3], [u64; 3]) {
    let operation = |shards: &mut [Option<Vec<u8>>]| {
        for shard in &mut shards[..missing] {
            drop(shard.take());
        }
        codec.reconstruct_data_opt(black_box(shards)).unwrap();
    };
    for _ in 0..20 {
        operation(shards);
    }
    let mut samples = [0.; 3];
    let mut counts = [0; 3];
    for (sample, count) in samples.iter_mut().zip(&mut counts) {
        let started = Instant::now();
        let mut iters = 0u64;
        loop {
            for _ in 0..4 {
                operation(shards);
            }
            iters += 4;
            if started.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = started.elapsed().as_secs_f64() * 1e9 / iters as f64;
        *count = iters;
    }
    (samples, counts)
}

fn main() {
    assert_eq!(
        rustfs_erasure_codec::galois_8::active_backend_name(),
        "rust-avx2"
    );
    assert_eq!(rayon::current_num_threads(), 4);
    let available = std::thread::available_parallelism().unwrap().get();
    assert_eq!(available, 5);
    for (data, parity, len, missing) in [
        (10, 4, 1_048_576, 1),
        (10, 4, 1_048_576, 2),
        (4, 2, 1_048_576, 1),
    ] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder().max_parallel_jobs(4).build(),
        )
        .unwrap();
        let mut original: Vec<Vec<u8>> = (0..data + parity)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut original).unwrap();
        let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
        let a1 = measure(&codec, &mut shards, missing);
        let b1 = measure(&codec, &mut shards, missing);
        let b2 = measure(&codec, &mut shards, missing);
        let a2 = measure(&codec, &mut shards, missing);
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(a, b)| a.as_ref() == Some(b))
        );
        println!(
            "{{\"case_id\":\"reconstruct_data_opt/{data}x{parity}/{len}/missing{missing}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"available_parallelism\":{available},\"rayon_threads\":4}}",
            a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1
        );
    }
}
