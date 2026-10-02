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
mod allocations {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    // Measure only the caller thread; max_parallel_jobs=1 keeps decode here.
    thread_local! {
        static MEASURE: Cell<Option<(usize, usize, usize)>> = const { Cell::new(None) };
    }

    fn record(bytes: usize) {
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
    // System. Thread-local bookkeeping does not allocate or access user memory.
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
}
