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
fn reset(shards: &mut [Option<Vec<u8>>], targets: &[usize], held: &mut Vec<Vec<u8>>, retain: bool) {
    held.clear();
    for &i in targets {
        let prior = shards[i].take();
        if retain {
            if let Some(bytes) = prior {
                held.push(bytes);
            }
        }
    }
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
    let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
    let mut held: Vec<Vec<u8>> = Vec::with_capacity(missing);
    for stage in 0..4 {
        let retain = mode == "candidate" && (stage == 1 || stage == 2);
        held.clear();
        for &i in &targets {
            drop(shards[i].take());
        }
        for _ in 0..8 {
            reset(&mut shards, &targets, &mut held, retain);
            codec.reconstruct_data_opt(black_box(&mut shards)).unwrap();
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
                reset(&mut shards, &targets, &mut held, retain);
                assert_eq!(held.len(), if retain { missing } else { 0 });
                let t = Instant::now();
                codec.reconstruct_data_opt(black_box(&mut shards)).unwrap();
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
        held.clear();
        for &i in &targets {
            drop(shards[i].take());
        }
        println!(
            "{{\"stage_no\":{stage},\"size\":{size},\"missing\":{missing},\"mode\":\"{mode}\",\"backend\":\"{expected_backend}\",\"retain_previous\":{retain},\"samples_ns\":{samples:?},\"iterations\":{iterations:?},\"thread_run_ns\":{run_ns:?},\"thread_wait_ns\":{wait_ns:?},\"aperf_delta\":{aperf:?},\"mperf_delta\":{mperf:?},\"minor_faults\":{minor_faults:?},\"major_faults\":{major_faults:?}}}"
        );
    }
}
