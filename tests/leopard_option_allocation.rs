#![cfg(feature = "std")]
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Counter;
static BYTES: AtomicUsize = AtomicUsize::new(0);
// SAFETY: Every operation delegates its original pointers and layouts to System.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: The caller supplies the allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: The caller supplies the allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies a live System allocation and its layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        BYTES.fetch_add(size, Ordering::Relaxed);
        // SAFETY: The caller supplies a live allocation, its layout, and new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;

#[test]
fn direct_option_recovery_avoids_temporary_output_allocations() {
    #[cfg(target_endian = "little")]
    let families = &[CodecFamily::LeopardGF8, CodecFamily::LeopardGF16][..];
    #[cfg(target_endian = "big")]
    let families = &[CodecFamily::LeopardGF8][..];
    for &family in families {
        for (data, parity, len, missing_data, missing_parity) in [
            (4, 3, 65_536, 1, 1),
            (10, 4, 1_048_576, 4, 0),
            (16, 8, 65_536, 1, 1),
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
            let mut original = vec![vec![42; len]; data + parity];
            codec.encode(&mut original).unwrap();
            let input = || {
                let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
                for shard in shards.iter_mut().take(missing_data) {
                    *shard = None;
                }
                for shard in shards.iter_mut().skip(data).take(missing_parity) {
                    *shard = None;
                }
                shards
            };
            codec.reconstruct(&mut input()).unwrap();
            codec.reconstruct_opt(&mut input()).unwrap();
            let mut old = input();
            BYTES.store(0, Ordering::Relaxed);
            codec.reconstruct(&mut old).unwrap();
            let old_bytes = BYTES.load(Ordering::Relaxed);
            let mut new = input();
            BYTES.store(0, Ordering::Relaxed);
            codec.reconstruct_opt(&mut new).unwrap();
            let new_bytes = BYTES.load(Ordering::Relaxed);
            assert!(
                old.iter()
                    .zip(&original)
                    .all(|(actual, expected)| actual.as_ref() == Some(expected))
            );
            assert_eq!(old, new);
            assert!(
                old_bytes >= new_bytes + (missing_data + missing_parity) * len,
                "{family:?} {data}+{parity} len={len} old={old_bytes} new={new_bytes}"
            );
            println!(
                "{family:?} {data}+{parity} len={len} missing={missing_data}+{missing_parity} generic={old_bytes} optimized={new_bytes} reduced={}",
                old_bytes - new_bytes
            );
        }
    }
}
