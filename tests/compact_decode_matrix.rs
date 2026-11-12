use rustfs_erasure_codec::{
    CodecOptions, Error, Field, MatrixMode, ReconstructInitResult, ReconstructShard, ReedSolomon,
    ShardSlot, galois_8, galois_16,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static REQUESTS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}
struct Counter;
fn record(bytes: usize) {
    let _ = REQUESTS.try_with(|requests| {
        if let Some((calls, total)) = requests.get() {
            requests.set(Some((calls + 1, total + bytes)));
        }
    });
}
// SAFETY: Delegate the original pointer/layout contracts to System. The
// thread-local counter allocates nothing and never accesses payload memory.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: Preserve the caller's valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: Preserve the layout and zero-initialization contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: Forward the live System allocation and requested size.
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: Forward the original System allocation and matching layout.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn counted(run: impl FnOnce()) -> (usize, usize) {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            REQUESTS.with(|requests| requests.set(None));
        }
    }
    REQUESTS.with(|requests| assert!(requests.replace(Some((0, 0))).is_none()));
    let _stop = Stop;
    run();
    REQUESTS.with(|requests| requests.get().unwrap())
}

fn make_codec<F: Field>(data: usize, parity: usize, mode: MatrixMode) -> ReedSolomon<F> {
    let options = CodecOptions::builder()
        .matrix_mode(mode)
        .max_parallel_jobs(2)
        .build();
    if mode == MatrixMode::Custom {
        let reference = ReedSolomon::<F>::new(data, parity).unwrap();
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

// Build parity through the existing single-input encoder, which never reads
// inverse matrices. Recovery is checked against these original complete bytes.
fn encoded<F: Field>(codec: &ReedSolomon<F>, len: usize) -> Vec<Vec<F::Elem>> {
    let data: Vec<Vec<_>> = (0..codec.data_shard_count())
        .map(|row| {
            (0..len)
                .map(|offset| F::nth((row * 71 + offset * 19 + offset / 23 + 17) % F::ORDER))
                .collect()
        })
        .collect();
    let mut parity = vec![vec![F::zero(); len]; codec.parity_shard_count()];
    for (index, input) in data.iter().enumerate() {
        codec.encode_single_sep(index, input, &mut parity).unwrap();
    }
    let mut ordinary = data.clone();
    ordinary.extend(vec![vec![F::zero(); len]; codec.parity_shard_count()]);
    codec.encode(&mut ordinary).unwrap();
    assert_eq!(&ordinary[codec.data_shard_count()..], parity);
    ordinary
}

fn erase<F: Field>(shards: &mut [ShardSlot<Vec<F::Elem>>], missing: &[usize]) {
    for &index in missing {
        shards[index].mark_missing();
        shards[index].as_inner_mut().fill(F::one());
    }
}
fn slots<F: Field>(expected: &[Vec<F::Elem>], missing: &[usize]) -> Vec<ShardSlot<Vec<F::Elem>>> {
    let mut shards: Vec<_> = expected
        .iter()
        .cloned()
        .map(ShardSlot::new_present)
        .collect();
    erase::<F>(&mut shards, missing);
    shards
}
fn check<F: Field>(shards: &[ShardSlot<Vec<F::Elem>>], expected: &[Vec<F::Elem>]) {
    for (index, (shard, bytes)) in shards.iter().zip(expected).enumerate() {
        assert!(shard.is_present(), "shard {index} was not published");
        assert_eq!(
            shard.as_inner(),
            bytes,
            "shard {index} differs from encoded input"
        );
    }
}
#[cfg(feature = "std")]
fn option_shards(expected: &[Vec<u8>], missing: &[usize]) -> Vec<Option<Vec<u8>>> {
    expected
        .iter()
        .enumerate()
        .map(|(index, bytes)| (!missing.contains(&index)).then(|| bytes.clone()))
        .collect()
}

#[test]
fn sparse_recovery_preserves_bytes_at_data_and_erasure_boundaries() {
    for data in [63, 64, 65, 128, 240] {
        let codec = make_codec::<galois_8::Field>(data, 6, MatrixMode::Cauchy);
        let expected = encoded(&codec, 65);
        let candidates = [0, data / 3, data / 2, data - 2, data - 1];
        for count in [0, 1, 2, 4, 5] {
            let missing = &candidates[..count];
            let mut shards = slots::<galois_8::Field>(&expected, missing);
            codec.reconstruct(&mut shards).unwrap();
            check::<galois_8::Field>(&shards, &expected);
            erase::<galois_8::Field>(&mut shards, missing);
            let workspace = codec.prepare_reconstruct_workspace(&shards).unwrap();
            codec
                .reconstruct_with_workspace(&mut shards, &workspace)
                .unwrap();
            check::<galois_8::Field>(&shards, &expected);
        }
    }
}

#[test]
fn sparse_recovery_supports_builtin_and_custom_matrices_in_both_fields() {
    fn exercise<F: Field>() {
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ] {
            let codec = make_codec::<F>(64, 6, mode);
            let expected = encoded(&codec, 17);
            for missing in [&[0][..], &[1, 63], &[0, 21, 42, 63]] {
                let mut shards = slots::<F>(&expected, missing);
                let workspace = codec.prepare_reconstruct_data_workspace(&shards).unwrap();
                codec
                    .reconstruct_with_workspace(&mut shards, &workspace)
                    .unwrap();
                check::<F>(&shards, &expected);
                erase::<F>(&mut shards, missing);
                codec.reconstruct_data(&mut shards).unwrap();
                check::<F>(&shards, &expected);
            }
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[test]
fn selective_parity_composition_uses_implicit_survivor_rows() {
    fn exercise<F: Field>() {
        let codec = make_codec::<F>(64, 6, MatrixMode::Cauchy);
        let expected = encoded(&codec, 65);
        let missing = [0, 21, 63, 64];
        for requested in [&[64][..], &[0, 64], &[]] {
            let required: Vec<_> = (0..70).map(|index| requested.contains(&index)).collect();
            let initial = slots::<F>(&expected, &missing);
            let mut ordinary = initial.clone();
            codec.reconstruct_some(&mut ordinary, &required).unwrap();
            let mut planned = initial.clone();
            let workspace = codec
                .prepare_reconstruct_some_workspace(&planned, &required)
                .unwrap();
            codec
                .reconstruct_with_workspace(&mut planned, &workspace)
                .unwrap();
            assert_eq!(planned, ordinary);
            for (index, shard) in planned.iter().enumerate() {
                if !missing.contains(&index) || requested.contains(&index) {
                    assert!(shard.is_present());
                    assert_eq!(shard.as_inner(), &expected[index]);
                } else {
                    assert_eq!(shard, &initial[index]);
                }
            }
        }
        // A pure parity erasure has no inverse and still borrows encoding rows.
        let fresh = make_codec::<F>(64, 6, MatrixMode::Cauchy);
        let mut parity_only = slots::<F>(&expected, &[64, 69]);
        fresh.reconstruct(&mut parity_only).unwrap();
        check::<F>(&parity_only, &expected);
        assert_eq!(fresh.reconstruction_cache_memory().entries, 0);
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[cfg(feature = "std")]
#[test]
fn parallel_and_option_entrypoints_execute_compact_prepared_coefficients() {
    let codec = make_codec::<galois_8::Field>(64, 3, MatrixMode::Cauchy);
    let expected = encoded(&codec, 512 * 1024 + 1);
    let missing = [0, 63, 64];
    let mut shards = slots::<galois_8::Field>(&expected, &missing);
    let plan = codec.prepare_reconstruct_workspace(&shards).unwrap();
    let charged = codec.reconstruction_cache_memory().retained_bytes;
    assert!(charged < 64 * 64);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    pool.install(|| {
        codec
            .reconstruct_with_workspace_par(&mut shards, &plan)
            .unwrap();
        check::<galois_8::Field>(&shards, &expected);
        drop(shards);
        let mut options = option_shards(&expected, &missing);
        codec
            .reconstruct_opt_with_workspace_par(&mut options, &plan)
            .unwrap();
        assert!(
            options
                .iter()
                .zip(&expected)
                .all(|(actual, expected)| actual.as_ref() == Some(expected))
        );
        for &index in &missing {
            options[index] = None;
        }
        codec.reconstruct_opt(&mut options).unwrap();
        assert!(
            options
                .iter()
                .zip(&expected)
                .all(|(actual, expected)| actual.as_ref() == Some(expected))
        );
    });
    let required: Vec<_> = (0..67).map(|index| index == 64).collect();
    let mut options = option_shards(&expected, &missing);
    codec.reconstruct_some_opt(&mut options, &required).unwrap();
    assert_eq!(options[64].as_ref(), Some(&expected[64]));
    assert!(options[0].is_none() && options[63].is_none());
}

#[cfg(feature = "std")]
#[test]
fn progressive_batches_recover_missing_identity_and_parity_rows() {
    let codec = make_codec::<galois_8::Field>(64, 6, MatrixMode::Vandermonde);
    let expected = encoded(&codec, 65);
    let missing = [0, 21, 63, 64];
    let survivors: Vec<_> = (0..70)
        .filter(|index| !missing.contains(index))
        .take(64)
        .collect();
    let expected_inputs: Vec<_> = (0..70).map(|index| survivors.contains(&index)).collect();
    let targets = [0, 2, 21, 63, 64, 69];
    let mut output: Vec<_> = (0..70)
        .map(|index| targets.contains(&index).then(|| vec![0; 65]))
        .collect();
    for batch in survivors.chunks(11) {
        let input: Vec<_> = (0..70)
            .map(|index| batch.contains(&index).then(|| expected[index].clone()))
            .collect();
        codec
            .decode_idx(&mut output, Some(&expected_inputs), &input)
            .unwrap();
    }
    for (index, actual) in output.iter().enumerate() {
        if targets.contains(&index) {
            assert_eq!(actual.as_ref(), Some(&expected[index]));
        } else {
            assert!(actual.is_none());
        }
    }
    assert!(codec.reconstruction_cache_memory().retained_bytes < 64 * 64);
}

#[test]
fn sparse_cache_charges_and_cold_hit_workspace_requests_are_bounded() {
    fn exercise<F: Field>(field: &str) {
        for missing in [&[0][..], &[0, 63], &[0, 21, 42, 63]] {
            let codec = make_codec::<F>(64, 6, MatrixMode::Cauchy);
            assert_eq!(codec.inversion_cache_max_bytes(), 8 * 1024 * 1024);
            let expected = encoded(&codec, 65);
            let mut shards = slots::<F>(&expected, missing);
            let cold = counted(|| codec.reconstruct(&mut shards).unwrap());
            check::<F>(&shards, &expected);
            let memory = codec.reconstruction_cache_memory();
            assert_eq!(memory.entries, 1);
            assert!(memory.retained_bytes > 0);
            assert!(memory.retained_bytes < 64 * 64 * core::mem::size_of::<F::Elem>());
            erase::<F>(&mut shards, missing);
            let hit = counted(|| codec.reconstruct(&mut shards).unwrap());
            check::<F>(&shards, &expected);
            erase::<F>(&mut shards, missing);
            let workspace = codec.prepare_reconstruct_workspace(&shards).unwrap();
            let planned = counted(|| {
                codec
                    .reconstruct_with_workspace(&mut shards, &workspace)
                    .unwrap()
            });
            check::<F>(&shards, &expected);
            println!(
                "COMPACT_DECODE_RESOURCES field={field} data=64 missing={} charged_bytes={} cold_calls={} cold_bytes={} hit_calls={} hit_bytes={} workspace_calls={} workspace_bytes={}",
                missing.len(),
                memory.retained_bytes,
                cold.0,
                cold.1,
                hit.0,
                hit.1,
                planned.0,
                planned.1
            );
            assert!(cold.0 > hit.0 && cold.1 > hit.1);
            assert!(hit.0 >= planned.0 && hit.1 >= planned.1);
        }
    }
    exercise::<galois_8::Field>("gf8");
    exercise::<galois_16::Field>("tower_gf16");
}

#[test]
fn compact_plan_survives_byte_budget_eviction_and_codec_drop() {
    fn exercise<F: Field>() {
        let probe = make_codec::<F>(64, 6, MatrixMode::Cauchy);
        let expected = encoded(&probe, 65);
        let mut shards = slots::<F>(&expected, &[0]);
        probe.reconstruct(&mut shards).unwrap();
        let entry_bytes = probe.reconstruction_cache_memory().retained_bytes;
        let codec = ReedSolomon::<F>::with_options(
            64,
            6,
            CodecOptions::builder()
                .matrix_mode(MatrixMode::Cauchy)
                .inversion_cache_capacity(100)
                .inversion_cache_max_bytes(entry_bytes)
                .build(),
        )
        .unwrap();
        erase::<F>(&mut shards, &[0]);
        let workspace = codec.prepare_reconstruct_workspace(&shards).unwrap();
        codec
            .reconstruct_with_workspace(&mut shards, &workspace)
            .unwrap();
        for index in [1, 21, 63] {
            erase::<F>(&mut shards, &[index]);
            codec.reconstruct(&mut shards).unwrap();
            check::<F>(&shards, &expected);
            let memory = codec.reconstruction_cache_memory();
            assert_eq!(memory.entries, 1);
            assert!(memory.retained_bytes <= entry_bytes);
        }
        let before = codec.reconstruction_cache_memory();
        erase::<F>(&mut shards, &[0]);
        codec
            .reconstruct_with_workspace(&mut shards, &workspace)
            .unwrap();
        check::<F>(&shards, &expected);
        assert_eq!(codec.reconstruction_cache_memory(), before);
        let replacement = codec.clone();
        drop(codec);
        erase::<F>(&mut shards, &[0]);
        replacement
            .reconstruct_with_workspace(&mut shards, &workspace)
            .unwrap();
        check::<F>(&shards, &expected);
        assert_eq!(replacement.reconstruction_cache_memory().entries, 0);
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[test]
fn compact_fallible_destinations_keep_completion_order_and_singular_inputs_unchanged() {
    use std::{cell::RefCell, rc::Rc};
    struct FallibleShard {
        slot: ShardSlot<Vec<u8>>,
        index: usize,
        calls: Rc<RefCell<Vec<usize>>>,
    }
    impl ReconstructShard<galois_8::Field> for FallibleShard {
        fn len(&self) -> Option<usize> {
            <ShardSlot<Vec<u8>> as ReconstructShard<galois_8::Field>>::len(&self.slot)
        }
        fn get(&mut self) -> Option<&mut [u8]> {
            <ShardSlot<Vec<u8>> as ReconstructShard<galois_8::Field>>::get(&mut self.slot)
        }
        fn get_or_initialize(&mut self, len: usize) -> ReconstructInitResult<'_, galois_8::Field> {
            self.calls.borrow_mut().push(self.index);
            if self.index == 21 {
                return Err(Err(Error::InvalidIndex));
            }
            <ShardSlot<Vec<u8>> as ReconstructShard<galois_8::Field>>::get_or_initialize(
                &mut self.slot,
                len,
            )
        }
    }
    let codec = make_codec::<galois_8::Field>(64, 6, MatrixMode::Cauchy);
    let expected = encoded(&codec, 65);
    let missing = [0, 21, 63, 64];
    let initial = slots::<galois_8::Field>(&expected, &missing);
    let workspace = codec.prepare_reconstruct_workspace(&initial).unwrap();
    assert!(codec.reconstruction_cache_memory().retained_bytes < 64 * 64);
    for prepared in [false, true] {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut shards: Vec<_> = initial
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, slot)| FallibleShard {
                slot,
                index,
                calls: calls.clone(),
            })
            .collect();
        let result = if prepared {
            codec.reconstruct_with_workspace(&mut shards, &workspace)
        } else {
            codec.reconstruct(&mut shards)
        };
        assert_eq!(result, Err(Error::InvalidIndex));
        assert_eq!(*calls.borrow(), [0, 21]);
        assert!(shards[0].slot.is_present());
        assert_eq!(shards[0].slot.as_inner(), &expected[0]);
        for &index in &[21, 63, 64] {
            assert_eq!(shards[index].slot, initial[index]);
        }
    }
    let singular = ReedSolomon::<galois_8::Field>::with_custom_matrix(
        64,
        4,
        &vec![vec![1; 64]; 4],
        CodecOptions::default(),
    )
    .unwrap();
    let expected = encoded(&singular, 17);
    let mut shards = slots::<galois_8::Field>(&expected, &[0, 63]);
    let before = shards.clone();
    assert_eq!(
        singular.reconstruct(&mut shards),
        Err(Error::InvalidCustomMatrix)
    );
    assert_eq!(shards, before);
    assert!(matches!(
        singular.prepare_reconstruct_workspace(&shards),
        Err(Error::InvalidCustomMatrix)
    ));
    assert_eq!(singular.reconstruction_cache_memory().entries, 0);
}

#[test]
fn local_elements_and_dense_all_data_fallback_keep_serial_support() {
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
    let codec = make_codec::<LocalField>(64, 6, MatrixMode::Cauchy);
    let expected = encoded(&codec, 17);
    let mut shards = slots::<LocalField>(&expected, &[0, 21, 63]);
    let workspace = codec.prepare_reconstruct_workspace(&shards).unwrap();
    codec
        .reconstruct_with_workspace(&mut shards, &workspace)
        .unwrap();
    check::<LocalField>(&shards, &expected);
    let codec = make_codec::<galois_8::Field>(64, 64, MatrixMode::Cauchy);
    let expected = encoded(&codec, 3);
    let missing: Vec<_> = (0..64).collect();
    let mut shards = slots::<galois_8::Field>(&expected, &missing);
    codec.reconstruct(&mut shards).unwrap();
    check::<galois_8::Field>(&shards, &expected);
    assert!(codec.reconstruction_cache_memory().retained_bytes >= 64 * 64);
}
