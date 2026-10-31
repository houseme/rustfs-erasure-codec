//! Diagnostic request counts; no elapsed-time measurements are made.
use rustfs_erasure_codec::{CodecOptions, diagnostic_row, galois_8::ReedSolomon};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Counting;
static ENABLED: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every operation delegates to System with its original arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: the caller supplies the System allocation contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the allocation is returned to the same underlying allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size, Ordering::Relaxed);
        }
        // SAFETY: the caller supplies the original allocation and layout.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn main() {
    assert_eq!(
        rustfs_erasure_codec::galois_8::active_backend_name(),
        "rust-avx2"
    );
    assert_eq!(rayon::current_num_threads(), 4);
    let available = std::thread::available_parallelism().unwrap().get();
    assert_eq!(available, 5);
    for (data_count, parity_count, len, missing) in [
        (10, 4, 1_048_576, 0),
        (10, 4, 1_048_577, 3),
        (4, 2, 1_048_576, 0),
    ] {
        let codec = ReedSolomon::with_options(
            data_count,
            parity_count,
            CodecOptions::builder().max_parallel_jobs(4).build(),
        )
        .unwrap();
        let mut original: Vec<Vec<u8>> = (0..data_count + parity_count)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut original).unwrap();
        let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
        shards[missing] = None;
        let (indices, row) = diagnostic_row::recovery_row(&codec, &mut shards).unwrap();
        let inputs: Vec<&[u8]> = indices.iter().map(|&i| original[i].as_slice()).collect();
        let decision = codec
            .reconstruct_stage_policies_for_bench(true)
            .0
            .decide(len, data_count, 1, available);
        assert!(decision.use_parallel && decision.jobs == 4);
        let mut output = vec![0xa5; len];
        for _ in 0..20 {
            diagnostic_row::run_row(
                &codec,
                &row,
                &inputs,
                &mut output,
                decision.chunk_len,
                decision.jobs,
            );
        }
        assert_eq!(output, original[missing]);
        let address = output.as_ptr();
        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        ENABLED.store(true, Ordering::Relaxed);
        for _ in 0..100 {
            diagnostic_row::run_row(
                &codec,
                &row,
                &inputs,
                &mut output,
                decision.chunk_len,
                decision.jobs,
            );
        }
        ENABLED.store(false, Ordering::Relaxed);
        assert_eq!(output.as_ptr(), address);
        assert_eq!(output, original[missing]);
        println!(
            "{{\"case_id\":\"retained_recovery_row/{data_count}x{parity_count}/{len}/missing{missing}\",\"iterations\":100,\"allocation_calls\":{},\"requested_bytes\":{},\"chunk_len\":{},\"jobs\":{}}}",
            CALLS.load(Ordering::Relaxed),
            BYTES.load(Ordering::Relaxed),
            decision.chunk_len,
            decision.jobs
        );
    }
}
