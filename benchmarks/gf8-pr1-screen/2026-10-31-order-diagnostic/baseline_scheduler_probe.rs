use std::hint::black_box;
use std::time::{Duration, Instant};

// Linux per-thread counters, read only outside the timed iterations.
fn scheduler_snapshot() -> [u64; 3] {
    let mut totals = [0u64; 3];
    for entry in std::fs::read_dir("/proc/self/task").unwrap() {
        let entry = entry.unwrap();
        let text = std::fs::read_to_string(entry.path().join("schedstat")).unwrap();
        let values: Vec<u64> = text
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 3);
        for (total, value) in totals.iter_mut().zip(values) {
            *total += value;
        }
    }
    totals
}

fn measure(mut op: impl FnMut()) -> ([f64; 3], [u64; 3], [u64; 3], u128) {
    let before = scheduler_snapshot();
    let stage_started = Instant::now();
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_secs(1) {
        op();
    }
    let mut samples = [0.; 3];
    let mut counts = [0; 3];
    for (sample, count) in samples.iter_mut().zip(&mut counts) {
        let started = Instant::now();
        let mut iters = 0u64;
        loop {
            for _ in 0..4 {
                op();
            }
            iters += 4;
            if started.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = started.elapsed().as_secs_f64() * 1e9 / iters as f64;
        *count = iters;
    }
    let wall_ns = stage_started.elapsed().as_nanos();
    let after = scheduler_snapshot();
    let delta = std::array::from_fn(|i| after[i].checked_sub(before[i]).unwrap());
    (samples, counts, delta, wall_ns)
}

fn main() {
    assert_eq!(base_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(next_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(rayon::current_num_threads(), 4);
    let available = std::thread::available_parallelism().unwrap().get();
    assert_eq!(available, 5);
    for (label, data, parity, len, missing) in [
        ("first", 4, 2, 1_048_576, 1),
        ("middle-one", 10, 4, 1_048_576, 1),
        ("middle-two", 10, 4, 1_048_576, 2),
        ("last", 4, 2, 1_048_576, 1),
    ] {
        let base = base_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            base_codec::CodecOptions::builder()
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        let next = next_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            next_codec::CodecOptions::builder()
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        let mut original: Vec<Vec<u8>> = (0..data + parity)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        base.encode(&mut original).unwrap();
        let mut encoded = original.clone();
        next.encode(&mut encoded).unwrap();
        assert_eq!(encoded, original);
        drop(encoded);
        let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
        let a1 = measure(|| {
            for shard in &mut shards[..missing] {
                drop(shard.take());
            }
            base.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        eprintln!("SCHED {} A1 {:?} {}", label, a1.2, a1.3);
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(a, b)| a.as_ref() == Some(b))
        );
        let b1 = measure(|| {
            for shard in &mut shards[..missing] {
                drop(shard.take());
            }
            next.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        eprintln!("SCHED {} B1 {:?} {}", label, b1.2, b1.3);
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(a, b)| a.as_ref() == Some(b))
        );
        let b2 = measure(|| {
            for shard in &mut shards[..missing] {
                drop(shard.take());
            }
            next.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        eprintln!("SCHED {} B2 {:?} {}", label, b2.2, b2.3);
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(a, b)| a.as_ref() == Some(b))
        );
        let a2 = measure(|| {
            for shard in &mut shards[..missing] {
                drop(shard.take());
            }
            base.reconstruct_data_opt(black_box(&mut shards)).unwrap();
        });
        eprintln!("SCHED {} A2 {:?} {}", label, a2.2, a2.3);
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(a, b)| a.as_ref() == Some(b))
        );
        println!(
            "{{\"case_id\":\"reconstruct_data_opt/{data}x{parity}/{len}/missing{missing}/{label}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"available_parallelism\":{available},\"rayon_threads\":4}}",
            a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1
        );
    }
}
