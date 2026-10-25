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
    let (workspace, expected) = {
        let source = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, false);
        let expected = encoded(&source, 65, 1234);
        let missing = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
        (
            source.prepare_reconstruct_workspace(&missing).unwrap(),
            expected,
        )
    };
    let equivalent = codec::<galois_16::Field>(4, 3, MatrixMode::Cauchy, false, true);
    let mut missing = slots::<galois_16::Field>(&expected, &[0, 1, 4]);
    equivalent
        .reconstruct_with_workspace(&mut missing, &workspace)
        .unwrap();
    check::<galois_16::Field>(&missing, &expected);
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
