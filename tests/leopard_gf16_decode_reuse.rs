#![cfg(target_endian = "little")]

use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};

fn fixture(
    data: usize,
    parity: usize,
    len: usize,
    jobs: usize,
    seed: usize,
) -> (ReedSolomon, Vec<Vec<u8>>) {
    let codec = ReedSolomon::with_options(
        data,
        parity,
        CodecOptions::builder()
            .codec_family(CodecFamily::LeopardGF16)
            .max_parallel_jobs(jobs)
            .build(),
    )
    .unwrap();
    let mut shards: Vec<Vec<u8>> = (0..data + parity)
        .map(|shard| {
            (0..len)
                .map(|offset| (offset * 29 + offset / 67 + shard * 47 + seed * 31) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    (codec, shards)
}

fn recover_selected(
    codec: &ReedSolomon,
    expected: &[Vec<u8>],
    missing: &[usize],
    requested: &[usize],
) {
    // Odd offsets prove the input conversion does not assume u16 alignment.
    let mut storage: Vec<Vec<u8>> = expected
        .iter()
        .map(|bytes| {
            let mut buffer = Vec::with_capacity(bytes.len() + 1);
            buffer.push(0x7b);
            buffer.extend_from_slice(bytes);
            buffer
        })
        .collect();
    let mut slots: Vec<_> = storage
        .iter_mut()
        .map(|buffer| ShardSlot::new_present(&mut buffer[1..]))
        .collect();
    for &index in missing {
        slots[index].mark_missing();
        slots[index].as_inner_mut().fill(0xa5);
    }
    let mut required = vec![false; expected.len()];
    for &index in requested {
        required[index] = true;
    }
    codec.reconstruct_some(&mut slots, &required).unwrap();
    for (index, slot) in slots.iter().enumerate() {
        if missing.contains(&index) && !required[index] {
            assert!(!slot.is_present());
            assert!(slot.as_inner().iter().all(|byte| *byte == 0xa5));
        } else {
            assert!(slot.is_present());
            assert_eq!(*slot.as_inner(), expected[index].as_slice());
        }
    }
    codec.reconstruct(&mut slots).unwrap();
    for (slot, expected) in slots.iter().zip(expected) {
        assert_eq!(*slot.as_inner(), expected.as_slice());
    }
    drop(slots);
    assert!(storage.iter().all(|buffer| buffer[0] == 0x7b));
}

#[test]
fn alternating_shapes_patterns_and_tail_sizes_preserve_requested_outputs() {
    for jobs in [1, 2] {
        for (round, (data, parity, len)) in [
            (4, 3, 64),
            (10, 4, 32 * 1024),
            (10, 4, 128 * 1024 + 64),
            (6, 3, 64 * 1024 + 64),
            (3, 5, 192),
            (10, 4, 128 * 1024 + 64),
        ]
        .into_iter()
        .enumerate()
        {
            for seed in [round, round + 19] {
                let (codec, expected) = fixture(data, parity, len, jobs, seed);
                recover_selected(&codec, &expected, &[0, 1, data], &[0, data]);
                recover_selected(&codec, &expected, &[data, data + 1], &[data + 1]);
            }
        }
    }
}

#[cfg(feature = "std")]
#[test]
fn concurrent_recoveries_share_plans_without_sharing_mutable_buffers() {
    use rayon::prelude::*;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        (0..12usize).into_par_iter().for_each(|seed| {
            let len = 64 * 1024 + 64 * (seed % 3 + 1);
            let (codec, expected) = fixture(4, 3, len, 2, seed);
            recover_selected(&codec, &expected, &[0, 1, 4], &[0, 4]);
        });
    });
}

#[cfg(feature = "std")]
#[test]
fn parallel_groups_preserve_serial_results_for_irregular_chunks_and_limits() {
    for threads in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            for jobs in [0, 1, 2, 3, 8] {
                for len in [
                    64,
                    64 * 1024,
                    64 * 1024 + 64,
                    3 * 64 * 1024 + 64,
                    5 * 64 * 1024,
                    7 * 64 * 1024 + 64,
                ] {
                    let (codec, expected) = fixture(4, 3, len, jobs, threads + jobs);
                    recover_selected(&codec, &expected, &[0, 1, 4], &[0, 4]);
                }
            }
        });
    }
}

#[cfg(feature = "std")]
mod allocations {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    // An isolated child uses one total order for the gate and allocator hooks
    // on all Rayon workers. Pool completion bounds the synchronous window.
    static ALL_ENABLED: AtomicBool = AtomicBool::new(false);
    static ALL_CALLS: AtomicUsize = AtomicUsize::new(0);
    static ALL_BYTES: AtomicUsize = AtomicUsize::new(0);

    fn measure_all<T>(run: impl FnOnce() -> T) -> (T, (usize, usize)) {
        assert!(!ALL_ENABLED.load(Ordering::SeqCst));
        ALL_CALLS.store(0, Ordering::SeqCst);
        ALL_BYTES.store(0, Ordering::SeqCst);
        struct Stop;
        impl Drop for Stop {
            fn drop(&mut self) {
                ALL_ENABLED.store(false, Ordering::SeqCst);
            }
        }
        ALL_ENABLED.store(true, Ordering::SeqCst);
        let stop = Stop;
        let result = run();
        drop(stop);
        (
            result,
            (
                ALL_CALLS.load(Ordering::SeqCst),
                ALL_BYTES.load(Ordering::SeqCst),
            ),
        )
    }

    // Measure only the caller thread; max_parallel_jobs=1 keeps decode here.
    thread_local! {
        static MEASURE: Cell<Option<(usize, usize, usize)>> = const { Cell::new(None) };
    }

    fn record(bytes: usize) {
        if ALL_ENABLED.load(Ordering::SeqCst) {
            ALL_CALLS.fetch_add(1, Ordering::SeqCst);
            ALL_BYTES.fetch_add(bytes, Ordering::SeqCst);
        }
        let _ = MEASURE.try_with(|state| {
            if let Some((threshold, total, large)) = state.get() {
                state.set(Some((
                    threshold,
                    total + bytes,
                    large + usize::from(bytes >= threshold),
                )));
            }
        });
    }

    struct CountingAllocator;
    // SAFETY: all pointer and layout arguments are forwarded unchanged to
    // System. Scalar bookkeeping does not allocate or access user memory.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record(layout.size());
            // SAFETY: the caller supplies a valid allocation layout.
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record(layout.size());
            // SAFETY: the caller supplies a valid allocation layout.
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record(new_size);
            // SAFETY: the original System allocation and new size are forwarded.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: the pointer and layout match the original System allocation.
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    #[test]
    fn warmed_decode_allocates_only_requested_temporary_output_and_small_metadata() {
        const CHILD: &str = "RSE_GF16_DECODE_ALLOCATION_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // Thread-local allocation counting excludes other workers, but the
            // decode-plan LRU is process-wide. Other pattern/concurrency tests
            // could evict our warmed plan between warmup and measurement. Run
            // this exact test alone so it measures a stable cache hit, including
            // when sanitizer instrumentation changes the test interleaving.
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "allocations::warmed_decode_allocates_only_requested_temporary_output_and_small_metadata",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(
                status.success(),
                "isolated GF16 allocation regression failed"
            );
            return;
        }

        for len in [32 * 1024, 2 * 1024 * 1024 + 64] {
            let (codec, expected) = fixture(10, 4, len, 1, 11);
            let mut slots: Vec<_> = expected
                .iter()
                .cloned()
                .map(ShardSlot::new_present)
                .collect();
            slots[0].mark_missing();
            codec.reconstruct(&mut slots).unwrap();
            slots[0].mark_missing();
            slots[0].as_inner_mut().fill(0xa5);

            MEASURE.with(|state| state.set(Some((len, 0, 0))));
            let result = codec.reconstruct(&mut slots);
            let (_, bytes, large_allocations) = MEASURE.with(|state| state.replace(None).unwrap());
            result.unwrap();

            // The core intentionally decodes requested outputs into temporary
            // storage before publishing them to caller-owned shard containers.
            assert_eq!(
                large_allocations, 1,
                "{len}-byte shards: {bytes} allocated bytes"
            );
            assert!(
                bytes <= len + 4096,
                "unexpected scratch/input allocation: {bytes} bytes"
            );
            assert_eq!(slots[0].as_inner(), &expected[0]);
        }
    }

    #[test]
    fn cold_decode_reuses_conversion_storage_for_output_scaling() {
        const CHILD: &str = "RSE_GF16_COLD_ALLOCATION_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "allocations::cold_decode_reuses_conversion_storage_for_output_scaling",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(
                status.success(),
                "isolated cold GF16 resource regression failed"
            );
            return;
        }

        // Warm only the process-wide immutable plan. Every measured recovery
        // runs on a fresh thread, so its mutable decode workspace starts cold.
        let (codec, expected) = fixture(10, 4, 64, 1, 7);
        let mut slots: Vec<_> = expected.into_iter().map(ShardSlot::new_present).collect();
        slots[0].mark_missing();
        codec.reconstruct(&mut slots).unwrap();

        let mut within_budget = true;
        for repetition in 0..3 {
            for len in [32 * 1024, 64 * 1024, 128 * 1024 + 64] {
                let (bytes, calls) = std::thread::spawn(move || {
                    let (codec, expected) = fixture(10, 4, len, 1, 29 + repetition);
                    let mut slots: Vec<_> = expected
                        .iter()
                        .cloned()
                        .map(ShardSlot::new_present)
                        .collect();
                    slots[0].mark_missing();
                    slots[0].as_inner_mut().fill(0xa5);
                    MEASURE.with(|state| state.set(Some((0, 0, 0))));
                    let result = codec.reconstruct(&mut slots);
                    let (_, bytes, calls) = MEASURE.with(|state| state.replace(None).unwrap());
                    result.unwrap();
                    for (slot, expected) in slots.iter().zip(&expected) {
                        assert!(slot.is_present());
                        assert_eq!(slot.as_inner(), expected);
                    }
                    (bytes, calls)
                })
                .join()
                .unwrap();
                let chunk_bytes = len.min(64 * 1024);
                // 16 FFT lanes, one conversion/scaling buffer, the requested
                // temporary output, and a small allowance for API metadata.
                let budget = 17 * chunk_bytes + len + 4096;
                println!(
                    "GF16_COLD_RESOURCE repetition={repetition} len={len} calls={calls} bytes={bytes} budget={budget}"
                );
                within_budget &= bytes <= budget;
            }
        }
        // With 64 transform lanes, removing one scratch brings 64000-byte
        // shards below 4 MiB. Full 64 KiB chunks must still be released.
        for repetition in 0..3 {
            for len in [64000, 64 * 1024] {
                let (bytes, calls) = std::thread::spawn(move || {
                    let (codec, expected) = fixture(40, 16, len, 1, 53 + repetition);
                    let mut slots: Vec<_> = expected
                        .iter()
                        .cloned()
                        .map(ShardSlot::new_present)
                        .collect();
                    slots[0].mark_missing();
                    codec.reconstruct(&mut slots).unwrap();
                    slots[0].mark_missing();
                    slots[0].as_inner_mut().fill(0xa5);
                    MEASURE.with(|state| state.set(Some((0, 0, 0))));
                    let result = codec.reconstruct(&mut slots);
                    let (_, bytes, calls) = MEASURE.with(|state| state.replace(None).unwrap());
                    result.unwrap();
                    for (slot, expected) in slots.iter().zip(&expected) {
                        assert!(slot.is_present());
                        assert_eq!(slot.as_inner(), expected);
                    }
                    (bytes, calls)
                })
                .join()
                .unwrap();
                let workspace_bytes = 65 * len;
                let cacheable = workspace_bytes <= 4 * 1024 * 1024;
                println!(
                    "GF16_CACHE_RESOURCE repetition={repetition} len={len} cacheable={cacheable} calls={calls} bytes={bytes} workspace_bytes={workspace_bytes}"
                );
                within_budget &= if cacheable {
                    bytes <= len + 4096
                } else {
                    bytes >= 64 * len && bytes <= workspace_bytes + len + 4096
                };
            }
        }
        assert!(
            within_budget,
            "decode scratch allocation/retention budget changed"
        );
    }
    #[test]
    fn parallel_output_views_are_bounded_by_groups_not_chunks() {
        const CHILD: &str = "RSE_GF16_PARALLEL_VIEWS_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "allocations::parallel_output_views_are_bounded_by_groups_not_chunks",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(
                status.success(),
                "isolated parallel output-view resource regression failed"
            );
            return;
        }
        let mut bounded = true;
        for (threads, jobs) in [(2, 2), (4, 0), (4, 3)] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let (serial, warm) = fixture(10, 4, 64 * 1024, 1, 17);
            // Warm the same work shape on every worker, not just whichever
            // worker happens to run the first parallel operation.
            pool.broadcast(|_| {
                let mut slots: Vec<_> = warm.iter().cloned().map(ShardSlot::new_present).collect();
                for index in [0, 1, 10] {
                    slots[index].mark_missing();
                }
                serial.reconstruct(&mut slots).unwrap();
            });
            for repetition in 0..3 {
                let mut counts = Vec::new();
                for chunks in [12, 36] {
                    let len = (chunks - 1) * 64 * 1024 + 64;
                    let (codec, expected) = fixture(10, 4, len, jobs, 41 + repetition);
                    let mut slots: Vec<_> = expected
                        .iter()
                        .cloned()
                        .map(ShardSlot::new_present)
                        .collect();
                    for index in [0, 1, 10] {
                        slots[index].mark_missing();
                    }
                    pool.install(|| codec.reconstruct(&mut slots)).unwrap();
                    for index in [0, 1, 10] {
                        slots[index].mark_missing();
                        slots[index].as_inner_mut().fill(0xa5);
                    }
                    let (result, count) =
                        measure_all(|| pool.install(|| codec.reconstruct(&mut slots)));
                    result.unwrap();
                    for (slot, expected) in slots.iter().zip(&expected) {
                        assert!(slot.is_present());
                        assert_eq!(slot.as_inner(), expected);
                    }
                    // Requested temporary payload grows with length. Compare
                    // the remaining requests/bytes, including all worker work.
                    counts.push((count.0, count.1, count.1 - 3 * len));
                }
                println!(
                    "PARALLEL_VIEW_RESOURCE threads={threads} jobs={jobs} repetition={repetition} short_chunks=12 long_chunks=36 short_calls={} short_bytes={} short_overhead={} long_calls={} long_bytes={} long_overhead={}",
                    counts[0].0, counts[0].1, counts[0].2, counts[1].0, counts[1].1, counts[1].2
                );
                bounded &= counts[0].0 == counts[1].0 && counts[0].2 == counts[1].2;
            }
        }
        assert!(
            bounded,
            "parallel output-view metadata grew with chunk count"
        );
    }
}
