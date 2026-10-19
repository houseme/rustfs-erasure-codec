//! Request-volume counter for the optional resource probe; never a latency benchmark.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOC: AtomicUsize = AtomicUsize::new(0);
static REALLOC: AtomicUsize = AtomicUsize::new(0);
static DEALLOC: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Counter;
#[global_allocator]
static COUNTER: Counter = Counter;

fn record(counter: &AtomicUsize, bytes: usize) {
    if ENABLED.load(Ordering::SeqCst) {
        counter.fetch_add(1, Ordering::SeqCst);
        BYTES.fetch_add(bytes, Ordering::SeqCst);
    }
}
// SAFETY: all allocation requests are forwarded unchanged to System. The scalar
// atomic counters do not allocate and include synchronous Rayon work, if any.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(&ALLOC, layout.size());
        // SAFETY: preserve the valid caller layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(&ALLOC, layout.size());
        // SAFETY: preserve the valid caller layout and zeroing contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(&REALLOC, new_size);
        // SAFETY: forward the original allocation and requested new size.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(&DEALLOC, 0);
        // SAFETY: forward the original System allocation unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub alloc: usize,
    pub realloc: usize,
    pub dealloc: usize,
    pub requested_bytes: usize,
}

pub fn measure<T>(operation: impl FnOnce() -> T) -> (T, Counts) {
    assert!(
        !ENABLED.load(Ordering::SeqCst),
        "nested resource measurement"
    );
    for counter in [&ALLOC, &REALLOC, &DEALLOC, &BYTES] {
        counter.store(0, Ordering::SeqCst);
    }
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            ENABLED.store(false, Ordering::SeqCst);
        }
    }
    ENABLED.store(true, Ordering::SeqCst);
    let stop = Stop;
    let result = std::hint::black_box(operation());
    drop(stop);
    (
        result,
        Counts {
            alloc: ALLOC.load(Ordering::SeqCst),
            realloc: REALLOC.load(Ordering::SeqCst),
            dealloc: DEALLOC.load(Ordering::SeqCst),
            requested_bytes: BYTES.load(Ordering::SeqCst),
        },
    )
}

impl std::fmt::Display for Counts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"alloc\":{},\"realloc\":{},\"dealloc\":{},\"requested_bytes\":{}}}",
            self.alloc, self.realloc, self.dealloc, self.requested_bytes
        )
    }
}
