use rustfs_erasure_codec::{
    CodecOptions,
    galois_8::{ReedSolomon, active_backend_name},
};
use std::{
    fs::File,
    hint::black_box,
    os::unix::fs::FileExt,
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
fn clock_counters(msr: &File) -> (u64, u64) {
    let mut a = [0; 8];
    let mut m = [0; 8];
    msr.read_exact_at(&mut a, 0xe8).unwrap();
    msr.read_exact_at(&mut m, 0xe7).unwrap();
    (u64::from_le_bytes(a), u64::from_le_bytes(m))
}
fn thread_faults() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
    let mut f = text.rsplit_once(") ").unwrap().1.split_whitespace();
    (
        f.nth(7).unwrap().parse().unwrap(),
        f.nth(1).unwrap().parse().unwrap(),
    )
}
fn fixture(
    original: &[Vec<u8>],
    targets: &[usize],
    reverse: bool,
) -> (Vec<Option<Vec<u8>>>, usize) {
    let count = original.len() - targets.len();
    let mut rank = 0;
    let mut capacity = 0;
    let shards = original
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            if targets.contains(&index) {
                return None;
            }
            let order = if reverse { count - 1 - rank } else { rank };
            rank += 1;
            let mut v = Vec::with_capacity(bytes.len() + (order % 16) * 64);
            v.extend_from_slice(bytes);
            assert_eq!(&v, bytes);
            capacity += v.capacity();
            Some(v)
        })
        .collect();
    assert_eq!(rank, count);
    (shards, capacity)
}
fn bucket_summary(shards: &[Option<Vec<u8>>], targets: &[usize]) -> (usize, usize, usize, usize) {
    let mut b4 = [0u8; 64];
    let mut b64 = [0u8; 1024];
    let mut count = 0;
    for (i, v) in shards.iter().enumerate() {
        if targets.contains(&i) {
            continue;
        }
        if let Some(bytes) = v {
            let n = (bytes.as_ptr() as usize) / 64;
            b4[n % 64] += 1;
            b64[n % 1024] += 1;
            count += 1;
            if count == 32 {
                break;
            }
        }
    }
    assert_eq!(count, 32);
    (
        b4.iter().filter(|&&x| x != 0).count(),
        *b4.iter().max().unwrap() as usize,
        b64.iter().filter(|&&x| x != 0).count(),
        *b64.iter().max().unwrap() as usize,
    )
}
fn main() {
    let size = number("PROBE_SIZE");
    let missing = number("PROBE_MISSING");
    let cpu = number("PROBE_MSR_CPU");
    let mode = std::env::var("PROBE_MODE").unwrap();
    assert!(mode == "same" || mode == "candidate");
    let expected_backend = std::env::var("RSE_EXPECT_BACKEND").unwrap();
    assert_eq!(active_backend_name(), expected_backend);
    let msr = File::open(format!("/dev/cpu/{cpu}/msr")).unwrap();
    let codec =
        ReedSolomon::with_options(32, 16, CodecOptions::builder().max_parallel_jobs(1).build())
            .unwrap();
    let mut original: Vec<Vec<u8>> = (0..48)
        .map(|i| {
            (0..size)
                .map(|j| (i * 31 + j * 17 + j / 251) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut original).unwrap();
    let targets: Vec<usize> = (0..missing).map(|i| i * 2).collect();
    assert!(targets.iter().all(|&i| i < 32));
    let (mut left, left_capacity) = fixture(&original, &targets, false);
    let (mut right, right_capacity) = fixture(&original, &targets, true);
    assert_eq!(left_capacity, right_capacity);
    let left_buckets = bucket_summary(&left, &targets);
    let right_buckets = bucket_summary(&right, &targets);
    for stage in 0..4 {
        let use_right = mode == "candidate" && (stage == 1 || stage == 2);
        let shards = if use_right { &mut right } else { &mut left };
        let expected_buckets = if use_right {
            right_buckets
        } else {
            left_buckets
        };
        for _ in 0..8 {
            for &i in &targets {
                drop(shards[i].take());
            }
            codec.reconstruct_data_opt(black_box(&mut *shards)).unwrap();
        }
        let mut samples = [0f64; 15];
        let mut iterations = [0u64; 15];
        let mut run_ns = [0u64; 15];
        let mut wait_ns = [0u64; 15];
        let mut aperf = [0u64; 15];
        let mut mperf = [0u64; 15];
        let mut minor_faults = [0u64; 15];
        let mut major_faults = [0u64; 15];
        for index in 0..15 {
            let faults_before = thread_faults();
            let before = thread_schedule();
            let clocks_before = clock_counters(&msr);
            let begin = Instant::now();
            let mut elapsed = Duration::ZERO;
            loop {
                for &i in &targets {
                    drop(shards[i].take());
                }
                let t = Instant::now();
                codec.reconstruct_data_opt(black_box(&mut *shards)).unwrap();
                elapsed += t.elapsed();
                iterations[index] += 1;
                if begin.elapsed() >= Duration::from_millis(300) {
                    break;
                }
            }
            let clocks_after = clock_counters(&msr);
            let after = thread_schedule();
            let faults_after = thread_faults();
            run_ns[index] = after.0.checked_sub(before.0).unwrap();
            wait_ns[index] = after.1.checked_sub(before.1).unwrap();
            aperf[index] = clocks_after.0.checked_sub(clocks_before.0).unwrap();
            mperf[index] = clocks_after.1.checked_sub(clocks_before.1).unwrap();
            assert!(aperf[index] > 0 && mperf[index] > 0);
            minor_faults[index] = faults_after.0.checked_sub(faults_before.0).unwrap();
            major_faults[index] = faults_after.1.checked_sub(faults_before.1).unwrap();
            samples[index] = elapsed.as_secs_f64() * 1e9 / iterations[index] as f64;
        }
        for &i in &targets {
            assert_eq!(shards[i].as_ref().unwrap(), &original[i]);
        }
        assert_eq!(bucket_summary(shards, &targets), expected_buckets);
        println!(
            "{{\"stage_no\":{stage},\"size\":{size},\"missing\":{missing},\"mode\":\"{mode}\",\"backend\":\"{expected_backend}\",\"layout_right\":{use_right},\"left_capacity\":{left_capacity},\"right_capacity\":{right_capacity},\"left_bucket4_unique\":{},\"left_bucket4_max\":{},\"left_bucket64_unique\":{},\"left_bucket64_max\":{},\"right_bucket4_unique\":{},\"right_bucket4_max\":{},\"right_bucket64_unique\":{},\"right_bucket64_max\":{},\"samples_ns\":{samples:?},\"iterations\":{iterations:?},\"thread_run_ns\":{run_ns:?},\"thread_wait_ns\":{wait_ns:?},\"aperf_delta\":{aperf:?},\"mperf_delta\":{mperf:?},\"minor_faults\":{minor_faults:?},\"major_faults\":{major_faults:?}}}",
            left_buckets.0,
            left_buckets.1,
            left_buckets.2,
            left_buckets.3,
            right_buckets.0,
            right_buckets.1,
            right_buckets.2,
            right_buckets.3
        );
    }
}
