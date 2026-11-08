use std::{
    hint::black_box,
    time::{Duration, Instant},
};
fn number(name: &str) -> usize {
    std::env::var(name).unwrap().parse().unwrap()
}
fn thread_schedule() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/thread-self/schedstat").unwrap();
    let mut f = text.split_whitespace();
    (
        f.next().unwrap().parse().unwrap(),
        f.next().unwrap().parse().unwrap(),
    )
}
fn main() {
    let size = number("PROBE_SIZE");
    let missing = number("PROBE_MISSING");
    let mode = std::env::var("PROBE_MODE").unwrap();
    assert!(mode == "same" || mode == "candidate");
    let expected_backend = std::env::var("RSE_EXPECT_BACKEND").unwrap();
    assert_eq!(base::galois_8::active_backend_name(), expected_backend);
    assert_eq!(variant::galois_8::active_backend_name(), expected_backend);
    let old = base::galois_8::ReedSolomon::with_options(
        32,
        16,
        base::CodecOptions::builder().max_parallel_jobs(1).build(),
    )
    .unwrap();
    let new = variant::galois_8::ReedSolomon::with_options(
        32,
        16,
        variant::CodecOptions::builder()
            .max_parallel_jobs(1)
            .build(),
    )
    .unwrap();
    let mut original: Vec<Vec<u8>> = (0..48)
        .map(|i| {
            (0..size)
                .map(|j| (i * 31 + j * 17 + j / 251) as u8)
                .collect()
        })
        .collect();
    let mut candidate_encoded = original.clone();
    old.encode(&mut original).unwrap();
    new.encode(&mut candidate_encoded).unwrap();
    assert_eq!(candidate_encoded, original);
    drop(candidate_encoded);
    let targets: Vec<usize> = (0..missing).map(|i| i * 2).collect();
    assert!(targets.iter().all(|&i| i < 32));
    let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
    for &i in &targets {
        shards[i] = None;
    }
    let run = |shards: &mut Vec<Option<Vec<u8>>>, use_candidate: bool| {
        if use_candidate {
            new.reconstruct_data_opt(black_box(shards)).unwrap();
        } else {
            old.reconstruct_data_opt(black_box(shards)).unwrap();
        }
    };
    for stage in 0..4 {
        let use_candidate = mode == "candidate" && (stage == 1 || stage == 2);
        for _ in 0..8 {
            for &i in &targets {
                drop(shards[i].take());
            }
            run(&mut shards, use_candidate);
        }
        let mut samples = [0f64; 15];
        let mut iterations = [0u64; 15];
        let mut run_ns = [0u64; 15];
        let mut wait_ns = [0u64; 15];
        for index in 0..15 {
            let before = thread_schedule();
            let begin = Instant::now();
            let mut elapsed = Duration::ZERO;
            loop {
                for &i in &targets {
                    drop(shards[i].take());
                }
                let start = Instant::now();
                run(&mut shards, use_candidate);
                elapsed += start.elapsed();
                iterations[index] += 1;
                if begin.elapsed() >= Duration::from_millis(300) {
                    break;
                }
            }
            let after = thread_schedule();
            run_ns[index] = after.0.checked_sub(before.0).unwrap();
            wait_ns[index] = after.1.checked_sub(before.1).unwrap();
            samples[index] = elapsed.as_secs_f64() * 1e9 / iterations[index] as f64;
        }
        for &i in &targets {
            assert_eq!(shards[i].as_ref().unwrap(), &original[i]);
        }
        println!(
            "{{\"stage_no\":{stage},\"size\":{size},\"missing\":{missing},\"mode\":\"{mode}\",\"backend\":\"{expected_backend}\",\"candidate\":{use_candidate},\"samples_ns\":{samples:?},\"iterations\":{iterations:?},\"thread_run_ns\":{run_ns:?},\"thread_wait_ns\":{wait_ns:?}}}"
        );
    }
}
