use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};

thread_local! {
    static REQUESTS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

struct CountingAllocator;

fn record(bytes: usize) {
    let _ = REQUESTS.try_with(|state| {
        if let Some((calls, total)) = state.get() {
            state.set(Some((calls + 1, total + bytes)));
        }
    });
}

// SAFETY: requests are forwarded unchanged to System. Integer bookkeeping is
// thread-local, does not allocate, and never reads or writes allocation memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: the caller supplies a valid GlobalAlloc layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: System preserves the caller's zero-initialization request.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: the caller supplies a live System allocation and valid size.
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer/layout describe the original System allocation.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measure<T>(operation: impl FnOnce() -> T) -> (T, (usize, usize)) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            REQUESTS.with(|state| state.set(None));
        }
    }
    REQUESTS.with(|state| state.set(Some((0, 0))));
    let reset = Reset;
    let result = operation();
    let requests = REQUESTS.with(|state| state.get().unwrap());
    drop(reset);
    (result, requests)
}

#[test]
fn observe_small_recovery_allocations() {
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(rustfs_erasure_codec::galois_8::active_backend_name(), expected);
    }
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        for (data, parity, len) in [
            (4usize, 2usize, 64usize),
            (10, 4, 64),
            (10, 4, 128),
            (16, 8, 64),
            (16, 8, 128),
            (16, 8, 256),
            (16, 8, 320),
            (10, 4, 512),
            (4, 2, 1024),
            (24, 9, 64),
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
            let mut original: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut original).unwrap();
            for missing in [1usize, 4] {
                if missing > parity {
                    continue;
                }
                let mut slots: Vec<_> = original
                    .iter()
                    .cloned()
                    .map(ShardSlot::new_present)
                    .collect();
                for slot in &mut slots[..missing] {
                    slot.mark_missing();
                }
                codec.reconstruct(&mut slots).unwrap();
                for slot in &mut slots[..missing] {
                    slot.mark_missing();
                }
                let (result, (calls, bytes)) = measure(|| codec.reconstruct(&mut slots));
                assert_eq!(result, Ok(()));
                assert!(
                    slots
                        .iter()
                        .zip(&original)
                        .all(|(s, x)| s.is_present() && s.as_inner() == x)
                );
                println!(
                    "SMALL_RECOVERY {family:?} {data} {parity} {len} {missing} {calls} {bytes}"
                );
            }
        }
    }
}
