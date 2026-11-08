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
    let text =
        std::fs::read_to_string("/proc/thread-self/schedstat").expect("thread scheduling counters");
    let mut fields = text.split_whitespace();
    (
        fields.next().unwrap().parse().unwrap(),
        fields.next().unwrap().parse().unwrap(),
    )
}
fn read_clock_counters(msr: &File) -> (u64, u64) {
    let mut a = [0u8; 8];
    let mut m = [0u8; 8];
    msr.read_exact_at(&mut a, 0xe8).expect("read APERF");
    msr.read_exact_at(&mut m, 0xe7).expect("read MPERF");
    (u64::from_le_bytes(a), u64::from_le_bytes(m))
}
fn main() {
    let msr_cpu = number("PROBE_MSR_CPU");
    let msr = File::open(format!("/dev/cpu/{msr_cpu}/msr")).expect("read-only MSR access");
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
    let stage_count: usize = std::env::var("PROBE_STAGES")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    assert!(stage_count == 1 || stage_count == 4);
    for stage_no in 0..stage_count {
        for _ in 0..8 {
            for &i in &targets {
                drop(shards[i].take());
            }
            run(&mut shards);
        }
        let mut samples = [0f64; 15];
        let mut iterations = [0u64; 15];
        let mut run_ns = [0u64; 15];
        let mut wait_ns = [0u64; 15];
        let mut aperf = [0u64; 15];
        let mut mperf = [0u64; 15];
        for index in 0..samples.len() {
            let before = thread_schedule();
            let clock_before = read_clock_counters(&msr);
            let begin = Instant::now();
            let mut elapsed = Duration::ZERO;
            loop {
                for &i in &targets {
                    drop(shards[i].take());
                }
                let t = Instant::now();
                run(&mut shards);
                elapsed += t.elapsed();
                iterations[index] += 1;
                if begin.elapsed() >= Duration::from_millis(300) {
                    break;
                }
            }
            let clock_after = read_clock_counters(&msr);
            let after = thread_schedule();
            aperf[index] = clock_after
                .0
                .checked_sub(clock_before.0)
                .expect("monotonic APERF");
            mperf[index] = clock_after
                .1
                .checked_sub(clock_before.1)
                .expect("monotonic MPERF");
            assert!(aperf[index] > 0 && mperf[index] > 0);
            run_ns[index] = after
                .0
                .checked_sub(before.0)
                .expect("monotonic run counter");
            wait_ns[index] = after
                .1
                .checked_sub(before.1)
                .expect("monotonic wait counter");
            samples[index] = elapsed.as_secs_f64() * 1e9 / iterations[index] as f64;
        }
        for &i in &targets {
            assert_eq!(shards[i].as_ref().unwrap(), &original[i]);
        }
        println!(
            "{{\"stage_no\":{stage_no},\"data\":{data},\"parity\":{parity},\"size\":{size},\"missing\":{missing},\"mode\":\"{mode}\",\"backend\":\"{backend}\",\"samples_ns\":{samples:?},\"iterations\":{iterations:?},\"thread_run_ns\":{run_ns:?},\"thread_wait_ns\":{wait_ns:?},\"aperf_delta\":{aperf:?},\"mperf_delta\":{mperf:?}}}"
        );
    }
}
