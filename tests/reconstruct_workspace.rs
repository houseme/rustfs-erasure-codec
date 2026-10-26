use rustfs_erasure_codec::{
    CodecFamily, CodecOptions, Error, Field, MatrixMode, ReconstructWorkspace, ReedSolomon,
    ShardSlot, galois_8, galois_16,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
struct Requests {
    calls: usize,
    bytes: usize,
}
thread_local! {
    static REQUESTS: Cell<Option<Requests>> = const { Cell::new(None) };
}
fn record(bytes: usize) {
    let _ = REQUESTS.try_with(|state| {
        if let Some(mut requests) = state.get() {
            requests.calls += 1;
            requests.bytes += bytes;
            state.set(Some(requests));
        }
    });
}
struct Counter;
// SAFETY: forward every original pointer/layout contract to System. Scalar
// caller-thread bookkeeping allocates nothing and never accesses payloads.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: preserve the caller's valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: preserve the valid layout and zero-initialization contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: forward the live System allocation and requested size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forward the original allocation and matching layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn counted<T>(run: impl FnOnce() -> T) -> (T, Requests) {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            REQUESTS.with(|state| state.set(None));
        }
    }
    REQUESTS.with(|state| assert!(state.replace(Some(Requests::default())).is_none()));
    let _stop = Stop;
    let result = run();
    let requests = REQUESTS.with(|state| state.take().unwrap());
    (result, requests)
}

fn codec<F: Field>(
    data: usize,
    parity: usize,
    mode: MatrixMode,
    fast: bool,
    cache: bool,
) -> ReedSolomon<F> {
    let options = CodecOptions::builder()
        .matrix_mode(mode)
        .fast_one_parity(fast)
        .inversion_cache(cache)
        .build();
    if mode == MatrixMode::Custom {
        let reference = ReedSolomon::<F>::with_options(
            data,
            parity,
            CodecOptions::builder().fast_one_parity(fast).build(),
        )
        .unwrap();
        let rows: Vec<_> = (data..data + parity)
            .map(|row| {
                reference
                    .encoding_format()
                    .matrix_row(row)
                    .unwrap()
                    .to_vec()
            })
            .collect();
        ReedSolomon::with_custom_matrix(data, parity, &rows, options).unwrap()
    } else {
        ReedSolomon::with_options(data, parity, options).unwrap()
    }
}
fn encoded<F: Field>(codec: &ReedSolomon<F>, len: usize, seed: usize) -> Vec<Vec<F::Elem>> {
    let mut shards: Vec<Vec<_>> = (0..codec.total_shard_count())
        .map(|row| {
            (0..len)
                .map(|offset| F::nth((row * 379 + offset * 191 + seed) % F::ORDER))
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}
fn slots<F: Field>(expected: &[Vec<F::Elem>], missing: &[usize]) -> Vec<ShardSlot<Vec<F::Elem>>> {
    let mut slots: Vec<_> = expected
        .iter()
        .cloned()
        .map(ShardSlot::new_present)
        .collect();
    erase::<F>(&mut slots, missing);
    slots
}
fn erase<F: Field>(slots: &mut [ShardSlot<Vec<F::Elem>>], missing: &[usize]) {
    for &index in missing {
        slots[index].mark_missing();
        slots[index].as_inner_mut().fill(F::one());
    }
}
fn check<F: Field>(slots: &[ShardSlot<Vec<F::Elem>>], expected: &[Vec<F::Elem>]) {
    for (slot, bytes) in slots.iter().zip(expected) {
        assert!(slot.is_present());
        assert_eq!(slot.as_inner(), bytes);
    }
}

#[test]
fn reusable_plans_preserve_all_classic_matrices_fields_and_missing_shapes() {
    fn exercise<F: Field>() {
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ] {
            for fast in [false, true] {
                for cache in [false, true] {
                    let parity = if fast { 1 } else { 3 };
                    let codec = codec::<F>(4, parity, mode, fast, cache);
                    for len in [1, 65, 4097] {
                        let expected = encoded(&codec, len, 31);
                        for missing in [&[][..], &[0], &[4], &[0, 1], &[0, 4], &[0, 1, 4]] {
                            if missing.len() > parity {
                                continue;
                            }
                            let mut shards = slots::<F>(&expected, missing);
                            let pointers: Vec<_> =
                                shards.iter().map(|s| s.as_inner().as_ptr()).collect();
                            let workspace: ReconstructWorkspace<F> =
                                codec.prepare_reconstruct_workspace(&shards).unwrap();
                            assert!(workspace.encoding_format() == codec.encoding_format());
                            for _ in 0..3 {
                                erase::<F>(&mut shards, missing);
                                codec
                                    .reconstruct_with_workspace(&mut shards, &workspace)
                                    .unwrap();
                                check::<F>(&shards, &expected);
                                assert!(
                                    shards
                                        .iter()
                                        .zip(&pointers)
                                        .all(|(s, p)| s.as_inner().as_ptr() == *p)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[test]
fn invalid_layouts_counts_lengths_and_formats_never_publish_outputs() {
    fn exercise<F: Field>() {
        let source = codec::<F>(4, 3, MatrixMode::Vandermonde, false, true);
        let expected = encoded(&source, 65, 37);
        let missing = slots::<F>(&expected, &[0]);
        let workspace = source.prepare_reconstruct_workspace(&missing).unwrap();
        for (data, parity, mode, fast, error) in [
            (
                5,
                2,
                MatrixMode::Vandermonde,
                false,
                Error::TooFewDataShards,
            ),
            (
                3,
                4,
                MatrixMode::Vandermonde,
                false,
                Error::TooManyDataShards,
            ),
            (
                4,
                2,
                MatrixMode::Vandermonde,
                false,
                Error::TooManyParityShards,
            ),
            (
                4,
                4,
                MatrixMode::Vandermonde,
                false,
                Error::TooFewParityShards,
            ),
            (4, 3, MatrixMode::Cauchy, false, Error::InvalidCustomMatrix),
        ] {
            let target = codec::<F>(data, parity, mode, fast, true);
            let bytes = encoded(&target, 65, 41);
            let mut candidate = slots::<F>(&bytes, &[0]);
            let before = candidate.clone();
            assert_eq!(
                target.reconstruct_with_workspace(&mut candidate, &workspace),
                Err(error)
            );
            assert_eq!(candidate, before);
        }
        for (count, error) in [
            (0, Error::TooFewShards),
            (6, Error::TooFewShards),
            (8, Error::TooManyShards),
        ] {
            let mut candidate = vec![ShardSlot::new_missing(vec![F::one(); 65]); count];
            let before = candidate.clone();
            assert!(
                matches!(source.prepare_reconstruct_workspace(&candidate), Err(e) if e == error)
            );
            assert_eq!(
                source.reconstruct_with_workspace(&mut candidate, &workspace),
                Err(error)
            );
            assert_eq!(candidate, before);
        }
        for (pattern, length, error) in [
            (&[1][..], 65, Error::InvalidShardFlags),
            (&[0][..], 0, Error::EmptyShard),
            (&[0][..], 64, Error::IncorrectShardSize),
            (&[1][..], 64, Error::IncorrectShardSize),
        ] {
            let mut candidate = slots::<F>(&expected, pattern);
            candidate[6].as_inner_mut().truncate(length);
            let before = candidate.clone();
            let (result, allocations) =
                counted(|| source.reconstruct_with_workspace(&mut candidate, &workspace));
            assert_eq!(result, Err(error));
            assert_eq!(allocations.calls, 0);
            assert_eq!(candidate, before);
        }
        let mut uniform = slots::<F>(&expected, &[0]);
        for shard in &mut uniform {
            shard.as_inner_mut().truncate(64);
        }
        let before = uniform.clone();
        assert_eq!(
            source.reconstruct_with_workspace(&mut uniform, &workspace),
            Err(Error::IncorrectShardSize)
        );
        assert_eq!(uniform, before);
        let too_few = slots::<F>(&expected, &[0, 1, 2, 3]);
        assert!(matches!(
            source.prepare_reconstruct_workspace(&too_few),
            Err(Error::TooFewShardsPresent)
        ));
        let regular = codec::<F>(4, 1, MatrixMode::Vandermonde, false, true);
        let fast = codec::<F>(4, 1, MatrixMode::Vandermonde, true, true);
        let reference = encoded(&regular, 65, 17);
        let plan = regular
            .prepare_reconstruct_workspace(&slots::<F>(&reference, &[0]))
            .unwrap();
        let mut candidate = slots::<F>(&encoded(&fast, 65, 17), &[0]);
        let before = candidate.clone();
        assert_eq!(
            fast.reconstruct_with_workspace(&mut candidate, &plan),
            Err(Error::InvalidCustomMatrix)
        );
        assert_eq!(candidate, before);
        // An all-present prepared plan still validates every later shard.
        let mut candidate = slots::<F>(&expected, &[]);
        let present = source.prepare_reconstruct_workspace(&candidate).unwrap();
        candidate[6].as_inner_mut().pop();
        let before = candidate.clone();
        assert_eq!(
            source.reconstruct_with_workspace(&mut candidate, &present),
            Err(Error::IncorrectShardSize)
        );
        assert_eq!(candidate, before);
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[test]
fn singular_preparation_and_short_destination_keep_existing_publication_order() {
    let singular = ReedSolomon::<galois_8::Field>::with_custom_matrix(
        2,
        2,
        &[vec![1, 1], vec![1, 1]],
        CodecOptions::default(),
    )
    .unwrap();
    let expected = encoded(&singular, 65, 19);
    let shards = slots::<galois_8::Field>(&expected, &[0, 1]);
    let before = shards.clone();
    assert!(matches!(
        singular.prepare_reconstruct_workspace(&shards),
        Err(Error::InvalidCustomMatrix)
    ));
    assert_eq!(shards, before);

    let codec = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, false);
    let expected = encoded(&codec, 65, 23);
    let mut reused = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
    reused[1].as_inner_mut().pop();
    let mut ordinary = reused.clone();
    let workspace = codec.prepare_reconstruct_workspace(&reused).unwrap();
    assert_eq!(
        codec.reconstruct(&mut ordinary),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(
        codec.reconstruct_with_workspace(&mut reused, &workspace),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(ordinary, reused);
    assert!(reused[0].is_present());
    assert_eq!(reused[0].as_inner(), &expected[0]);
    assert!(!reused[1].is_present() && !reused[4].is_present());
}

#[test]
fn plans_own_their_format_and_inverse_after_the_source_codec_is_dropped() {
    for goal in ["all", "data", "mixed"] {
        let required = [true, false, false, false, true, false, false];
        let (workspace, expected) = {
            let source = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, false);
            let expected = encoded(&source, 65, 1234);
            let missing = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
            let plan = match goal {
                "all" => source.prepare_reconstruct_workspace(&missing),
                "data" => source.prepare_reconstruct_data_workspace(&missing),
                _ => source.prepare_reconstruct_some_workspace(&missing, &required),
            }
            .unwrap();
            (plan, expected)
        };
        let equivalent = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, true);
        let mut missing = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
        let mut ordinary = missing.clone();
        match goal {
            "all" => equivalent.reconstruct(&mut ordinary),
            "data" => equivalent.reconstruct_data(&mut ordinary),
            _ => equivalent.reconstruct_some(&mut ordinary, &required),
        }
        .unwrap();
        equivalent
            .reconstruct_with_workspace(&mut missing, &workspace)
            .unwrap();
        assert_eq!(missing, ordinary);
    }
}

#[test]
fn serial_workspace_supports_thread_local_field_elements() {
    use std::{marker::PhantomData, rc::Rc};
    type Local = (u8, PhantomData<Rc<()>>);
    struct LocalField;
    impl Field for LocalField {
        const ORDER: usize = 256;
        type Elem = Local;
        fn add(a: Local, b: Local) -> Local {
            (galois_8::add(a.0, b.0), PhantomData)
        }
        fn mul(a: Local, b: Local) -> Local {
            (galois_8::mul(a.0, b.0), PhantomData)
        }
        fn div(a: Local, b: Local) -> Local {
            (galois_8::div(a.0, b.0), PhantomData)
        }
        fn exp(a: Local, n: usize) -> Local {
            (galois_8::exp(a.0, n), PhantomData)
        }
        fn zero() -> Local {
            (0, PhantomData)
        }
        fn one() -> Local {
            (1, PhantomData)
        }
        fn nth_internal(n: usize) -> Local {
            (n as u8, PhantomData)
        }
    }
    let codec = ReedSolomon::<LocalField>::new(4, 3).unwrap();
    let expected = encoded(&codec, 65, 7);
    let mut missing = slots::<LocalField>(&expected, &[0, 1, 4]);
    let workspace = codec.prepare_reconstruct_workspace(&missing).unwrap();
    codec
        .reconstruct_with_workspace(&mut missing, &workspace)
        .unwrap();
    check::<LocalField>(&missing, &expected);
    for data_only in [false, true] {
        let mut missing = slots::<LocalField>(&expected, &[0, 1, 4]);
        let mut ordinary = missing.clone();
        let required = [false, false, false, false, true, false, false];
        let plan = if data_only {
            codec.reconstruct_data(&mut ordinary).unwrap();
            codec.prepare_reconstruct_data_workspace(&missing)
        } else {
            codec.reconstruct_some(&mut ordinary, &required).unwrap();
            codec.prepare_reconstruct_some_workspace(&missing, &required)
        }
        .unwrap();
        codec
            .reconstruct_with_workspace(&mut missing, &plan)
            .unwrap();
        assert_eq!(missing, ordinary);
    }
}

#[cfg(feature = "std")]
#[test]
fn gf8_legacy_workspace_alias_and_bulk_executor_remain_interoperable() {
    let codec = galois_8::ReedSolomon::new(4, 3).unwrap();
    let expected = encoded(&codec, 65, 11);
    let mut options: Vec<_> = expected.iter().cloned().map(Some).collect();
    for index in [0, 1, 4] {
        options[index] = None;
    }
    let generic: galois_8::OptionVecReconstructWorkspace =
        codec.prepare_reconstruct_workspace(&options).unwrap();
    codec
        .reconstruct_opt_with_workspace(&mut options, &generic)
        .unwrap();
    assert_eq!(
        options.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
        expected
    );
    let mut missing = slots::<galois_8::Field>(&expected, &[0, 1, 4]);
    let legacy = codec
        .prepare_reconstruct_opt_workspace(
            &expected
                .iter()
                .enumerate()
                .map(|(i, s)| (![0, 1, 4].contains(&i)).then(|| s.clone()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    codec
        .reconstruct_with_workspace(&mut missing, &legacy)
        .unwrap();
    check::<galois_8::Field>(&missing, &expected);
}

#[cfg(feature = "std")]
#[test]
fn immutable_workspace_can_be_shared_by_concurrent_disjoint_recoveries() {
    let codec = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, false);
    let initial = encoded(&codec, 65, 3);
    let workspace = codec
        .prepare_reconstruct_workspace(&slots::<galois_16::Field>(&initial, &[0, 1, 4]))
        .unwrap();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|seed| {
                let codec = &codec;
                let workspace = &workspace;
                scope.spawn(move || {
                    let expected = encoded(codec, 65, seed * 1000 + 17);
                    let mut missing = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
                    for _ in 0..3 {
                        erase::<galois_16::Field>(&mut missing, &[0, 1, 4]);
                        codec
                            .reconstruct_with_workspace(&mut missing, workspace)
                            .unwrap();
                        check::<galois_16::Field>(&missing, &expected);
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
    });
}

#[cfg(feature = "std")]
#[test]
fn caller_owned_plans_skip_replanning_and_inversion_cache_lookups() {
    fn exercise<F: Field>(field: &str) {
        for data in [4, 33] {
            for cache in [false, true] {
                for len in [65, 32768] {
                    let codec = codec::<F>(data, 3, MatrixMode::Vandermonde, false, cache);
                    let expected = encoded(&codec, len, 1919);
                    let missing = [0, 1, data];
                    let mut regular = slots::<F>(&expected, &missing);
                    let mut reused = regular.clone();
                    let workspace = codec.prepare_reconstruct_workspace(&reused).unwrap();
                    // Warm any backend state while keeping identical owners.
                    codec.reconstruct(&mut regular).unwrap();
                    codec
                        .reconstruct_with_workspace(&mut reused, &workspace)
                        .unwrap();
                    for repetition in 0..3 {
                        erase::<F>(&mut regular, &missing);
                        erase::<F>(&mut reused, &missing);
                        let requests_before_regular = codec.reconstruction_cache_stats().requests;
                        let (result, old) = counted(|| codec.reconstruct(&mut regular));
                        result.unwrap();
                        let before = codec.reconstruction_cache_stats().requests;
                        let (result, new) =
                            counted(|| codec.reconstruct_with_workspace(&mut reused, &workspace));
                        result.unwrap();
                        let regular_lookups = before - requests_before_regular;
                        let workspace_lookups =
                            codec.reconstruction_cache_stats().requests - before;
                        let metrics = cfg!(feature = "benchmark-metrics");
                        assert_eq!(regular_lookups, usize::from(cache && metrics));
                        assert_eq!(workspace_lookups, 0);
                        check::<F>(&regular, &expected);
                        check::<F>(&reused, &expected);
                        println!(
                            "RECONSTRUCT_WORKSPACE_RESOURCE field={field} data={data} parity=3 len={len} cache={cache} repetition={repetition} regular_calls={} regular_bytes={} workspace_calls={} workspace_bytes={} regular_lookups={regular_lookups} workspace_lookups={workspace_lookups} metrics={metrics}",
                            old.calls, old.bytes, new.calls, new.bytes
                        );
                        assert!(new.calls <= old.calls && new.bytes <= old.bytes);
                        if data == 4 {
                            assert_eq!(new.calls, 0);
                        }
                        if !cache {
                            assert!(new.calls < old.calls);
                        }
                    }
                }
            }
        }
    }
    exercise::<galois_8::Field>("gf8");
    exercise::<galois_16::Field>("tower-gf16");
}

#[test]
fn leopard_workspaces_are_rejected_after_shard_count_validation() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec = ReedSolomon::<galois_8::Field>::with_options(
            4,
            3,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let present = slots::<galois_8::Field>(&encoded(&codec, 64, 17), &[]);
        assert!(matches!(
            codec.prepare_reconstruct_workspace(&present),
            Err(Error::UnsupportedCodecFamily)
        ));
        assert!(matches!(
            codec.prepare_reconstruct_workspace(&present[..6]),
            Err(Error::TooFewShards)
        ));
        let classic = ReedSolomon::<galois_8::Field>::new(4, 3).unwrap();
        let workspace = classic.prepare_reconstruct_workspace(&present).unwrap();
        let mut candidate = present.clone();
        let before = candidate.clone();
        assert_eq!(
            codec.reconstruct_with_workspace(&mut candidate, &workspace),
            Err(Error::UnsupportedCodecFamily)
        );
        assert_eq!(candidate, before);
    }
}

#[test]
fn data_and_selected_plans_match_existing_recovery_for_all_small_masks() {
    fn exercise<F: Field>() {
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ] {
            for (parity, fast) in [(2, false), (1, true)] {
                for cache in [false, true] {
                    let codec = codec::<F>(3, parity, mode, fast, cache);
                    let expected = encoded(&codec, 65, 3791);
                    let total = codec.total_shard_count();
                    for erasures in 0usize..1 << total {
                        if erasures.count_ones() as usize > parity {
                            continue;
                        }
                        let missing: Vec<_> =
                            (0..total).filter(|i| erasures & (1 << i) != 0).collect();
                        let initial = slots::<F>(&expected, &missing);
                        let mut data_only = initial.clone();
                        let mut ordinary_data = initial.clone();
                        let plan = codec
                            .prepare_reconstruct_data_workspace(&data_only)
                            .unwrap();
                        assert_eq!(
                            plan.data_targets(),
                            missing
                                .iter()
                                .copied()
                                .filter(|i| *i < 3)
                                .collect::<Vec<_>>()
                        );
                        assert!(plan.parity_targets().is_empty());
                        codec.reconstruct_data(&mut ordinary_data).unwrap();
                        codec
                            .reconstruct_with_workspace(&mut data_only, &plan)
                            .unwrap();
                        assert_eq!(data_only, ordinary_data);
                        for requested in 0usize..1 << total {
                            let mask: Vec<_> =
                                (0..total).map(|i| requested & (1 << i) != 0).collect();
                            for required in [&mask[..3], mask.as_slice()] {
                                let mut ordinary = initial.clone();
                                codec.reconstruct_some(&mut ordinary, required).unwrap();
                                let mut reused = initial.clone();
                                let plan = codec
                                    .prepare_reconstruct_some_workspace(&reused, required)
                                    .unwrap();
                                let wanted: Vec<_> = missing
                                    .iter()
                                    .copied()
                                    .filter(|i| required.get(*i).copied().unwrap_or(false))
                                    .collect();
                                assert_eq!(
                                    plan.data_targets(),
                                    wanted
                                        .iter()
                                        .copied()
                                        .filter(|i| *i < 3)
                                        .collect::<Vec<_>>()
                                );
                                assert_eq!(
                                    plan.parity_targets(),
                                    wanted
                                        .iter()
                                        .copied()
                                        .filter(|i| *i >= 3)
                                        .collect::<Vec<_>>()
                                );
                                for _ in 0..2 {
                                    erase::<F>(&mut reused, &missing);
                                    codec
                                        .reconstruct_with_workspace(&mut reused, &plan)
                                        .unwrap();
                                    assert_eq!(reused, ordinary);
                                    for (i, slot) in reused.iter().enumerate() {
                                        let present = !missing.contains(&i) || wanted.contains(&i);
                                        assert_eq!(slot.is_present(), present);
                                        if present {
                                            assert_eq!(slot.as_inner(), &expected[i]);
                                        } else {
                                            assert_eq!(slot, &initial[i]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[test]
fn empty_requested_plans_skip_unused_singular_inverses_but_validate_inputs() {
    fn exercise<F: Field>() {
        let codec = ReedSolomon::<F>::with_custom_matrix(
            2,
            2,
            &[vec![F::one(); 2], vec![F::one(); 2]],
            CodecOptions::default(),
        )
        .unwrap();
        let expected = encoded(&codec, 65, 31);
        let initial = slots::<F>(&expected, &[0, 1]);
        for mask in [
            &[false, false][..],
            &[false, false, false, false],
            &[false, false, true, false],
        ] {
            let plan = codec
                .prepare_reconstruct_some_workspace(&initial, mask)
                .unwrap();
            assert!(plan.data_targets().is_empty() && plan.parity_targets().is_empty());
            let mut candidate = initial.clone();
            codec
                .reconstruct_with_workspace(&mut candidate, &plan)
                .unwrap();
            assert_eq!(candidate, initial);
            candidate[3].as_inner_mut().pop();
            let before = candidate.clone();
            assert_eq!(
                codec.reconstruct_with_workspace(&mut candidate, &plan),
                Err(Error::IncorrectShardSize)
            );
            assert_eq!(candidate, before);
        }
        assert!(matches!(
            codec.prepare_reconstruct_data_workspace(&initial),
            Err(Error::InvalidCustomMatrix)
        ));
        for size in [0, 1, 3, 5] {
            assert!(matches!(
                codec.prepare_reconstruct_some_workspace(&initial[..3], &vec![false; size]),
                Err(Error::InvalidShardFlags)
            ));
        }
        assert!(matches!(
            codec.prepare_reconstruct_some_workspace(&initial[..3], &[false; 4]),
            Err(Error::TooFewShards)
        ));
        let mut insufficient = initial.clone();
        insufficient[2].mark_missing();
        assert!(matches!(
            codec.prepare_reconstruct_some_workspace(&insufficient, &[false; 4]),
            Err(Error::TooFewShardsPresent)
        ));
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec = ReedSolomon::<galois_8::Field>::with_options(
            4,
            3,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let shards = slots::<galois_8::Field>(&encoded(&codec, 64, 71), &[0]);
        assert!(matches!(
            codec.prepare_reconstruct_data_workspace(&shards),
            Err(Error::UnsupportedCodecFamily)
        ));
        assert!(matches!(
            codec.prepare_reconstruct_some_workspace(&shards, &[false; 7]),
            Err(Error::UnsupportedCodecFamily)
        ));
        assert!(matches!(
            codec.prepare_reconstruct_some_workspace(&shards[..6], &[false; 6]),
            Err(Error::InvalidShardFlags)
        ));
        assert!(matches!(
            codec.prepare_reconstruct_some_workspace(&shards[..6], &[false; 7]),
            Err(Error::TooFewShards)
        ));
    }
}

#[test]
fn subset_plans_preserve_partial_publication_when_a_requested_buffer_is_short() {
    let codec = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, false);
    let expected = encoded(&codec, 65, 191);
    for data_only in [false, true] {
        let mut reused = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
        reused[1].as_inner_mut().pop();
        let mut ordinary = reused.clone();
        let required = [true, true, false, false, true, false, false];
        let plan = if data_only {
            codec.prepare_reconstruct_data_workspace(&reused)
        } else {
            codec.prepare_reconstruct_some_workspace(&reused, &required)
        }
        .unwrap();
        let result = if data_only {
            codec.reconstruct_data(&mut ordinary)
        } else {
            codec.reconstruct_some(&mut ordinary, &required)
        };
        assert_eq!(result, Err(Error::IncorrectShardSize));
        assert_eq!(
            codec.reconstruct_with_workspace(&mut reused, &plan),
            Err(Error::IncorrectShardSize)
        );
        assert_eq!(reused, ordinary);
        assert!(reused[0].is_present());
        assert_eq!(reused[0].as_inner(), &expected[0]);
        assert!(!reused[1].is_present() && !reused[4].is_present());
    }
}

#[cfg(feature = "std")]
#[test]
fn selected_masks_are_owned_and_legacy_bulk_executes_only_prepared_targets() {
    let codec = galois_8::ReedSolomon::new(4, 3).unwrap();
    let expected = encoded(&codec, 65, 719);
    for goal in ["data", "parity", "mixed"] {
        let data_only = goal == "data";
        let mut options: Vec<_> = expected.iter().cloned().map(Some).collect();
        for i in [0, 1, 4] {
            options[i] = None;
        }
        let initial = options.clone();
        let mut required = vec![false; 7];
        required[4] = true;
        required[0] = goal == "mixed";
        let plan: galois_8::OptionVecReconstructWorkspace = if data_only {
            codec.prepare_reconstruct_data_workspace(&options)
        } else {
            codec.prepare_reconstruct_some_workspace(&options, &required)
        }
        .unwrap();
        let mut ordinary = initial.clone();
        if data_only {
            codec.reconstruct_data(&mut ordinary).unwrap();
        } else {
            codec.reconstruct_some(&mut ordinary, &required).unwrap();
        }
        required.fill(false);
        drop(required);
        codec
            .reconstruct_opt_with_workspace(&mut options, &plan)
            .unwrap();
        assert_eq!(options, ordinary);
        for (i, shard) in options.iter().enumerate() {
            assert_eq!(
                shard.is_some(),
                !([0, 1, 4].contains(&i))
                    || if data_only {
                        i < 4
                    } else {
                        i == 4 || (goal == "mixed" && i == 0)
                    }
            );
        }
    }
}

#[cfg(feature = "std")]
#[test]
fn subset_workspace_resources_reuse_data_inverse_and_composed_parity_rows() {
    fn exercise<F: Field>(field: &str) {
        for data in [4, 33] {
            for cache in [false, true] {
                let codec = codec::<F>(data, 3, MatrixMode::Vandermonde, false, cache);
                let expected = encoded(&codec, 65, 1319);
                let missing = [0, 1, data];
                for goal in ["data", "parity", "mixed"] {
                    let mut required = vec![false; data + 3];
                    if goal == "data" {
                        required[..data].fill(true);
                    } else {
                        required[data] = true;
                        required[0] = goal == "mixed";
                    }
                    let mut regular = slots::<F>(&expected, &missing);
                    let mut reused = regular.clone();
                    let plan = if goal == "data" {
                        codec.prepare_reconstruct_data_workspace(&reused)
                    } else {
                        codec.prepare_reconstruct_some_workspace(&reused, &required)
                    }
                    .unwrap();
                    if goal == "data" {
                        codec.reconstruct_data(&mut regular).unwrap();
                    } else {
                        codec.reconstruct_some(&mut regular, &required).unwrap();
                    }
                    codec
                        .reconstruct_with_workspace(&mut reused, &plan)
                        .unwrap();
                    for repetition in 0..3 {
                        erase::<F>(&mut regular, &missing);
                        erase::<F>(&mut reused, &missing);
                        let before = codec.reconstruction_cache_stats().requests;
                        let (result, old) = counted(|| {
                            if goal == "data" {
                                codec.reconstruct_data(&mut regular)
                            } else {
                                codec.reconstruct_some(&mut regular, &required)
                            }
                        });
                        result.unwrap();
                        let after_regular = codec.reconstruction_cache_stats().requests;
                        let (result, new) =
                            counted(|| codec.reconstruct_with_workspace(&mut reused, &plan));
                        result.unwrap();
                        let regular_lookups = after_regular - before;
                        let workspace_lookups =
                            codec.reconstruction_cache_stats().requests - after_regular;
                        let metrics = cfg!(feature = "benchmark-metrics");
                        assert_eq!(regular_lookups, usize::from(cache && metrics));
                        assert_eq!(workspace_lookups, 0);
                        assert_eq!(regular, reused);
                        for (i, slot) in reused.iter().enumerate() {
                            let present = !missing.contains(&i) || required[i];
                            assert_eq!(slot.is_present(), present);
                            if present {
                                assert_eq!(slot.as_inner(), &expected[i]);
                            } else {
                                assert!(slot.as_inner().iter().all(|v| *v == F::one()));
                            }
                        }
                        println!(
                            "SUBSET_WORKSPACE_RESOURCE field={field} data={data} parity=3 len=65 cache={cache} goal={goal} repetition={repetition} regular_calls={} regular_bytes={} workspace_calls={} workspace_bytes={} regular_lookups={regular_lookups} workspace_lookups={workspace_lookups} metrics={metrics}",
                            old.calls, old.bytes, new.calls, new.bytes
                        );
                        assert!(new.calls <= old.calls && new.bytes <= old.bytes);
                        if data == 4 {
                            assert_eq!(new.calls, 0);
                        }
                        if !cache {
                            assert!(new.calls < old.calls);
                        }
                    }
                }
            }
        }
    }
    exercise::<galois_8::Field>("gf8");
    exercise::<galois_16::Field>("tower-gf16");
}

#[test]
fn prepared_selective_rows_skip_repeated_scalar_coefficient_composition() {
    thread_local! {static OPS:Cell<(usize,usize)>=const{Cell::new((0,0))};}
    struct Observed;
    impl Field for Observed {
        const ORDER: usize = 256;
        type Elem = u8;
        fn add(a: u8, b: u8) -> u8 {
            OPS.with(|c| {
                let (m, n) = c.get();
                c.set((m, n + 1));
            });
            galois_8::add(a, b)
        }
        fn mul(a: u8, b: u8) -> u8 {
            OPS.with(|c| {
                let (m, n) = c.get();
                c.set((m + 1, n));
            });
            galois_8::mul(a, b)
        }
        fn div(a: u8, b: u8) -> u8 {
            galois_8::div(a, b)
        }
        fn exp(a: u8, n: usize) -> u8 {
            galois_8::exp(a, n)
        }
        fn zero() -> u8 {
            0
        }
        fn one() -> u8 {
            1
        }
        fn nth_internal(n: usize) -> u8 {
            n as u8
        }
    }
    let codec = ReedSolomon::<Observed>::new(4, 3).unwrap();
    for len in [1, 65, 4097] {
        let expected = encoded(&codec, len, 217);
        let mut required = [false; 7];
        required[4] = true;
        let mut regular = slots::<Observed>(&expected, &[0, 1, 4]);
        let mut reused = regular.clone();
        let plan = codec
            .prepare_reconstruct_some_workspace(&reused, &required)
            .unwrap();
        for repetition in 0..3 {
            erase::<Observed>(&mut regular, &[0, 1, 4]);
            erase::<Observed>(&mut reused, &[0, 1, 4]);
            OPS.with(|c| c.set((0, 0)));
            codec.reconstruct_some(&mut regular, &required).unwrap();
            let old = OPS.with(Cell::get);
            OPS.with(|c| c.set((0, 0)));
            codec
                .reconstruct_with_workspace(&mut reused, &plan)
                .unwrap();
            let new = OPS.with(Cell::get);
            assert_eq!(regular, reused);
            assert_eq!(reused[4].as_inner(), &expected[4]);
            assert!(!reused[0].is_present() && !reused[1].is_present());
            println!(
                "SUBSET_WORKSPACE_ARITHMETIC len={len} repetition={repetition} regular_mul={} regular_add={} workspace_mul={} workspace_add={}",
                old.0, old.1, new.0, new.1
            );
            assert_eq!(old.0, new.0 + 8);
            assert_eq!(old.1, new.1 + 8);
        }
    }
}

struct ViewCase {
    name: &'static str,
    data: usize,
    parity: usize,
    missing: Vec<usize>,
    goal: &'static str,
}
fn view_cases() -> Vec<ViewCase> {
    vec![
        ViewCase {
            name: "narrow",
            data: 4,
            parity: 3,
            missing: vec![0, 1, 4],
            goal: "all",
        },
        ViewCase {
            name: "input32",
            data: 32,
            parity: 3,
            missing: vec![0, 1, 32],
            goal: "all",
        },
        ViewCase {
            name: "input33",
            data: 33,
            parity: 3,
            missing: vec![0, 1, 33],
            goal: "all",
        },
        ViewCase {
            name: "input64",
            data: 64,
            parity: 3,
            missing: vec![0, 1, 64],
            goal: "all",
        },
        ViewCase {
            name: "parity32",
            data: 2,
            parity: 33,
            missing: std::iter::once(0).chain(2..34).collect(),
            goal: "parity",
        },
        ViewCase {
            name: "parity33",
            data: 2,
            parity: 34,
            missing: std::iter::once(0).chain(2..35).collect(),
            goal: "parity",
        },
        ViewCase {
            name: "parity64",
            data: 2,
            parity: 65,
            missing: std::iter::once(0).chain(2..66).collect(),
            goal: "parity",
        },
        ViewCase {
            name: "all_data_mixed",
            data: 33,
            parity: 34,
            missing: (0..34).collect(),
            goal: "all",
        },
        ViewCase {
            name: "all_data_only",
            data: 33,
            parity: 34,
            missing: (0..34).collect(),
            goal: "data",
        },
        ViewCase {
            name: "split_direct",
            data: 33,
            parity: 34,
            missing: (0..17).chain(33..50).collect(),
            goal: "selected",
        },
        ViewCase {
            name: "wide_parity_mixed",
            data: 33,
            parity: 34,
            missing: std::iter::once(0).chain(33..66).collect(),
            goal: "selected",
        },
        ViewCase {
            name: "no_output",
            data: 33,
            parity: 3,
            missing: vec![0, 1, 33],
            goal: "empty",
        },
    ]
}
fn view_mask(case: &ViewCase) -> Vec<bool> {
    (0..case.data + case.parity)
        .map(|i| {
            case.missing.contains(&i)
                && match case.goal {
                    "all" | "selected" => true,
                    "data" => i < case.data,
                    "parity" => i >= case.data,
                    _ => false,
                }
        })
        .collect()
}
fn view_plan<F: Field, T: rustfs_erasure_codec::ReconstructShard<F>>(
    codec: &ReedSolomon<F>,
    shards: &[T],
    case: &ViewCase,
    mask: &[bool],
) -> ReconstructWorkspace<F> {
    match case.goal {
        "all" => codec.prepare_reconstruct_workspace(shards),
        "data" => codec.prepare_reconstruct_data_workspace(shards),
        _ => codec.prepare_reconstruct_some_workspace(shards, mask),
    }
    .unwrap()
}
fn view_budget<F: Field>(case: &ViewCase, mask: &[bool], bulk: bool) -> Requests {
    let data = mask[..case.data].iter().filter(|b| **b).count();
    let parity = mask[case.data..].iter().filter(|b| **b).count();
    if data + parity == 0 {
        return Requests::default();
    }
    let composed = matches!(case.goal, "selected" | "parity")
        && parity > 0
        && case.missing.iter().any(|i| *i < case.data);
    let fat = std::mem::size_of::<&mut [F::Elem]>();
    let thin = std::mem::size_of::<&mut ShardSlot<Vec<F::Elem>>>();
    let tables = if bulk {
        vec![
            (case.data, fat),
            (if composed { data + parity } else { data }, fat),
            (if composed { 0 } else { parity }, fat),
        ]
    } else {
        vec![
            (case.data, fat),
            (data, thin),
            (parity, thin),
            (if !composed && parity > 0 { data } else { 0 }, fat),
        ]
    };
    Requests {
        calls: tables.iter().filter(|(n, _)| *n > 32).count(),
        bytes: tables
            .iter()
            .filter(|(n, _)| *n > 32)
            .map(|(n, size)| n * size)
            .sum(),
    }
}

#[test]
fn generic_wide_view_storage_follows_exact_plan_counts() {
    fn exercise<F: Field>(field: &str) -> bool {
        let mut bounded = true;
        for case in view_cases() {
            let codec = codec::<F>(case.data, case.parity, MatrixMode::Vandermonde, false, true);
            let expected = encoded(&codec, 65, 1949);
            let mask = view_mask(&case);
            let mut reused = slots::<F>(&expected, &case.missing);
            let plan = view_plan(&codec, &reused, &case, &mask);
            let budget = view_budget::<F>(&case, &mask, false);
            codec
                .reconstruct_with_workspace(&mut reused, &plan)
                .unwrap();
            for repetition in 0..3 {
                erase::<F>(&mut reused, &case.missing);
                let (result, requests) =
                    counted(|| codec.reconstruct_with_workspace(&mut reused, &plan));
                result.unwrap();
                for (i, slot) in reused.iter().enumerate() {
                    let present = !case.missing.contains(&i) || mask[i];
                    assert_eq!(slot.is_present(), present);
                    if present {
                        assert_eq!(slot.as_inner(), &expected[i]);
                    } else {
                        assert!(slot.as_inner().iter().all(|v| *v == F::one()));
                    }
                }
                println!(
                    "RECOVERY_VIEW_RESOURCE api=generic field={field} case={} data={} parity={} repetition={repetition} calls={} bytes={} budget_calls={} budget_bytes={}",
                    case.name,
                    case.data,
                    case.parity,
                    requests.calls,
                    requests.bytes,
                    budget.calls,
                    budget.bytes
                );
                bounded &= requests.calls <= budget.calls && requests.bytes <= budget.bytes;
            }
        }
        bounded
    }
    // Collect all controls and both fields before the final resource gate.
    let byte = exercise::<galois_8::Field>("gf8");
    let tower = exercise::<galois_16::Field>("tower-gf16");
    assert!(
        byte && tower,
        "generic view storage exceeds planned metadata capacity"
    );
}

#[cfg(feature = "std")]
#[test]
fn bulk_direct_recovery_builds_one_combined_output_view_table() {
    let mut bounded = true;
    for case in view_cases() {
        let codec =
            codec::<galois_8::Field>(case.data, case.parity, MatrixMode::Vandermonde, false, true);
        let expected = encoded(&codec, 65, 1949);
        let mask = view_mask(&case);
        let mut shards: Vec<_> = expected.iter().cloned().map(Some).collect();
        for &i in &case.missing {
            shards[i] = None;
        }
        let plan = view_plan(&codec, &shards, &case, &mask);
        let budget = view_budget::<galois_8::Field>(&case, &mask, true);
        let payload_calls = mask.iter().filter(|b| **b).count();
        let payload_bytes = payload_calls * 65;
        codec
            .reconstruct_opt_with_workspace(&mut shards, &plan)
            .unwrap();
        for repetition in 0..3 {
            for &i in &case.missing {
                shards[i] = None;
            }
            let before = codec.runtime_profile_stats();
            let (result, requests) =
                counted(|| codec.reconstruct_opt_with_workspace(&mut shards, &plan));
            result.unwrap();
            let after = codec.runtime_profile_stats();
            let data_calls =
                after.reconstruct_data_stage_calls - before.reconstruct_data_stage_calls;
            let parity_calls =
                after.reconstruct_parity_stage_calls - before.reconstruct_parity_stage_calls;
            let data_bytes =
                after.reconstruct_data_stage_bytes - before.reconstruct_data_stage_bytes;
            let parity_bytes =
                after.reconstruct_parity_stage_bytes - before.reconstruct_parity_stage_bytes;
            let metrics = cfg!(feature = "benchmark-metrics");
            let data_targets = mask[..case.data].iter().filter(|v| **v).count();
            let parity_targets = payload_calls - data_targets;
            assert_eq!(data_calls, usize::from(metrics && data_targets > 0));
            assert_eq!(parity_calls, usize::from(metrics && parity_targets > 0));
            assert_eq!(data_bytes, if metrics { data_targets * 65 } else { 0 });
            assert_eq!(parity_bytes, if metrics { parity_targets * 65 } else { 0 });
            for (i, shard) in shards.iter().enumerate() {
                let present = !case.missing.contains(&i) || mask[i];
                assert_eq!(shard.is_some(), present);
                if present {
                    assert_eq!(shard.as_ref().unwrap(), &expected[i]);
                }
            }
            let metadata_calls = requests.calls.checked_sub(payload_calls).unwrap();
            let metadata_bytes = requests.bytes.checked_sub(payload_bytes).unwrap();
            println!(
                "RECOVERY_VIEW_RESOURCE api=bulk field=gf8 case={} data={} parity={} repetition={repetition} calls={} bytes={} payload_calls={payload_calls} payload_bytes={payload_bytes} metadata_calls={metadata_calls} metadata_bytes={metadata_bytes} budget_calls={} budget_bytes={} data_calls={data_calls} parity_calls={parity_calls} data_bytes={data_bytes} parity_bytes={parity_bytes} metrics={metrics}",
                case.name,
                case.data,
                case.parity,
                requests.calls,
                requests.bytes,
                budget.calls,
                budget.bytes
            );
            bounded &= metadata_calls <= budget.calls && metadata_bytes <= budget.bytes;
        }
    }
    assert!(
        bounded,
        "bulk recovery duplicated or over-reserved view tables"
    );
}

#[test]
fn wide_fallible_destinations_publish_only_completed_earlier_targets() {
    fn exercise<F: Field>() {
        let codec = codec::<F>(33, 34, MatrixMode::Vandermonde, false, true);
        let expected = encoded(&codec, 65, 1237);
        let missing: Vec<_> = (0..34).collect();
        for selective in [false, true] {
            let mut shards = slots::<F>(&expected, &missing);
            shards[32].as_inner_mut().pop();
            let mask: Vec<_> = (0..67).map(|i| missing.contains(&i)).collect();
            let plan = if selective {
                codec.prepare_reconstruct_some_workspace(&shards, &mask)
            } else {
                codec.prepare_reconstruct_workspace(&shards)
            }
            .unwrap();
            assert_eq!(
                codec.reconstruct_with_workspace(&mut shards, &plan),
                Err(Error::IncorrectShardSize)
            );
            for (i, slot) in shards.iter().enumerate() {
                if !(32..34).contains(&i) {
                    assert!(slot.is_present());
                    assert_eq!(slot.as_inner(), &expected[i]);
                } else {
                    assert!(!slot.is_present());
                    assert!(slot.as_inner().iter().all(|v| *v == F::one()));
                }
            }
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[cfg(feature = "std")]
#[test]
fn direct_bulk_parallel_chunks_keep_target_order_and_stage_counts() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        for data in [2, 33] {
            let codec = ReedSolomon::<galois_8::Field>::with_options(
                data,
                34,
                CodecOptions::builder().max_parallel_jobs(2).build(),
            )
            .unwrap();
            let len = 512 * 1024 + 65;
            let expected = encoded(&codec, len, 1091);
            let missing: Vec<_> = std::iter::once(0).chain(data..data + 33).collect();
            let required: Vec<_> = (0..data + 34)
                .map(|i| i >= data || data == 33 && i == 0)
                .collect();
            let mut shards: Vec<_> = expected.iter().cloned().map(Some).collect();
            for &i in &missing {
                shards[i] = None;
            }
            let outputs = required
                .iter()
                .enumerate()
                .filter(|(i, b)| **b && missing.contains(i))
                .count();
            let decision = codec.parallel_policy(len, outputs);
            codec.reset_runtime_profile_stats();
            codec.reconstruct_some_opt(&mut shards, &required).unwrap();
            let stats = codec.runtime_profile_stats();
            let metrics = cfg!(feature = "benchmark-metrics");
            if metrics {
                assert_eq!(stats.reconstruct_data_stage_calls, usize::from(data == 33));
                assert_eq!(
                    stats.reconstruct_data_stage_bytes,
                    if data == 33 { len } else { 0 }
                );
                assert_eq!(stats.reconstruct_parity_stage_calls, 1);
                assert_eq!(stats.reconstruct_parity_stage_bytes, 33 * len);
                if decision.use_parallel {
                    assert!(stats.code_some_parallel_calls > 0);
                }
            }
            for (i, shard) in shards.iter().enumerate() {
                let present = !missing.contains(&i) || required[i];
                assert_eq!(shard.is_some(), present);
                if present {
                    assert_eq!(shard.as_ref().unwrap(), &expected[i]);
                }
            }
            println!(
                "RECOVERY_VIEW_PARALLEL data={data} parity=34 len={len} requested={outputs} policy_parallel={} metrics={metrics} parallel_calls={} data_calls={} parity_calls={}",
                decision.use_parallel,
                stats.code_some_parallel_calls,
                stats.reconstruct_data_stage_calls,
                stats.reconstruct_parity_stage_calls
            );
        }
    });
}

#[test]
fn complete_required_targets_reuse_the_owned_shape_index_storage() {
    fn exercise<F: Field>(field: &str) -> bool {
        let mut cases = vec![("present", 4, 3, Vec::new())];
        for missing in [1, 32, 33, 64, 65, 96] {
            cases.push(("parity", 4, missing + 1, (4..4 + missing).collect()));
        }
        for data in [4, 32, 33, 64, 65] {
            cases.push(("data", data, data + 1, (0..data).collect()));
        }
        let mut bounded = true;
        for (case, data, parity, missing) in cases {
            let codec = codec::<F>(data, parity, MatrixMode::Cauchy, false, true);
            let expected = encoded(&codec, 65, 4217);
            let mut ordinary = slots::<F>(&expected, &missing);
            let mut selected = ordinary.clone();
            // Only erasures are requested; flags on surviving shards are false.
            let required: Vec<_> = (0..data + parity).map(|i| missing.contains(&i)).collect();
            codec.reconstruct(&mut ordinary).unwrap();
            codec.reconstruct_some(&mut selected, &required).unwrap();
            for repetition in 0..3 {
                erase::<F>(&mut ordinary, &missing);
                erase::<F>(&mut selected, &missing);
                let (all_plan, all_prepare) =
                    counted(|| codec.prepare_reconstruct_workspace(&ordinary));
                let all_plan = all_plan.unwrap();
                let (selected_plan, selected_prepare) =
                    counted(|| codec.prepare_reconstruct_some_workspace(&selected, &required));
                let selected_plan = selected_plan.unwrap();
                assert_eq!(all_plan.data_targets(), selected_plan.data_targets());
                assert_eq!(all_plan.parity_targets(), selected_plan.parity_targets());
                codec
                    .reconstruct_with_workspace(&mut ordinary, &all_plan)
                    .unwrap();
                codec
                    .reconstruct_with_workspace(&mut selected, &selected_plan)
                    .unwrap();
                check::<F>(&ordinary, &expected);
                check::<F>(&selected, &expected);
                erase::<F>(&mut ordinary, &missing);
                erase::<F>(&mut selected, &missing);
                let (result, all_call) = counted(|| codec.reconstruct(&mut ordinary));
                result.unwrap();
                let (result, selected_call) =
                    counted(|| codec.reconstruct_some(&mut selected, &required));
                result.unwrap();
                check::<F>(&ordinary, &expected);
                check::<F>(&selected, &expected);
                println!(
                    "TARGET_INDEX_RESOURCE field={field} case={case} data={data} parity={parity} missing={} repetition={repetition} all_prepare_calls={} all_prepare_bytes={} selected_prepare_calls={} selected_prepare_bytes={} all_call_calls={} all_call_bytes={} selected_call_calls={} selected_call_bytes={}",
                    missing.len(),
                    all_prepare.calls,
                    all_prepare.bytes,
                    selected_prepare.calls,
                    selected_prepare.bytes,
                    all_call.calls,
                    all_call.bytes,
                    selected_call.calls,
                    selected_call.bytes
                );
                // With only data or only parity erased, both goals use the
                // same coefficients. Requested-target metadata needs no copy.
                bounded &= all_prepare.calls == selected_prepare.calls
                    && all_prepare.bytes == selected_prepare.bytes
                    && all_call.calls == selected_call.calls
                    && all_call.bytes == selected_call.bytes;
            }
        }
        bounded
    }
    let byte = exercise::<galois_8::Field>("gf8");
    let tower = exercise::<galois_16::Field>("tower-gf16");
    assert!(
        byte && tower,
        "complete required targets duplicate shape indices"
    );
}

#[test]
fn shape_index_storage_is_exact_after_validation_and_bounded_on_errors() {
    struct Case {
        name: &'static str,
        data: usize,
        parity: usize,
        missing: Vec<usize>,
        bad_length: Option<usize>,
    }
    fn cases() -> Vec<Case> {
        let mut cases = Vec::new();
        for data in [4, 32, 33, 64, 65, 96] {
            for (name, missing, bad_length) in [
                ("present", vec![], None),
                ("head", vec![0], None),
                ("middle", vec![data / 2], None),
                ("tail", vec![data + 3], None),
                ("sparse", vec![0, data / 2, data + 1], None),
                ("quorum", (0..5).collect(), None),
                ("short", vec![0], Some(64)),
                ("empty", vec![0], Some(0)),
                ("quorum_short", (0..5).collect(), Some(64)),
            ] {
                cases.push(Case {
                    name,
                    data,
                    parity: 4,
                    missing,
                    bad_length,
                });
            }
        }
        for (name, missing, bad_length) in [
            ("wide_missing", (4..69).collect(), None),
            (
                "wide_mixed",
                std::iter::once(0).chain(4..69).collect(),
                None,
            ),
            ("wide_quorum", (0..101).collect(), None),
            ("wide_short", (4..100).collect(), Some(64)),
        ] {
            cases.push(Case {
                name,
                data: 4,
                parity: 97,
                missing,
                bad_length,
            });
        }
        cases
    }
    fn expected_error(case: &Case) -> Option<Error> {
        match case.bad_length {
            Some(0) => Some(Error::EmptyShard),
            Some(_) => Some(Error::IncorrectShardSize),
            None if case.missing.len() > case.parity => Some(Error::TooFewShardsPresent),
            None => None,
        }
    }
    fn budget<F: Field>(case: &Case, full: bool, call: bool) -> Requests {
        // Missing-index growth is intentionally unchanged. Valid inputs spill
        // only after successful validation, directly to their final k width.
        let mut result = Requests::default();
        let mut capacity = 32;
        while capacity < case.missing.len() {
            capacity *= 2;
            result.calls += 1;
            result.bytes += capacity * std::mem::size_of::<usize>();
        }
        if expected_error(case).is_some() || case.missing.is_empty() {
            return result;
        }
        if case.data > 32 {
            result.calls += 1;
            result.bytes += case.data * std::mem::size_of::<usize>();
        }
        if call && full {
            let data = case.missing.iter().filter(|i| **i < case.data).count();
            let parity = case.missing.len() - data;
            let thin = std::mem::size_of::<&mut ShardSlot<Vec<F::Elem>>>();
            let fat = std::mem::size_of::<&[F::Elem]>();
            for (count, size) in [
                (case.data, fat),
                (data, thin),
                (parity, thin),
                (if parity > 0 { data } else { 0 }, fat),
            ] {
                if count > 32 {
                    result.calls += 1;
                    result.bytes += count * size;
                }
            }
        }
        result
    }
    fn exercise<F: Field>(field: &str) -> bool {
        let mut bounded = true;
        for case in cases() {
            let codec = codec::<F>(case.data, case.parity, MatrixMode::Cauchy, false, true);
            let expected = encoded(&codec, 65, 9281);
            let mut initial = slots::<F>(&expected, &case.missing);
            if let Some(len) = case.bad_length {
                initial.last_mut().unwrap().as_inner_mut().truncate(len);
            }
            let required = vec![false; case.data + case.parity];
            let error = expected_error(&case);
            if error.is_none() {
                codec.prepare_reconstruct_workspace(&initial).unwrap(); // Warm inverse cache.
            }
            for full in [true, false] {
                let prepare_budget = budget::<F>(&case, full, false);
                let call_budget = budget::<F>(&case, full, true);
                for repetition in 0..3 {
                    let (plan, prepare) = counted(|| {
                        if full {
                            codec.prepare_reconstruct_workspace(&initial)
                        } else {
                            codec.prepare_reconstruct_some_workspace(&initial, &required)
                        }
                    });
                    match (plan, error) {
                        (Ok(plan), None) => {
                            let mut prepared = initial.clone();
                            codec
                                .reconstruct_with_workspace(&mut prepared, &plan)
                                .unwrap();
                            if full {
                                check::<F>(&prepared, &expected);
                            } else {
                                assert_eq!(prepared, initial);
                            }
                        }
                        (Err(actual), Some(expected)) => assert_eq!(actual, expected),
                        _ => panic!("unexpected preparation result"),
                    }
                    let mut shards = initial.clone();
                    let (result, call) = counted(|| {
                        if full {
                            codec.reconstruct(&mut shards)
                        } else {
                            codec.reconstruct_some(&mut shards, &required)
                        }
                    });
                    assert_eq!(result, error.map_or(Ok(()), Err));
                    if full && error.is_none() {
                        check::<F>(&shards, &expected);
                    } else {
                        assert_eq!(shards, initial);
                    }
                    println!(
                        "SHAPE_INDEX_RESOURCE field={field} case={} data={} parity={} missing={} full={full} repetition={repetition} prepare_calls={} prepare_bytes={} call_calls={} call_bytes={} prepare_budget_calls={} prepare_budget_bytes={} call_budget_calls={} call_budget_bytes={}",
                        case.name,
                        case.data,
                        case.parity,
                        case.missing.len(),
                        prepare.calls,
                        prepare.bytes,
                        call.calls,
                        call.bytes,
                        prepare_budget.calls,
                        prepare_budget.bytes,
                        call_budget.calls,
                        call_budget.bytes
                    );
                    bounded &= prepare.calls <= prepare_budget.calls
                        && prepare.bytes <= prepare_budget.bytes
                        && call.calls <= call_budget.calls
                        && call.bytes <= call_budget.bytes;
                }
            }
        }
        bounded
    }
    let byte = exercise::<galois_8::Field>("gf8");
    let tower = exercise::<galois_16::Field>("tower-gf16");
    assert!(
        byte && tower,
        "shape index storage exceeds validated capacity"
    );
}
