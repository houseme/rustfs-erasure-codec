#![cfg(feature = "std")]

use rustfs_erasure_codec::{CodecOptions, Field, MatrixMode, ReedSolomon, ShardByShard};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn record() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(current) = count.get() {
            count.set(Some(current + 1));
        }
    });
}

// SAFETY: every allocator request is forwarded unchanged to System, and the
// thread-local counter only records integers without allocating.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: System receives the caller's original valid layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: System receives the caller's original valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout describe the original System allocation.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        // SAFETY: System receives its own allocation and the requested size.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations(operation: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(Some(0)));
    operation();
    ALLOCATIONS.with(|count| count.replace(None).unwrap())
}

#[test]
fn encoding_and_buffered_verification_borrow_parity_rows_without_allocating() {
    fn check<F: Field>() -> bool
    where
        F::Elem: Send + Sync,
    {
        let mut budget_met = true;
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
        ] {
            for (data_count, parity_count) in [(4, 2), (10, 4), (4, 33), (10, 40), (33, 64)] {
                let codec = ReedSolomon::<F>::with_options(
                    data_count,
                    parity_count,
                    CodecOptions::builder().matrix_mode(mode).build(),
                )
                .unwrap();
                let data: Vec<Vec<F::Elem>> = (0..data_count)
                    .map(|index| {
                        (0..65)
                            .map(|offset| F::nth((index * 263 + offset * 17 + 1) % F::ORDER))
                            .collect()
                    })
                    .collect();
                let mut expected = vec![vec![F::zero(); 65]; parity_count];
                codec.encode_sep(&data, &mut expected).unwrap();
                let mut complete = data.clone();
                complete.extend(expected.clone());
                for entry in [
                    "encode-sep",
                    "encode",
                    "encode-sep-par",
                    "encode-par",
                    "single-cycle",
                    "single-par-cycle",
                    "verify-buffer",
                    "verify-buffer-par",
                    "shard-by-shard",
                ] {
                    let mut parity = vec![vec![F::zero(); 65]; parity_count];
                    let mut shards = complete.clone();
                    let execute =
                        |parity: &mut Vec<Vec<F::Elem>>, shards: &mut Vec<Vec<F::Elem>>| match entry
                        {
                            "encode-sep" => codec.encode_sep(&data, parity).unwrap(),
                            "encode" => codec.encode(shards).unwrap(),
                            "encode-sep-par" => codec.encode_sep_par(&data, parity).unwrap(),
                            "encode-par" => codec.encode_par(shards).unwrap(),
                            "single-cycle" => {
                                for (index, input) in data.iter().enumerate() {
                                    codec.encode_single_sep(index, input, parity).unwrap();
                                }
                            }
                            "single-par-cycle" => {
                                for (index, input) in data.iter().enumerate() {
                                    codec.encode_single_sep_par(index, input, parity).unwrap();
                                }
                            }
                            "verify-buffer" => {
                                assert!(codec.verify_with_buffer(&complete, parity).unwrap())
                            }
                            "verify-buffer-par" => {
                                assert!(codec.verify_with_buffer_par(&complete, parity).unwrap())
                            }
                            "shard-by-shard" => {
                                let mut encoder = ShardByShard::new(&codec);
                                for _ in 0..data_count {
                                    encoder.encode_sep(&data, parity).unwrap();
                                }
                                assert!(encoder.parity_ready());
                            }
                            _ => unreachable!(),
                        };
                    execute(&mut parity, &mut shards); // backend/policy setup outside tracked window
                    parity.iter_mut().for_each(|shard| shard.fill(F::zero()));
                    shards[data_count..]
                        .iter_mut()
                        .for_each(|shard| shard.fill(F::zero()));
                    let policy_decisions = match entry {
                        "encode-sep-par" | "encode-par" | "verify-buffer-par" => 1,
                        "single-par-cycle" => data_count,
                        _ => 0,
                    };
                    // Debug policy queries read a diagnostic environment key;
                    // Windows converts that key through an allocated UTF-16
                    // buffer. Measure the unchanged policy independently so it
                    // cannot be mistaken for a coding-row allocation. Release
                    // still requires absolute zero for both control and entry.
                    let policy_calls = allocations(|| {
                        for _ in 0..policy_decisions {
                            std::hint::black_box(codec.parallel_policy(65, parity_count));
                        }
                    });
                    let calls = allocations(|| execute(&mut parity, &mut shards));
                    let coding_calls = calls.checked_sub(policy_calls);
                    budget_met &=
                        (cfg!(debug_assertions) || policy_calls == 0) && coding_calls == Some(0);
                    println!(
                        "PARITY_ROW_ALLOCATION field_order={} matrix={mode:?} data={data_count} parity={parity_count} entry={entry} calls={calls} policy_decisions={policy_decisions} policy_calls={policy_calls} coding_calls={coding_calls:?}",
                        F::ORDER
                    );
                    if entry == "encode" || entry == "encode-par" {
                        assert_eq!(shards, complete);
                    } else {
                        assert_eq!(parity, expected);
                    }
                }
            }
        }
        budget_met
    }
    #[cfg(debug_assertions)]
    assert!(
        std::env::var_os("RS_PARALLEL_POLICY_DEBUG").is_none(),
        "allocation controls require policy logging to be disabled"
    );
    let gf8 = check::<rustfs_erasure_codec::galois_8::Field>();
    let gf16 = check::<rustfs_erasure_codec::galois_16::Field>();
    assert!(
        gf8 && gf16,
        "parity matrix row references must be borrowed without temporary allocation"
    );
    eprintln!("PARITY_ROW_BORROW_ALLOCATION cases=270 coding_calls_per_case=0");
}
