use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};

fn codec(data: usize, parity: usize) -> ReedSolomon {
    ReedSolomon::with_options(
        data,
        parity,
        CodecOptions::builder()
            .codec_family(CodecFamily::LeopardGF8)
            .build(),
    )
    .unwrap()
}

fn encoded(rs: &ReedSolomon, len: usize, seed: usize) -> Vec<Vec<u8>> {
    let mut shards: Vec<Vec<u8>> = (0..rs.total_shard_count())
        .map(|shard| {
            (0..len)
                .map(|offset| {
                    if seed == 0 {
                        0
                    } else {
                        (seed + shard * 29 + offset * 17 + offset / 64 * 13) as u8
                    }
                })
                .collect()
        })
        .collect();
    rs.encode(&mut shards).unwrap();
    shards
}

fn missing_slots(original: &[Vec<u8>], missing: &[usize]) -> Vec<ShardSlot<Vec<u8>>> {
    original
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            if missing.contains(&index) {
                ShardSlot::new_missing(vec![0xa5; bytes.len()])
            } else {
                ShardSlot::new_present(bytes.clone())
            }
        })
        .collect()
}

fn check_selective(rs: &ReedSolomon, original: &[Vec<u8>], missing: &[usize], target: usize) {
    let mut slots = missing_slots(original, missing);
    let mut required = vec![false; slots.len()];
    required[target] = true;
    rs.reconstruct_some(&mut slots, &required).unwrap();
    for (index, slot) in slots.iter().enumerate() {
        if missing.contains(&index) && index != target {
            assert!(!slot.is_present(), "unexpected publication at {index}");
            assert!(slot.as_inner().iter().all(|byte| *byte == 0xa5));
        } else {
            assert!(slot.is_present());
            assert_eq!(slot.as_inner(), &original[index], "shard {index}");
        }
    }
}

#[test]
fn decode_reuse_preserves_erasure_patterns_across_shapes_sizes_and_requested_outputs() {
    // Some adjacent shapes share rounded parity/transform sizes; others share
    // an erasure count but have a different missing-data or parity location.
    for (data, parity, len) in [
        (3, 2, 64),
        (10, 4, 32 * 1024 + 64),
        (11, 4, 64 * 1024 + 64),
        (11, 3, 192),
        (3, 7, 64 * 1024 + 64),
        (252, 4, 64),
        (10, 4, 64),
        (1, 1, 192),
        (17, 1, 64),
        (3, 2, 64 * 1024 + 64),
    ] {
        let rs = codec(data, parity);
        for seed in [29, 0, 71] {
            let original = encoded(&rs, len, seed);
            let mut mixed = vec![0, data - 1, data + parity - 1];
            mixed.sort_unstable();
            mixed.dedup();
            mixed.truncate(parity);
            for missing in [vec![0], vec![data + parity - 1], mixed, vec![0]] {
                // Change only requested outputs while keeping the complete
                // erasure pattern fixed, then revisit previous erasure patterns.
                for &target in &missing {
                    check_selective(&rs, &original, &missing, target);
                }
                let mut slots = missing_slots(&original, &missing);
                rs.reconstruct(&mut slots).unwrap();
                for (slot, expected) in slots.iter().zip(&original) {
                    assert_eq!(slot.as_inner(), expected);
                }
            }
        }
    }
}

#[test]
fn concurrent_decoders_keep_owned_thread_local_state() {
    let threads: Vec<_> = (0..4)
        .map(|thread| {
            std::thread::spawn(move || {
                let rs = codec(10 + thread, 4);
                for round in 0..8 {
                    let len = if round % 2 == 0 { 64 * 1024 + 64 } else { 128 };
                    let seed = if round % 3 == 0 {
                        0
                    } else {
                        13 + thread + round
                    };
                    let original = encoded(&rs, len, seed);
                    let missing = [round % rs.data_shard_count(), rs.total_shard_count() - 1];
                    for target in missing {
                        check_selective(&rs, &original, &missing, target);
                    }
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[cfg(feature = "std")]
mod allocations {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    struct CountingAllocator;
    thread_local! {
        static ALLOCATIONS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
    }
    fn record(size: usize) {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some((calls, bytes)) = count.get() {
                count.set(Some((calls + 1, bytes + size)));
            }
        });
    }
    // SAFETY: all allocation operations are delegated unchanged to System;
    // the thread-local counters only record scalar values without allocation.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record(layout.size());
            // SAFETY: the allocator receives the caller's unchanged valid layout.
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record(layout.size());
            // SAFETY: the allocator receives the caller's unchanged valid layout.
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: ptr and layout identify the unchanged System allocation.
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record(new_size);
            // SAFETY: System receives its own allocation and the requested size.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    #[test]
    fn warm_selective_decode_only_allocates_requested_output_storage() {
        std::thread::spawn(|| {
            let rs = codec(10, 4);
            for len in [32 * 1024, 64 * 1024 + 64] {
                let original = encoded(&rs, len, 31);
                let missing = [0, 1, 13];
                check_selective(&rs, &original, &missing, 0);
                for target in [1, 13, 0] {
                    let mut slots = missing_slots(&original, &missing);
                    let mut required = [false; 14];
                    required[target] = true;
                    ALLOCATIONS.with(|count| count.set(Some((0, 0))));
                    let result = rs.reconstruct_some(&mut slots, &required);
                    let (calls, bytes) = ALLOCATIONS.with(|count| count.replace(None).unwrap());
                    result.unwrap();
                    assert_eq!(slots[target].as_inner(), &original[target]);
                    // Core recovery deliberately retains a temporary requested
                    // output until decoding succeeds, plus Vec metadata.
                    assert!(
                        calls <= 2,
                        "{len} bytes, target {target}: {calls} allocations"
                    );
                    assert!(
                        bytes <= len + 1024,
                        "{len} bytes, target {target}: {bytes} allocated"
                    );
                }
            }
        })
        .join()
        .unwrap();
    }
}
