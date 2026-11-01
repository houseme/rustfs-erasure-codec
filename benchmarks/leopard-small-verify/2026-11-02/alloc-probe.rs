use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};

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
fn leopard_allocation_counts() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        for (data, parity) in [(4, 2), (10, 4), (16, 8), (20, 9)] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            for len in [64usize, 128, 4096] {
                let mut shards: Vec<Vec<u8>> = (0..data + parity)
                    .map(|i| (0..len).map(|j| (i * 31 + j * 17) as u8).collect())
                    .collect();
                codec.encode(&mut shards).unwrap();
                assert!(codec.verify(&shards).unwrap());
                let (result, (calls, bytes)) = measure(|| codec.verify(&shards));
                assert_eq!(result, Ok(true));
                println!("{family:?},{data},{parity},{len},{calls},{bytes}");
            }
        }
    }
}
