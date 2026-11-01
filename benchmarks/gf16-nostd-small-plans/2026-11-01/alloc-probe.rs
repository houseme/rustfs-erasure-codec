use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Tracking;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) { CALLS.fetch_add(1, Ordering::Relaxed); BYTES.fetch_add(layout.size(), Ordering::Relaxed); }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) { CALLS.fetch_add(1, Ordering::Relaxed); BYTES.fetch_add(layout.size(), Ordering::Relaxed); }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) { CALLS.fetch_add(1, Ordering::Relaxed); BYTES.fetch_add(size, Ordering::Relaxed); }
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}
#[global_allocator] static GLOBAL: Tracking = Tracking;
fn main() {
    for (data, parity) in [(10usize, 4usize), (16, 8), (32, 16), (128, 4), (1024, 4)] {
        let codec = ReedSolomon::with_options(data, parity, CodecOptions::builder().codec_family(CodecFamily::LeopardGF16).max_parallel_jobs(1).build()).unwrap();
        for size in [64usize, 4096, 1048576] {
            if data > 32 && size == 1048576 { continue; }
            let mut shards: Vec<Vec<u8>> = (0..data+parity).map(|i| (0..size).map(|j| (i*31+j*17+j/11) as u8).collect()).collect();
            codec.encode(&mut shards).unwrap(); assert!(codec.verify(&shards).unwrap());
            CALLS.store(0, Ordering::SeqCst); BYTES.store(0, Ordering::SeqCst); ACTIVE.store(true, Ordering::SeqCst);
            let result=codec.encode(&mut shards);
            ACTIVE.store(false, Ordering::SeqCst); let calls=CALLS.load(Ordering::SeqCst);let bytes=BYTES.load(Ordering::SeqCst);
            result.unwrap(); assert!(codec.verify(&shards).unwrap());
            println!("{data}+{parity} size={size} calls={calls} bytes={bytes}");
        }
    }
}
