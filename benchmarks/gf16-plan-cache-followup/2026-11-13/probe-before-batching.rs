//! Standalone Unix diagnostic linked against base_codec, next_codec and libc.
//! Keeps every comparison, including samples that exceed the drift limit.
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Barrier,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

type Shards = [(Vec<u8>, bool)];

#[derive(Clone, Copy)]
struct Usage {
    cpu_ns: u64,
    switches: i64,
    minor_faults: i64,
}

fn usage() -> Usage {
    let mut clock = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: clock provides valid output storage for the process CPU clock.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, clock.as_mut_ptr()) },
        0
    );
    // SAFETY: the successful clock_gettime call initialized the output.
    let clock = unsafe { clock.assume_init() };
    let mut resources = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: resources provides valid output storage for this process's usage.
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, resources.as_mut_ptr()) },
        0
    );
    // SAFETY: the successful getrusage call initialized the output.
    let resources = unsafe { resources.assume_init() };
    Usage {
        cpu_ns: clock.tv_sec as u64 * 1_000_000_000 + clock.tv_nsec as u64,
        switches: resources.ru_nvcsw as i64 + resources.ru_nivcsw as i64,
        minor_faults: resources.ru_minflt as i64,
    }
}

#[derive(Clone, Copy)]
struct Sample {
    wall_ns: f64,
    cpu_ns: f64,
}

#[derive(Clone, Copy)]
struct Settings {
    rounds: u64,
    warm_ms: u64,
    phase_warm_ms: u64,
    sample_ms: u64,
}

#[derive(Clone, Copy)]
struct Case {
    data: usize,
    bytes: usize,
    cold: bool,
    callers: usize,
}

impl Case {
    fn id(self) -> String {
        let execution = if self.callers == 1 {
            "serial"
        } else {
            "concurrent"
        };
        let mode = if self.cold { "cold" } else { "warm" };
        format!(
            "{execution}/{}/{}x4/{}/{mode}",
            self.callers, self.data, self.bytes
        )
    }

    fn prepare(self, shards: &mut Shards, iteration: usize) {
        let pattern = if self.cold { iteration % 20 } else { 0 };
        let missing = pattern % self.data;
        shards[missing].0.fill(0);
        shards[missing].1 = false;
        if self.cold {
            let parity = self.data + (pattern / self.data) % 4;
            shards[parity].0.fill(0);
            shards[parity].1 = false;
        }
    }
}

fn correct(shards: &Shards, expected: &[Vec<u8>]) -> bool {
    shards
        .iter()
        .zip(expected)
        .all(|(shard, expected)| shard.1 && &shard.0 == expected)
}

fn compare(case: Case, settings: Settings, mut step: impl FnMut(bool, usize, bool)) {
    let case_id = case.id();
    let warm = Instant::now();
    let mut iteration = 0usize;
    while warm.elapsed() < Duration::from_millis(settings.warm_ms) {
        step(true, iteration, false);
        step(false, iteration, false);
        iteration += 1;
    }
    for round in 0..settings.rounds {
        let order = if round % 2 == 0 {
            [true, false, false, true]
        } else {
            [false, true, true, false]
        };
        let mut before = Vec::new();
        let mut after = Vec::new();
        for (index, baseline) in order.into_iter().enumerate() {
            let warm = Instant::now();
            while warm.elapsed() < Duration::from_millis(settings.phase_warm_ms) {
                step(baseline, iteration, false);
                iteration += 1;
            }
            let initial = usage();
            let start = Instant::now();
            let mut calls = 0usize;
            loop {
                let batches = if case.callers == 1 { 32 } else { 1 };
                for _ in 0..batches {
                    step(baseline, iteration, false);
                    iteration += 1;
                    calls += case.callers;
                }
                if start.elapsed() >= Duration::from_millis(settings.sample_ms) {
                    break;
                }
            }
            let wall_ns = start.elapsed().as_nanos() as f64 / calls as f64;
            let final_usage = usage();
            let sample = Sample {
                wall_ns,
                cpu_ns: (final_usage.cpu_ns - initial.cpu_ns) as f64 / calls as f64,
            };
            step(baseline, iteration, true);
            let role = if baseline { "baseline" } else { "candidate" };
            let switches = final_usage.switches - initial.switches;
            let minor_faults = final_usage.minor_faults - initial.minor_faults;
            println!(
                "{{\"kind\":\"phase\",\"case\":\"{case_id}\",\"round\":{round},\"index\":{index},\"role\":\"{role}\",\"calls\":{calls},\"wall_ns\":{},\"cpu_ns\":{},\"switches\":{switches},\"minor_faults\":{minor_faults}}}",
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
            "{{\"kind\":\"comparison\",\"case\":\"{case_id}\",\"round\":{round},\"wall_delta_pct\":{},\"cpu_delta_pct\":{},\"base_drift\":{base_drift},\"next_drift\":{next_drift},\"base_cpu_drift\":{base_cpu_drift},\"next_cpu_drift\":{next_cpu_drift},\"over_limit\":{over_limit}}}",
            (next_wall / base_wall - 1.0) * 100.0,
            (next_cpu / base_cpu - 1.0) * 100.0
        );
    }
}

struct Waves {
    start: Barrier,
    done: Barrier,
    baseline: AtomicBool,
    iteration: AtomicUsize,
    verify: AtomicBool,
    stop: AtomicBool,
    valid: AtomicBool,
}

impl Waves {
    fn step(&self, baseline: bool, iteration: usize, verify: bool) {
        self.baseline.store(baseline, Ordering::Release);
        self.iteration.store(iteration, Ordering::Release);
        self.verify.store(verify, Ordering::Release);
        self.start.wait();
        self.done.wait();
        if verify {
            assert!(
                self.valid.load(Ordering::Acquire),
                "concurrent recovery failed"
            );
        }
    }
}

struct StopWorkers<'a>(&'a Waves);

impl Drop for StopWorkers<'_> {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Release);
        self.0.start.wait();
    }
}

fn concurrent(
    case: Case,
    settings: Settings,
    expected: &[Vec<u8>],
    baseline: impl Fn(&mut Shards) -> bool + Sync,
    candidate: impl Fn(&mut Shards) -> bool + Sync,
) {
    let waves = Waves {
        start: Barrier::new(case.callers + 1),
        done: Barrier::new(case.callers + 1),
        baseline: AtomicBool::new(true),
        iteration: AtomicUsize::new(0),
        verify: AtomicBool::new(false),
        stop: AtomicBool::new(false),
        valid: AtomicBool::new(true),
    };
    // Each worker uses the same caller buffers throughout both implementations.
    let mut buffers: Vec<Vec<_>> = (0..case.callers)
        .map(|_| {
            expected
                .iter()
                .cloned()
                .map(|bytes| (bytes, true))
                .collect()
        })
        .collect();
    std::thread::scope(|scope| {
        for shards in &mut buffers {
            let waves = &waves;
            let baseline = &baseline;
            let candidate = &candidate;
            scope.spawn(move || {
                let mut valid = true;
                loop {
                    waves.start.wait();
                    if waves.stop.load(Ordering::Acquire) {
                        break;
                    }
                    if waves.verify.load(Ordering::Acquire) {
                        valid &= correct(shards, expected);
                    } else if valid {
                        valid &= catch_unwind(AssertUnwindSafe(|| {
                            case.prepare(shards, waves.iteration.load(Ordering::Acquire));
                            if waves.baseline.load(Ordering::Acquire) {
                                baseline(shards)
                            } else {
                                candidate(shards)
                            }
                        }))
                        .unwrap_or(false);
                    }
                    if !valid {
                        waves.valid.store(false, Ordering::Release);
                    }
                    waves.done.wait();
                }
            });
        }
        // Releasing the workers also runs if a correctness assertion unwinds.
        let stop = StopWorkers(&waves);
        compare(case, settings, |baseline, iteration, verify| {
            waves.step(baseline, iteration, verify)
        });
        drop(stop);
    });
}

fn setting(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .map(|value| value.parse().unwrap())
        .unwrap_or(default)
}

#[cfg(target_os = "linux")]
fn pin_cpus() -> Option<usize> {
    let selected = std::env::var("RSE_CACHE_PIN_CPUS").ok()?;
    let cpus: Vec<usize> = selected
        .split(',')
        .map(|cpu| cpu.parse().unwrap())
        .collect();
    // SAFETY: an all-zero cpu_set_t is a valid empty CPU mask.
    let mut mask: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    for &cpu in &cpus {
        assert!(cpu < libc::CPU_SETSIZE as usize);
        // SAFETY: cpu is within the mask's checked capacity.
        unsafe { libc::CPU_SET(cpu, &mut mask) };
    }
    // SAFETY: mask is a valid initialized CPU set with the matching byte size.
    assert_eq!(
        unsafe { libc::sched_setaffinity(0, std::mem::size_of_val(&mask), &mask) },
        0
    );
    Some(cpus.len())
}

#[cfg(not(target_os = "linux"))]
fn pin_cpus() -> Option<usize> {
    assert!(
        std::env::var_os("RSE_CACHE_PIN_CPUS").is_none(),
        "probe CPU affinity requires Linux"
    );
    None
}

fn main() {
    let pinned = pin_cpus()
        .map(|count| count.to_string())
        .unwrap_or_else(|| "null".into());
    let settings = Settings {
        rounds: setting("RSE_CACHE_ROUNDS", 3),
        warm_ms: setting("RSE_CACHE_WARM_MS", 1000),
        phase_warm_ms: setting("RSE_CACHE_PHASE_WARM_MS", 100),
        sample_ms: setting("RSE_CACHE_SAMPLE_MS", 250),
    };
    assert!(settings.rounds > 0 && settings.sample_ms > 0);
    let filter = std::env::var("RSE_CACHE_FILTER").unwrap_or_default();
    println!(
        "{{\"kind\":\"settings\",\"rounds\":{},\"warm_ms\":{},\"phase_warm_ms\":{},\"sample_ms\":{},\"pinned_cpus\":{pinned},\"drift_limit\":0.05}}",
        settings.rounds, settings.warm_ms, settings.phase_warm_ms, settings.sample_ms
    );
    let mut completed = 0usize;
    for callers in [1usize, 8] {
        for data in [10usize, 252, 253, 300] {
            for bytes in [64usize, 4096] {
                for cold in [false, true] {
                    let case = Case {
                        data,
                        bytes,
                        cold,
                        callers,
                    };
                    if !case.id().contains(&filter) {
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
                    if callers == 1 {
                        let mut shards: Vec<_> = expected
                            .iter()
                            .cloned()
                            .map(|bytes| (bytes, true))
                            .collect();
                        compare(case, settings, |baseline, iteration, verify| {
                            if verify {
                                assert!(correct(&shards, &expected));
                            } else {
                                case.prepare(&mut shards, iteration);
                                if baseline {
                                    base.reconstruct(&mut shards).unwrap();
                                } else {
                                    next.reconstruct(&mut shards).unwrap();
                                }
                            }
                        });
                    } else {
                        concurrent(
                            case,
                            settings,
                            &expected,
                            |shards| base.reconstruct(shards).is_ok(),
                            |shards| next.reconstruct(shards).is_ok(),
                        );
                    }
                    completed += 1;
                }
            }
        }
    }
    assert!(completed > 0, "case filter selected no workload");
}
