#![cfg(all(feature = "std", target_arch = "x86_64"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};

thread_local! {
    static REQUESTS: Cell<Option<usize>> = const { Cell::new(None) };
}

struct CountingAllocator;

fn record() {
    let _ = REQUESTS.try_with(|state| {
        if let Some(calls) = state.get() {
            state.set(Some(calls + 1));
        }
    });
}

// SAFETY: all allocation operations are forwarded unchanged to System;
// bookkeeping is thread-local and never accesses allocation memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: layout is forwarded from the caller unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: layout is forwarded from the caller unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        // SAFETY: pointer, layout and size are forwarded from the caller unchanged.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout are forwarded from the caller unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measure<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            REQUESTS.with(|state| state.set(None));
        }
    }
    REQUESTS.with(|state| state.set(Some(0)));
    let reset = Reset;
    let result = operation();
    let calls = REQUESTS.with(|state| state.get().unwrap());
    drop(reset);
    (result, calls)
}

#[test]
fn warmed_packed_staging_avoids_output_allocations_at_capacity_boundaries() {
    let selected = rustfs_erasure_codec::galois_8::active_backend_name();
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(selected, expected);
    }
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        for (data, parity, len) in [
            (16usize, 8usize, 128usize),
            (16, 8, 256),
            (10, 4, 512),
            (4, 2, 1024),
        ] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut source).unwrap();
            let mut slots: Vec<_> = source.iter().cloned().map(ShardSlot::new_present).collect();
            let missing = if parity >= 4 {
                vec![0, 1, data, data + 1]
            } else {
                vec![0, data]
            };
            for &i in &missing {
                slots[i].mark_missing();
            }
            codec.reconstruct(&mut slots).unwrap();
            for &i in &missing {
                slots[i].mark_missing();
                slots[i].as_inner_mut().fill(0xa5);
            }
            let (result, allocations) = measure(|| codec.reconstruct(&mut slots));
            assert_eq!(result, Ok(()));
            assert_eq!(allocations, 0, "{family:?} {data}+{parity} {len}");
            assert!(
                slots
                    .iter()
                    .zip(&source)
                    .all(|(slot, expected)| slot.is_present() && slot.as_inner() == expected)
            );
        }
    }
}
