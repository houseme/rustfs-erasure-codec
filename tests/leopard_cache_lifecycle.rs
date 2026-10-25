//! Controlled cache-pressure experiment, not a timing or RSS benchmark.
#![cfg(all(feature = "std", target_endian = "little"))]

use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Barrier};

// Track successful allocation sizes for the entire process lifetime. Starting
// live accounting at the measurement gate would miscount frees of warm caches.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);

struct Counter;
#[global_allocator]
static COUNTER: Counter = Counter;

fn add_live(bytes: usize) {
    let live = LIVE.fetch_add(bytes, SeqCst) + bytes;
    if ENABLED.load(SeqCst) {
        PEAK.fetch_max(live, SeqCst);
    }
}
fn request(bytes: usize) {
    if ENABLED.load(SeqCst) {
        CALLS.fetch_add(1, SeqCst);
        REQUESTED.fetch_add(bytes, SeqCst);
    }
}
// SAFETY: the original pointer/layout contracts are forwarded to System. Only
// successful requests alter live sizes; failed realloc retains the old charge.
// Bookkeeping uses scalar atomics, allocates nothing, and never reads payloads.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        request(layout.size());
        // SAFETY: forward the unchanged valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            add_live(layout.size());
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        request(layout.size());
        // SAFETY: preserve the original layout and zero-initialization contract.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            add_live(layout.size());
        }
        ptr
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        request(new_size);
        // SAFETY: the caller supplies the live System allocation and valid size.
        let result = unsafe { System.realloc(ptr, layout, new_size) };
        if !result.is_null() {
            if new_size >= layout.size() {
                add_live(new_size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - new_size, SeqCst);
            }
        }
        result
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), SeqCst);
        // SAFETY: forward the original live allocation and matching layout.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[derive(Debug)]
struct Sample {
    before: usize,
    peak: usize,
    after: usize,
    calls: usize,
    requested: usize,
}
impl Sample {
    fn retained_delta(&self) -> i128 {
        self.after as i128 - self.before as i128
    }
    fn peak_extra(&self) -> usize {
        self.peak - self.before
    }
}
fn measure(run: impl FnOnce()) -> Sample {
    assert!(!ENABLED.load(SeqCst));
    let before = LIVE.load(SeqCst);
    PEAK.store(before, SeqCst);
    CALLS.store(0, SeqCst);
    REQUESTED.store(0, SeqCst);
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            ENABLED.store(false, SeqCst);
        }
    }
    ENABLED.store(true, SeqCst);
    let stop = Stop;
    run();
    drop(stop);
    Sample {
        before,
        peak: PEAK.load(SeqCst),
        after: LIVE.load(SeqCst),
        calls: CALLS.load(SeqCst),
        requested: REQUESTED.load(SeqCst),
    }
}

fn check_counter() {
    let old = Layout::from_size_align(64, 8).unwrap();
    let zeroed = Layout::from_size_align(32, 8).unwrap();
    // SAFETY: all pointers below are checked, resized with their current
    // layouts, and eventually freed exactly once using their final layouts.
    unsafe {
        let mut ptr = std::alloc::alloc(old);
        assert!(!ptr.is_null());
        let sample = measure(|| {
            let zero = std::alloc::alloc_zeroed(zeroed);
            assert!(!zero.is_null());
            assert_eq!(*zero, 0);
            ptr = std::alloc::realloc(ptr, old, 96);
            assert!(!ptr.is_null());
            ptr = std::alloc::realloc(ptr, Layout::from_size_align(96, 8).unwrap(), 16);
            assert!(!ptr.is_null());
            std::alloc::dealloc(zero, zeroed);
        });
        assert_eq!(sample.calls, 3);
        assert_eq!(sample.requested, 32 + 96 + 16);
        assert_eq!(sample.retained_delta(), -48);
        assert_eq!(sample.peak_extra(), 64);
        std::alloc::dealloc(ptr, Layout::from_size_align(16, 8).unwrap());
    }
}

const POLICY: &str = "drop_smaller_cache";
const LEN: usize = 64 * 1024;
// 16 FFT work lanes plus one conversion/scaling lane, each LEN bytes.
const SMALL_WORK_BYTES: usize = 17 * LEN;
const PHASES: [(bool, &str); 7] = [
    (false, "small_hit"),
    (true, "oversized_first"),
    (false, "small_after_oversized"),
    (false, "small_hit_again"),
    (true, "oversized_again"),
    (true, "oversized_repeat"),
    (false, "small_after_repeat"),
];

struct Encoded {
    codec: ReedSolomon,
    shards: Vec<Vec<u8>>,
}
impl Encoded {
    fn new(data: usize, parity: usize) -> Self {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let mut shards: Vec<Vec<u8>> = (0..data + parity)
            .map(|row| {
                (0..LEN)
                    .map(|offset| (row * 37 + offset * 19 + offset / 67) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut shards).unwrap();
        Self { codec, shards }
    }
}
struct Recovery {
    expected: Arc<Encoded>,
    slots: Vec<ShardSlot<Vec<u8>>>,
}
impl Recovery {
    fn new(expected: &Arc<Encoded>) -> Self {
        Self {
            expected: expected.clone(),
            slots: expected
                .shards
                .iter()
                .cloned()
                .map(ShardSlot::new_present)
                .collect(),
        }
    }
    // Catch a decoder panic so every worker still reaches the phase barriers;
    // report failure only after joining instead of leaving the parent waiting.
    fn recover(&mut self) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let pointer = self.slots[0].as_inner().as_ptr();
            self.slots[0].as_inner_mut().fill(0xa5);
            self.slots[0].mark_missing();
            self.expected.codec.reconstruct(&mut self.slots).is_ok()
                && self.slots[0].as_inner().as_ptr() == pointer
                && self
                    .slots
                    .iter()
                    .zip(&self.expected.shards)
                    .all(|(slot, expected)| slot.is_present() && slot.as_inner() == expected)
        }))
        .unwrap_or(false)
    }
}

#[test]
fn alternating_cache_sizes_report_live_retention_and_concurrent_peaks() {
    const CHILD: &str = "RSE_GF16_CACHE_LIFECYCLE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "alternating_cache_sizes_report_live_retention_and_concurrent_peaks",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "isolated cache lifecycle experiment failed"
        );
        return;
    }
    check_counter();
    let small = Arc::new(Encoded::new(10, 4));
    let large = Arc::new(Encoded::new(40, 16));
    // Populate both process-wide immutable decode plans before the workers;
    // workers only warm the small mutable workspace in their own TLS cache.
    let (warm_small, warm_large) = (small.clone(), large.clone());
    assert!(
        std::thread::spawn(move || {
            Recovery::new(&warm_small).recover() && Recovery::new(&warm_large).recover()
        })
        .join()
        .unwrap()
    );
    // Joining also releases the prewarming thread's mutable TLS workspace;
    // only the shared immutable plans survive into either policy experiment.

    for threads in [1, 2, 4] {
        for repetition in 0..3 {
            // Force every worker through parking_lot's blocking path before
            // counting. A warm uncontended decode need not initialize the
            // parking table used by the shared immutable-plan cache's mutex.
            let ready = Arc::new((
                parking_lot::Mutex::new((0, false)),
                parking_lot::Condvar::new(),
                parking_lot::Condvar::new(),
            ));
            let start = Arc::new(Barrier::new(threads + 1));
            let done = Arc::new(Barrier::new(threads + 1));
            let release = Arc::new(Barrier::new(threads + 1));
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    let (mut small, mut large) = (Recovery::new(&small), Recovery::new(&large));
                    let (ready, start, done, release) =
                        (ready.clone(), start.clone(), done.clone(), release.clone());
                    std::thread::spawn(move || {
                        let mut valid = small.recover();
                        {
                            let mut state = ready.0.lock();
                            state.0 += 1;
                            ready.1.notify_one();
                            while !state.1 {
                                ready.2.wait(&mut state);
                            }
                        }
                        for (oversized, _) in PHASES {
                            start.wait();
                            // Do not short-circuit later recoveries after an
                            // earlier failure: every phase must still execute.
                            valid &= if oversized {
                                large.recover()
                            } else {
                                small.recover()
                            };
                            done.wait();
                            // Keep owners and TLS caches alive until the parent
                            // has captured the end and peak of this window.
                            release.wait();
                        }
                        valid
                    })
                })
                .collect();
            {
                let mut state = ready.0.lock();
                while state.0 != threads {
                    ready.1.wait(&mut state);
                }
                // Each worker still holds the mutex until wait has registered
                // its parking state and unlocked it. This guarantees all TLS
                // parking data and hash-table growth precede the sample gate.
                state.1 = true;
                assert_eq!(ready.2.notify_all(), threads);
            }
            let mut previous_large = false;
            let mut valid = true;
            for (oversized, name) in PHASES {
                let sample = measure(|| {
                    start.wait();
                    done.wait();
                });
                let retained = match (previous_large, oversized) {
                    (false, true) => -(threads as i128 * SMALL_WORK_BYTES as i128),
                    (true, false) => threads as i128 * SMALL_WORK_BYTES as i128,
                    _ => 0,
                };
                // Tiny per-thread synchronization metadata can be initialized
                // on first contention. Keep a bounded allowance, not an exact
                // process-wide peak target that depends on worker overlap.
                valid &= (sample.retained_delta() - retained).abs() <= 4096;
                valid &= sample.peak >= sample.before.max(sample.after);
                let payload = if oversized {
                    // 64 FFT lanes, one scratch lane and one temporary output.
                    threads * 66 * LEN
                } else if previous_large {
                    // Rebuild 16 work lanes and scratch; publish one output.
                    threads * 18 * LEN
                } else {
                    threads * LEN
                };
                valid &= sample.requested >= payload;
                valid &= sample.requested <= payload + threads * 4096;
                valid &= sample.peak_extra() <= sample.requested;
                println!(
                    "GF16_CACHE_LIFECYCLE policy={POLICY} threads={threads} repetition={repetition} phase={name} oversized={oversized} live_before={} live_peak={} live_after={} retained_delta={} expected_cache_delta={retained} peak_extra={} calls={} requested_bytes={}",
                    sample.before,
                    sample.peak,
                    sample.after,
                    sample.retained_delta(),
                    sample.peak_extra(),
                    sample.calls,
                    sample.requested
                );
                previous_large = oversized;
                release.wait();
            }
            for worker in workers {
                valid &= worker.join().unwrap();
            }
            assert!(valid, "cache retention or recovered payload differs");
        }
    }
}
