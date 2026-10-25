use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};

fn codec(family: CodecFamily, data: usize, parity: usize) -> ReedSolomon {
    ReedSolomon::with_options(
        data,
        parity,
        CodecOptions::builder().codec_family(family).build(),
    )
    .unwrap()
}

fn data_shards(count: usize, len: usize, seed: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|shard| {
            (0..len)
                .map(|offset| (seed + shard * 29 + offset * 17 + offset / 64 * 13) as u8)
                .collect()
        })
        .collect()
}

#[test]
fn reused_encode_matches_fresh_threads_across_shapes_sizes_and_inputs() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        // Adjacent cases share the same rounded parity size, data count or
        // parity count. A cache keyed by only one of these will be incorrect.
        for (index, (data_count, parity_count, len)) in [
            (3, 2, 64),
            (10, 4, 32 * 1024 + 64),
            (11, 4, 64 * 1024 + 64),
            (11, 3, 192),
            (3, 7, 64 * 1024 + 64),
            (10, 4, 64),
            (1, 1, 192),
            (17, 1, 64),
            (3, 2, 64 * 1024 + 64),
        ]
        .into_iter()
        .enumerate()
        {
            let fresh = std::thread::spawn(move || {
                let rs = codec(family, data_count, parity_count);
                let data = data_shards(data_count, len, index);
                let mut parity = vec![vec![0; len]; parity_count];
                rs.encode_sep(&data, &mut parity).unwrap();
                parity
            })
            .join()
            .unwrap();

            let rs = codec(family, data_count, parity_count);
            let data = data_shards(data_count, len, index);
            let mut parity = vec![vec![0xa5; len]; parity_count];
            rs.encode_sep(&data, &mut parity).unwrap();
            assert_eq!(parity, fresh, "{family:?}, case {index}");

            // Every active work lane must be initialized even when reusing
            // buffers containing the previous nonzero FFT output.
            let zeros = vec![vec![0; len]; data_count];
            rs.encode_sep(&zeros, &mut parity).unwrap();
            assert!(parity.iter().flatten().all(|&value| value == 0));
            rs.encode_sep(&data, &mut parity).unwrap();
            assert_eq!(parity, fresh, "{family:?}, warmed case {index}");
        }
    }
}

#[test]
fn multichunk_encode_matches_independent_wire_blocks() {
    for (family, data_count, parity_count) in [
        (CodecFamily::LeopardGF8, 10, 4),
        (CodecFamily::LeopardGF16, 10, 4),
        (CodecFamily::LeopardGF16, 4, 32),
        (CodecFamily::LeopardGF16, 4, 64),
    ] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let rs = codec(family, data_count, parity_count);
        let len = 64 * 1024 + 192;
        let data = data_shards(data_count, len, 97);
        let mut whole = vec![vec![0; len]; parity_count];
        rs.encode_sep(&data, &mut whole).unwrap();

        let mut block_parity = vec![vec![0; 64]; parity_count];
        for offset in (0..len).step_by(64) {
            let block_data: Vec<_> = data.iter().map(|s| &s[offset..offset + 64]).collect();
            rs.encode_sep(&block_data, &mut block_parity).unwrap();
            for (expected, actual) in block_parity.iter().zip(&whole) {
                assert_eq!(
                    expected,
                    &actual[offset..offset + 64],
                    "{family:?}, {offset}"
                );
            }
        }
    }
}

#[cfg(feature = "std")]
#[test]
fn leopard_profile_counters_follow_benchmark_metrics_feature() {
    let before = rustfs_erasure_codec::leopard_gf8_profile_stats();
    let rs = codec(CodecFamily::LeopardGF8, 10, 4);
    let data = data_shards(10, 64, 0);
    let mut parity = vec![vec![0; 64]; 4];
    rs.encode_sep(&data, &mut parity).unwrap();
    let after = rustfs_erasure_codec::leopard_gf8_profile_stats();
    if cfg!(feature = "benchmark-metrics") {
        assert!(after.encode_calls > before.encode_calls);
        assert!(after.input_copy_bytes > before.input_copy_bytes);
        assert!(after.output_writeback_bytes > before.output_writeback_bytes);
    } else {
        assert_eq!(after, before);
        assert_eq!(after.encode_calls, 0);
        assert_eq!(after.input_copy_bytes, 0);
        assert_eq!(after.output_writeback_bytes, 0);
    }
}

#[cfg(all(feature = "std", target_endian = "little"))]
mod allocations {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    struct CountingAllocator;

    thread_local! {
        static ALLOCATIONS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
    }

    fn record(bytes: usize) {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some((calls, total)) = count.get() {
                count.set(Some((calls + 1, total + bytes)));
            }
        });
    }

    fn measure<T>(run: impl FnOnce() -> T) -> (T, (usize, usize)) {
        ALLOCATIONS.with(|count| count.set(Some((0, 0))));
        let result = run();
        let counts = ALLOCATIONS.with(|count| count.replace(None).unwrap());
        (result, counts)
    }

    // SAFETY: every allocation is forwarded unchanged to System; the counter
    // only records calls on the currently measured thread and never allocates.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record(layout.size());
            // SAFETY: the caller supplies the allocator's required layout.
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record(layout.size());
            // SAFETY: the caller supplies the allocator's required layout.
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: ptr and layout are forwarded to their original allocator.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record(new_size);
            // SAFETY: the caller's valid System allocation and layout are unchanged.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    #[test]
    fn warmed_leopard_encode_reuses_plans_and_work_without_allocating() {
        for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
            for len in [32 * 1024, 64 * 1024 + 64] {
                let rs = codec(family, 10, 4);
                let data = data_shards(10, len, 23);
                let mut parity = vec![vec![0; len]; 4];
                rs.encode_sep(&data, &mut parity).unwrap();

                let (result, counts) = measure(|| rs.encode_sep(&data, &mut parity));
                result.unwrap();
                assert_eq!(counts, (0, 0), "{family:?}, {len} bytes");
            }
        }
    }

    #[test]
    fn gf16_encode_uses_only_active_work_storage_and_bounded_caches() {
        let mut within_budget = true;
        for repetition in 0..3 {
            for (data_count, parity_count, len) in [
                (10, 4, 32 * 1024),
                (10, 4, 64 * 1024),
                (10, 4, 128 * 1024 + 64),
                (4, 32, 64 * 1024),
                (4, 64, 64 * 1024),
            ] {
                let data = data_shards(data_count, len, 31 + repetition);
                let rs = codec(CodecFamily::LeopardGF16, data_count, parity_count);
                let mut expected = vec![vec![0; len]; parity_count];
                rs.encode_sep(&data, &mut expected).unwrap();
                let (cold, warm) = std::thread::spawn(move || {
                    // Tables are process-wide and already initialized; a fresh
                    // thread has no encode plan, work or scratch cache.
                    let rs = codec(CodecFamily::LeopardGF16, data_count, parity_count);
                    let mut parity = vec![vec![0xa5; len]; parity_count];
                    let (result, cold) = measure(|| rs.encode_sep(&data, &mut parity));
                    result.unwrap();
                    assert_eq!(parity, expected);
                    parity.iter_mut().for_each(|shard| shard.fill(0x5a));
                    let (result, warm) = measure(|| rs.encode_sep(&data, &mut parity));
                    result.unwrap();
                    assert_eq!(parity, expected);
                    (cold, warm)
                })
                .join()
                .unwrap();
                let work_bytes = 2 * parity_count.next_power_of_two() * len.min(64 * 1024);
                let work_only_cacheable = work_bytes <= 4 * 1024 * 1024;
                // FFT schedules and wide view metadata remain in the count.
                // No unused chunk payload may hide in this small allowance.
                within_budget &= cold.1 <= work_bytes + 16 * 1024;
                within_budget &= if work_only_cacheable {
                    warm == (0, 0)
                } else {
                    warm.1 >= work_bytes && warm.1 <= work_bytes + 16 * 1024
                };
                println!(
                    "GF16_ENCODE_RESOURCE repetition={repetition} data={data_count} parity={parity_count} len={len} work_bytes={work_bytes} work_only_cacheable={work_only_cacheable} cold_calls={} cold_bytes={} warm_calls={} warm_bytes={}",
                    cold.0, cold.1, warm.0, warm.1
                );
            }
        }
        assert!(
            within_budget,
            "GF16 encode allocated unused scratch or retained oversized work"
        );
    }
}
