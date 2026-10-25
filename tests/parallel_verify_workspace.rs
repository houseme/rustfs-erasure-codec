#![cfg(feature = "std")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rustfs_erasure_codec::{
    CodecFamily, CodecOptions, Error, Field, MatrixMode, ReedSolomon, VerifyWorkspace,
};

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

fn encoded<F: Field>(codec: &ReedSolomon<F>, len: usize) -> Vec<Vec<F::Elem>> {
    let mut shards: Vec<_> = (0..codec.total_shard_count())
        .map(|index| {
            (0..len)
                .map(|offset| F::nth((index * 37 + offset * 19 + 1) % F::ORDER))
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}

fn check_classic_contracts<F: Field>()
where
    F::Elem: Send + Sync,
{
    let initial = ReedSolomon::<F>::new(2, 1).unwrap();
    let mut workspace = VerifyWorkspace::new(&initial, 1);
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ] {
        for fast in [false, true] {
            let parity = if fast { 1 } else { 3 };
            let options = CodecOptions::builder()
                .matrix_mode(mode)
                .fast_one_parity(fast)
                .max_parallel_jobs(2)
                .build();
            let codec = if mode == MatrixMode::Custom {
                let matrix = if fast {
                    vec![vec![F::one(); 4]]
                } else {
                    let reference = ReedSolomon::<F>::new(4, parity).unwrap();
                    (4..4 + parity)
                        .map(|row| {
                            reference
                                .encoding_format()
                                .matrix_row(row)
                                .unwrap()
                                .to_vec()
                        })
                        .collect()
                };
                ReedSolomon::<F>::with_custom_matrix(4, parity, &matrix, options).unwrap()
            } else {
                ReedSolomon::<F>::with_options(4, parity, options).unwrap()
            };
            for len in [1, 65, 4097] {
                let mut shards = encoded(&codec, len);
                assert!(
                    codec
                        .verify_with_workspace_par(&shards, &mut workspace)
                        .unwrap()
                );
                assert_eq!(workspace.parity_shards(), parity);
                assert_eq!(workspace.shard_len(), Some(len));
                for offset in [0, len - 1] {
                    shards[4][offset] = F::add(shards[4][offset], F::one());
                    assert!(
                        !codec
                            .verify_with_workspace_par(&shards, &mut workspace)
                            .unwrap()
                    );
                    assert!(!codec.verify_par(&shards).unwrap());
                    shards[4][offset] = F::add(shards[4][offset], F::one());
                }
            }
        }
    }
}

#[test]
fn generic_parallel_workspace_preserves_matrix_fast_and_corruption_contracts() {
    check_classic_contracts::<rustfs_erasure_codec::galois_8::Field>();
    check_classic_contracts::<rustfs_erasure_codec::galois_16::Field>();
}

#[test]
fn invalid_inputs_do_not_resize_or_overwrite_the_workspace() {
    let codec = rustfs_erasure_codec::galois_8::ReedSolomon::new(4, 2).unwrap();
    let shards = encoded(&codec, 65);
    let mut workspace = VerifyWorkspace::new(&codec, 65);
    assert!(
        codec
            .verify_with_workspace_par(&shards, &mut workspace)
            .unwrap()
    );
    let before = workspace.clone();
    for (invalid, error) in [
        (vec![], Error::TooFewShards),
        (vec![vec![1; 128]; 5], Error::TooFewShards),
        (vec![vec![1; 128]; 7], Error::TooManyShards),
        (vec![vec![]; 6], Error::EmptyShard),
    ] {
        assert_eq!(
            codec.verify_with_workspace_par(&invalid, &mut workspace),
            Err(error)
        );
        assert_eq!(workspace, before);
    }
    let mut invalid = vec![vec![1; 128]; 6];
    invalid[5].pop();
    assert_eq!(
        codec.verify_with_workspace_par(&invalid, &mut workspace),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(workspace, before);
}

#[test]
fn workspace_follows_the_existing_parallel_and_leopard_dispatch() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    pool.install(|| {
        fn parallel<F: Field>()
        where
            F::Elem: Send + Sync,
        {
            let codec = ReedSolomon::<F>::with_options(
                4,
                3,
                CodecOptions::builder().max_parallel_jobs(2).build(),
            )
            .unwrap();
            let mut shards = encoded(&codec, 512 * 1024 + 1);
            let decision = codec.parallel_policy(shards[0].len(), 3);
            assert!(decision.jobs <= 2);
            println!(
                "VERIFY_WORKSPACE_DISPATCH field_order={} parallel={} jobs={}",
                F::ORDER,
                decision.use_parallel,
                decision.jobs
            );
            let mut workspace = VerifyWorkspace::new(&codec, shards[0].len());
            assert!(
                codec
                    .verify_with_workspace_par(&shards, &mut workspace)
                    .unwrap()
            );
            shards[6][512 * 1024] = F::add(shards[6][512 * 1024], F::one());
            assert!(
                !codec
                    .verify_with_workspace_par(&shards, &mut workspace)
                    .unwrap()
            );
        }
        parallel::<rustfs_erasure_codec::galois_8::Field>();
        parallel::<rustfs_erasure_codec::galois_16::Field>();
    });
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec = rustfs_erasure_codec::galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let mut shards = encoded(&codec, 128);
        let mut workspace = VerifyWorkspace::new(&codec, 64);
        assert!(
            codec
                .verify_with_workspace_par(&shards, &mut workspace)
                .unwrap()
        );
        shards[5][127] ^= 1;
        assert!(
            !codec
                .verify_with_workspace_par(&shards, &mut workspace)
                .unwrap()
        );
        assert_eq!(
            codec.verify_with_workspace_opt(&shards, &mut workspace),
            codec.verify_par(&shards)
        );
    }
}

#[test]
fn repeated_workspace_verification_matches_caller_buffer_allocation_cost() {
    fn check<F: Field>()
    where
        F::Elem: Send + Sync,
    {
        for (data, parity, len) in [(4, 2, 32 * 1024), (2, 33, 65), (10, 4, 1024 * 1024)] {
            let codec = ReedSolomon::<F>::with_options(
                data,
                parity,
                CodecOptions::builder().max_parallel_jobs(1).build(),
            )
            .unwrap();
            let shards = encoded(&codec, len);
            let mut buffer = vec![vec![F::zero(); len]; parity];
            let mut workspace = VerifyWorkspace::new(&codec, len);
            assert!(codec.verify_par(&shards).unwrap());
            assert!(codec.verify_with_buffer_par(&shards, &mut buffer).unwrap());
            assert!(
                codec
                    .verify_with_workspace_par(&shards, &mut workspace)
                    .unwrap()
            );
            for repetition in 0..3 {
                let (valid, ordinary) = measure(|| codec.verify_par(&shards));
                assert!(valid.unwrap());
                let (valid, caller) =
                    measure(|| codec.verify_with_buffer_par(&shards, &mut buffer));
                assert!(valid.unwrap());
                let (valid, reused) =
                    measure(|| codec.verify_with_workspace_par(&shards, &mut workspace));
                assert!(valid.unwrap());
                assert_eq!(
                    reused, caller,
                    "workspace adds no per-call storage requests"
                );
                assert!(ordinary.0 > reused.0 && ordinary.1 > reused.1);
                if !cfg!(debug_assertions) {
                    assert_eq!(reused, (0, 0));
                }
                println!(
                    "VERIFY_WORKSPACE_RESOURCE field_order={} data={data} parity={parity} elements={len} repetition={repetition} ordinary_calls={} ordinary_bytes={} buffer_calls={} buffer_bytes={} workspace_calls={} workspace_bytes={}",
                    F::ORDER,
                    ordinary.0,
                    ordinary.1,
                    caller.0,
                    caller.1,
                    reused.0,
                    reused.1
                );
            }
        }
    }
    check::<rustfs_erasure_codec::galois_8::Field>();
    check::<rustfs_erasure_codec::galois_16::Field>();
}

#[test]
fn fast_single_parity_verification_reuses_the_classic_inline_scratch_budget() {
    fn check<F: Field>() -> bool
    where
        F::Elem: Send + Sync,
    {
        let mut budget_met = true;
        let codec = ReedSolomon::<F>::with_options(
            4,
            1,
            CodecOptions::builder()
                .fast_one_parity(true)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        for len in [1, 65, 4096, 4097] {
            let mut shards = encoded(&codec, len);
            assert!(codec.verify(&shards).unwrap());
            assert!(codec.verify_par(&shards).unwrap());
            for repetition in 0..3 {
                let (valid, serial) = measure(|| codec.verify(&shards));
                assert!(valid.unwrap());
                let (valid, parallel) = measure(|| codec.verify_par(&shards));
                assert!(valid.unwrap());
                println!(
                    "FAST_VERIFY_RESOURCE field_order={} elements={len} repetition={repetition} serial_calls={} serial_bytes={} parallel_calls={} parallel_bytes={}",
                    F::ORDER,
                    serial.0,
                    serial.1,
                    parallel.0,
                    parallel.1
                );
                budget_met &= serial == parallel && (len > 4096 || serial == (0, 0));
            }
            for offset in [0, len - 1] {
                shards[4][offset] = F::add(shards[4][offset], F::one());
                assert!(!codec.verify(&shards).unwrap());
                assert!(!codec.verify_par(&shards).unwrap());
                shards[4][offset] = F::add(shards[4][offset], F::one());
            }
        }
        budget_met
    }
    let gf8 = check::<rustfs_erasure_codec::galois_8::Field>();
    let gf16 = check::<rustfs_erasure_codec::galois_16::Field>();
    assert!(
        gf8 && gf16,
        "fast verification must share the ordinary inline scratch budget"
    );
}

#[test]
fn parity_corruption_cannot_hide_a_later_invalid_shard_or_mutate_scratch() {
    fn check<F: Field>()
    where
        F::Elem: Send + Sync,
    {
        for fast in [false, true] {
            let parity = if fast { 1 } else { 3 };
            let codec = ReedSolomon::<F>::with_options(
                4,
                parity,
                CodecOptions::builder()
                    .fast_one_parity(fast)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let mut shards = encoded(&codec, 65);
            shards[4][0] = F::add(shards[4][0], F::one());
            shards.last_mut().unwrap().pop();
            let mut buffer = vec![vec![F::nth(23); 65]; parity];
            let before = buffer.clone();
            let other = ReedSolomon::<F>::new(2, 1).unwrap();
            let mut workspace = VerifyWorkspace::new(&other, 1);
            for api in 0..6 {
                let (result, requests) = measure(|| match api {
                    0 => codec.verify(&shards),
                    1 => codec.verify_with_buffer(&shards, &mut buffer),
                    2 => codec.verify_with_workspace(&shards, &mut workspace),
                    3 => codec.verify_par(&shards),
                    4 => codec.verify_with_buffer_par(&shards, &mut buffer),
                    5 => codec.verify_with_workspace_par(&shards, &mut workspace),
                    _ => unreachable!(),
                });
                assert_eq!(result, Err(Error::IncorrectShardSize));
                assert_eq!(
                    requests,
                    (0, 0),
                    "invalid input must precede scratch preparation"
                );
                assert!(buffer == before);
                assert_eq!(workspace.parity_shards(), 1);
                assert_eq!(workspace.shard_len(), Some(1));
            }
        }
    }
    check::<rustfs_erasure_codec::galois_8::Field>();
    check::<rustfs_erasure_codec::galois_16::Field>();
}

#[test]
fn ordinary_verify_reserves_only_the_known_wide_view_count() {
    fn check<F: Field>() -> bool
    where
        F::Elem: Send + Sync,
    {
        let codec = ReedSolomon::<F>::with_options(
            2,
            33,
            CodecOptions::builder()
                .fast_one_parity(false)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let mut shards = encoded(&codec, 65);
        let mut buffer = vec![vec![F::zero(); 65]; 33];
        let view_bytes = 33 * size_of::<&mut [F::Elem]>();
        let mut bounded = true;
        for repetition in 0..3 {
            for (condition, position) in [
                ("valid", None),
                ("head", Some((2, 0))),
                ("tail", Some((34, 64))),
            ] {
                if let Some((row, offset)) = position {
                    shards[row][offset] = F::add(shards[row][offset], F::one());
                }
                let expected = position.is_none();
                let (serial, serial_count) = measure(|| codec.verify(&shards));
                let (parallel, parallel_count) = measure(|| codec.verify_par(&shards));
                let (direct, direct_count) =
                    measure(|| codec.verify_with_buffer(&shards, &mut buffer));
                let (direct_par, direct_par_count) =
                    measure(|| codec.verify_with_buffer_par(&shards, &mut buffer));
                for result in [serial, parallel, direct, direct_par] {
                    assert_eq!(result.unwrap(), expected);
                }
                bounded &= serial_count.0 <= direct_count.0 + 1
                    && serial_count.1 <= direct_count.1 + view_bytes;
                bounded &= parallel_count.0 <= direct_par_count.0 + 1
                    && parallel_count.1 <= direct_par_count.1 + view_bytes;
                println!(
                    "VERIFY_VIEW_RESOURCE field_order={} repetition={repetition} condition={condition} view_budget={view_bytes} serial_calls={} serial_bytes={} parallel_calls={} parallel_bytes={} buffer_calls={} buffer_bytes={} buffer_par_calls={} buffer_par_bytes={}",
                    F::ORDER,
                    serial_count.0,
                    serial_count.1,
                    parallel_count.0,
                    parallel_count.1,
                    direct_count.0,
                    direct_count.1,
                    direct_par_count.0,
                    direct_par_count.1
                );
                if let Some((row, offset)) = position {
                    shards[row][offset] = F::add(shards[row][offset], F::one());
                }
            }
        }
        bounded
    }
    let gf8 = check::<rustfs_erasure_codec::galois_8::Field>();
    let gf16 = check::<rustfs_erasure_codec::galois_16::Field>();
    assert!(
        gf8 && gf16,
        "ordinary verify over-reserved wide reference storage"
    );
}
