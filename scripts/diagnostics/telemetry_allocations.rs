//! Test-only allocator probe. Never linked into the formal sampler.
#![allow(dead_code)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    alloc: usize,
    realloc: usize,
    dealloc: usize,
}
thread_local! { static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) }; }
struct CountingAllocator;
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn record(alloc: usize, realloc: usize, dealloc: usize) {
    let _ = COUNTS.try_with(|state| {
        if let Some(mut counts) = state.get() {
            counts.alloc += alloc;
            counts.realloc += realloc;
            counts.dealloc += dealloc;
            state.set(Some(counts));
        }
    });
}
// SAFETY: every request is delegated unchanged to System. Thread-local scalar
// counters neither allocate nor modify ownership; unrelated threads are excluded.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(1, 0, 0);
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(1, 0, 0);
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(0, 1, 0);
        // SAFETY: pointer/layout identify the original System allocation.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(0, 0, 1);
        // SAFETY: pointer/layout identify the original System allocation.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn normal_telemetry_does_not_touch_the_rust_heap() {
    assert!(std::env::var_os("RSE_SAMPLER_DIAGNOSTIC_PROCESS").is_none());
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTS.with(|state| state.set(None));
        }
    }
    COUNTS.with(|state| state.set(Some(Counts::default())));
    let reset = Reset;
    let observation = (0..16).map(|_| sampling::telemetry()).last().unwrap();
    let counts = COUNTS.with(|state| state.replace(None).unwrap());
    drop(reset);
    assert_ne!(
        observation.to_string(),
        "null",
        "real Linux telemetry must be available"
    );
    assert_eq!(counts, Counts::default());
}
