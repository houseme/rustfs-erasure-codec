use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Usage {
    cpu_ns: u64,
    switches: i64,
    minor_faults: i64,
}

fn usage() -> Usage {
    let mut clock = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: clock points to valid output storage and the process clock is supported on Unix.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, clock.as_mut_ptr()) },
        0
    );
    // SAFETY: the successful clock_gettime call initialized the output.
    let clock = unsafe { clock.assume_init() };
    let mut resources = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: resources points to valid output storage for this process's usage.
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, resources.as_mut_ptr()) },
        0
    );
    // SAFETY: the successful getrusage call initialized the output.
    let resources = unsafe { resources.assume_init() };
    Usage {
        cpu_ns: clock.tv_sec as u64 * 1_000_000_000 + clock.tv_nsec as u64,
        switches: resources.ru_nvcsw + resources.ru_nivcsw,
        minor_faults: resources.ru_minflt,
    }
}

#[derive(Clone, Copy)]
struct Sample {
    wall_ns: f64,
    cpu_ns: f64,
}

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
    sample_ms: u64,
    mut reconstruct: impl FnMut(&mut [(Vec<u8>, bool)]),
) -> (Sample, usize, i64, i64) {
    for iteration in 0..128 {
        prepare(shards, data, cold, iteration);
        reconstruct(shards);
    }
    let before = usage();
    let start = Instant::now();
    let mut calls = 0usize;
    while start.elapsed() < Duration::from_millis(sample_ms) {
        for _ in 0..32 {
            prepare(shards, data, cold, calls + 128);
            reconstruct(shards);
            calls += 1;
        }
    }
    let wall_ns = start.elapsed().as_nanos() as f64 / calls as f64;
    let after = usage();
    for (shard, expected) in shards.iter().zip(expected) {
        assert!(shard.1);
        assert_eq!(&shard.0, expected);
    }
    (
        Sample {
            wall_ns,
            cpu_ns: (after.cpu_ns - before.cpu_ns) as f64 / calls as f64,
        },
        calls,
        after.switches - before.switches,
        after.minor_faults - before.minor_faults,
    )
}

fn setting(name: &str, default: u64) -> u64 {
    let value = std::env::var(name)
        .map(|value| value.parse().unwrap())
        .unwrap_or(default);
    assert!(value > 0);
    value
}

fn main() {
    let rounds = setting("RSE_CACHE_ROUNDS", 3);
    let warm_ms = setting("RSE_CACHE_WARM_MS", 1000);
    let sample_ms = setting("RSE_CACHE_SAMPLE_MS", 250);
    let filter = std::env::var("RSE_CACHE_FILTER").unwrap_or_default();
    println!(
        "{{\"kind\":\"settings\",\"rounds\":{rounds},\"warm_ms\":{warm_ms},\"sample_ms\":{sample_ms},\"drift_limit\":0.05}}"
    );
    for data in [10usize, 252, 253, 300] {
        for bytes in [64usize, 4096] {
            for cold in [false, true] {
                let mode = if cold { "cold" } else { "warm" };
                let case = format!("serial/{data}x4/{bytes}/{mode}");
                if !case.contains(&filter) {
                    continue;
                }
                let base = base_codec::galois_8::ReedSolomon::with_options(
                    data,
                    4,
                    base_codec::CodecOptions::builder()
                        .codec_family(base_codec::CodecFamily::LeopardGF16)
                        .max_parallel_jobs(1)
                        .build(),
                )
                .unwrap();
                let next = next_codec::galois_8::ReedSolomon::with_options(
                    data,
                    4,
                    next_codec::CodecOptions::builder()
                        .codec_family(next_codec::CodecFamily::LeopardGF16)
                        .max_parallel_jobs(1)
                        .build(),
                )
                .unwrap();
                let mut expected: Vec<Vec<u8>> = (0..data + 4)
                    .map(|shard| {
                        (0..bytes)
                            .map(|byte| (shard * 31 + byte * 17 + byte / 11) as u8)
                            .collect()
                    })
                    .collect();
                base.encode(&mut expected).unwrap();
                assert!(next.verify(&expected).unwrap());
                let mut shards: Vec<_> = expected
                    .iter()
                    .cloned()
                    .map(|bytes| (bytes, true))
                    .collect();
                let warm = Instant::now();
                let mut iteration = 0usize;
                while warm.elapsed() < Duration::from_millis(warm_ms) {
                    prepare(&mut shards, data, cold, iteration);
                    base.reconstruct(&mut shards).unwrap();
                    prepare(&mut shards, data, cold, iteration);
                    next.reconstruct(&mut shards).unwrap();
                    iteration += 1;
                }
                for round in 0..rounds {
                    let order = if round % 2 == 0 {
                        [true, false, false, true]
                    } else {
                        [false, true, true, false]
                    };
                    let mut before = Vec::new();
                    let mut after = Vec::new();
                    for (index, baseline) in order.into_iter().enumerate() {
                        let (sample, calls, switches, minor_faults) = if baseline {
                            phase(&mut shards, &expected, data, cold, sample_ms, |shards| {
                                base.reconstruct(shards).unwrap()
                            })
                        } else {
                            phase(&mut shards, &expected, data, cold, sample_ms, |shards| {
                                next.reconstruct(shards).unwrap()
                            })
                        };
                        let role = if baseline { "baseline" } else { "candidate" };
                        println!(
                            "{{\"kind\":\"phase\",\"case\":\"{case}\",\"round\":{round},\"index\":{index},\"role\":\"{role}\",\"calls\":{calls},\"wall_ns\":{},\"cpu_ns\":{},\"switches\":{switches},\"minor_faults\":{minor_faults}}}",
                            sample.wall_ns, sample.cpu_ns
                        );
                        if baseline {
                            before.push(sample);
                        } else {
                            after.push(sample);
                        }
                    }
                    let base_wall = (before[0].wall_ns + before[1].wall_ns) / 2.0;
                    let next_wall = (after[0].wall_ns + after[1].wall_ns) / 2.0;
                    let base_cpu = (before[0].cpu_ns + before[1].cpu_ns) / 2.0;
                    let next_cpu = (after[0].cpu_ns + after[1].cpu_ns) / 2.0;
                    let base_drift = (before[0].wall_ns - before[1].wall_ns).abs() / base_wall;
                    let next_drift = (after[0].wall_ns - after[1].wall_ns).abs() / next_wall;
                    let base_cpu_drift = (before[0].cpu_ns - before[1].cpu_ns).abs() / base_cpu;
                    let next_cpu_drift = (after[0].cpu_ns - after[1].cpu_ns).abs() / next_cpu;
                    let over_limit = base_drift > 0.05 || next_drift > 0.05;
                    println!(
                        "{{\"kind\":\"comparison\",\"case\":\"{case}\",\"round\":{round},\"wall_delta_pct\":{},\"cpu_delta_pct\":{},\"base_drift\":{base_drift},\"next_drift\":{next_drift},\"base_cpu_drift\":{base_cpu_drift},\"next_cpu_drift\":{next_cpu_drift},\"over_limit\":{over_limit}}}",
                        (next_wall / base_wall - 1.0) * 100.0,
                        (next_cpu / base_cpu - 1.0) * 100.0
                    );
                }
            }
        }
    }
}
