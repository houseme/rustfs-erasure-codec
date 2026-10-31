use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Counting;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
// SAFETY: All allocations delegate to System with unchanged arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, layout, size) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;
#[test]
fn gf16_cold_workspace_allocations() {
    for size in [8192usize, 65536, 1048576] {
        let codec = Arc::new(
            ReedSolomon::with_options(
                10,
                4,
                CodecOptions::builder()
                    .codec_family(CodecFamily::LeopardGF16)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap(),
        );
        let mut source: Vec<Vec<u8>> = (0..14)
            .map(|i| (0..size).map(|j| (i * 31 + j * 17) as u8).collect())
            .collect();
        codec.encode(&mut source).unwrap();
        let mut warm: Vec<_> = source.iter().cloned().map(Some).collect();
        for shard in &mut warm[..4] {
            *shard = None;
        }
        codec.reconstruct(&mut warm).unwrap();
        for mode in ["option", "slot"] {
            let codec = Arc::clone(&codec);
            let source = source.clone();
            let (calls, bytes) = std::thread::spawn(move || {
                if mode == "option" {
                    let mut shards: Vec<_> = source.iter().cloned().map(Some).collect();
                    for shard in &mut shards[..4] {
                        *shard = None;
                    }
                    CALLS.store(0, Ordering::Relaxed);
                    BYTES.store(0, Ordering::Relaxed);
                    ACTIVE.store(true, Ordering::SeqCst);
                    let result = codec.reconstruct(&mut shards);
                    ACTIVE.store(false, Ordering::SeqCst);
                    let counts = (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
                    result.unwrap();
                    assert!(
                        shards
                            .iter()
                            .zip(&source)
                            .all(|(a, b)| a.as_ref() == Some(b))
                    );
                    counts
                } else {
                    let mut shards: Vec<_> =
                        source.iter().cloned().map(ShardSlot::new_present).collect();
                    for shard in &mut shards[..4] {
                        shard.mark_missing();
                    }
                    CALLS.store(0, Ordering::Relaxed);
                    BYTES.store(0, Ordering::Relaxed);
                    ACTIVE.store(true, Ordering::SeqCst);
                    let result = codec.reconstruct(&mut shards);
                    ACTIVE.store(false, Ordering::SeqCst);
                    let counts = (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
                    result.unwrap();
                    assert!(
                        shards
                            .iter()
                            .zip(&source)
                            .all(|(a, b)| a.is_present() && a.as_inner() == b)
                    );
                    counts
                }
            })
            .join()
            .unwrap();
            println!("GF16_COLD_ALLOC mode={mode} size={size} calls={calls} bytes={bytes}");
        }
    }
}
