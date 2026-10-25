use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, Field, MatrixMode, ReedSolomon};
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

struct CountingAllocator;

fn record(bytes: usize) {
    let _ = REQUESTS.try_with(|requests| {
        if let Some(mut value) = requests.get() {
            value.calls += 1;
            value.bytes += bytes;
            requests.set(Some(value));
        }
    });
}

// SAFETY: All operations delegate unchanged pointers/layouts to System. The
// thread-local counter neither allocates nor accesses the allocated storage.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: GlobalAlloc callers supply a valid layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: GlobalAlloc callers supply a valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: Preserve the caller's live System allocation and new size.
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Preserve the pointer and layout of the original allocation.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn counted<T>(operation: impl FnOnce() -> T) -> (T, Requests) {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            REQUESTS.with(|requests| requests.set(None));
        }
    }
    let _stop = Stop;
    REQUESTS.with(|requests| requests.set(Some(Requests::default())));
    let result = operation();
    let requests = REQUESTS.with(|requests| requests.take().unwrap());
    (result, requests)
}

fn data<F: Field>(count: usize, len: usize) -> Vec<Vec<F::Elem>> {
    (0..count)
        .map(|row| {
            (0..len)
                .map(|offset| F::nth((row * 37 + offset * 13) % 251))
                .collect()
        })
        .collect()
}

fn check_updates<F: Field>() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ] {
        for fast in [false, true] {
            let parity_count = if fast { 1 } else { 3 };
            let options = CodecOptions::builder()
                .matrix_mode(mode)
                .fast_one_parity(fast)
                .build();
            let codec = if mode == MatrixMode::Custom {
                let rows: Vec<Vec<_>> = (0..parity_count)
                    .map(|row| {
                        (0..4)
                            .map(|col| {
                                if fast {
                                    F::one()
                                } else {
                                    F::nth(row * 4 + col + 1)
                                }
                            })
                            .collect()
                    })
                    .collect();
                ReedSolomon::<F>::with_custom_matrix(4, parity_count, &rows, options).unwrap()
            } else {
                ReedSolomon::<F>::with_options(4, parity_count, options).unwrap()
            };
            for len in [
                1,
                65,
                16 * 1024 - 1,
                16 * 1024,
                16 * 1024 + 1,
                64 * 1024,
                64 * 1024 + 1,
                256 * 1024 + 17,
            ] {
                let original = data::<F>(4, len);
                let mut changed = original.clone();
                for index in [0, 3] {
                    // Change complete shards, including every chunk and the tail.
                    for value in &mut changed[index] {
                        *value = F::add(*value, F::one());
                    }
                }
                let mut parity = vec![vec![F::zero(); len]; parity_count];
                codec.encode_sep(&original, &mut parity).unwrap();
                let mut expected = vec![vec![F::zero(); len]; parity_count];
                codec.encode_sep(&changed, &mut expected).unwrap();
                let changes = [
                    Some(changed[0].as_slice()),
                    None,
                    Some(original[2].as_slice()),
                    Some(changed[3].as_slice()),
                ];
                let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
                codec.update(&refs, &changes, &mut parity).unwrap();
                assert_eq!(parity, expected, "{mode:?}, fast={fast}, len={len}");
                // Reversing the same changes must restore every original byte.
                let restore = [
                    Some(original[0].as_slice()),
                    None,
                    None,
                    Some(original[3].as_slice()),
                ];
                let refs: Vec<_> = changed.iter().map(Vec::as_slice).collect();
                codec.update(&refs, &restore, &mut parity).unwrap();
                codec.encode_sep(&original, &mut expected).unwrap();
                assert_eq!(parity, expected);
            }
        }
    }
}

#[test]
fn byte_updates_match_full_encode_across_matrices_chunks_and_tails() {
    check_updates::<rustfs_erasure_codec::galois_8::Field>();
}

#[test]
fn tower_updates_match_full_encode_in_field_element_units() {
    check_updates::<rustfs_erasure_codec::galois_16::Field>();
}

fn check_requests<F: Field>() {
    let codec = ReedSolomon::<F>::new(4, 2).unwrap();
    for (len, scratch_elements) in [
        (65, 65),
        (64 * 1024, 16 * 1024),
        (1024 * 1024, 64 * 1024),
        (4 * 1024 * 1024 + 1, 256 * 1024),
    ] {
        let original = data::<F>(4, len);
        let mut parity = vec![vec![F::zero(); len]; 2];
        codec.encode_sep(&original, &mut parity).unwrap();
        let expected = parity.clone();
        let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
        let none = [None; 4];
        let (result, requests) = counted(|| codec.update(&refs, &none, &mut parity));
        result.unwrap();
        assert_eq!(parity, expected);
        assert_eq!(requests.calls, 0, "empty update: len={len}, {requests:?}");

        // Explicitly supplied but identical shards are algebraic no-ops too.
        let changes = [Some(refs[0]), None, None, Some(refs[3])];
        let (result, requests) = counted(|| codec.update(&refs, &changes, &mut parity));
        result.unwrap();
        assert_eq!(parity, expected);
        assert_eq!(requests.calls, 0, "identical updates: {requests:?}");

        let mut changed = original.clone();
        changed[0][0] = F::add(changed[0][0], F::one());
        changed[3][len - 1] = F::add(changed[3][len - 1], F::one());
        let mut expected = expected;
        codec.encode_sep(&changed, &mut expected).unwrap();
        let changes = [
            Some(changed[0].as_slice()),
            None,
            None,
            Some(changed[3].as_slice()),
        ];
        let (result, requests) = counted(|| codec.update(&refs, &changes, &mut parity));
        result.unwrap();
        assert_eq!(parity, expected);
        assert_eq!(requests.calls, 1, "len={len}, {requests:?}");
        assert_eq!(
            requests.bytes,
            scratch_elements * size_of::<F::Elem>(),
            "len={len}, {requests:?}"
        );
    }
}

#[test]
fn sparse_updates_compute_only_changed_chunks_and_initialize_no_zero_scratch() {
    thread_local! {
        static WORK: Cell<(usize, usize, usize)> = const { Cell::new((0, 0, 0)) };
    }
    struct CountedField;
    impl Field for CountedField {
        const ORDER: usize = 256;
        type Elem = u8;
        fn add(a: u8, b: u8) -> u8 {
            WORK.with(|work| {
                let (add, mul, zero) = work.get();
                work.set((add + 1, mul, zero));
            });
            rustfs_erasure_codec::galois_8::add(a, b)
        }
        fn mul(a: u8, b: u8) -> u8 {
            rustfs_erasure_codec::galois_8::mul(a, b)
        }
        fn div(a: u8, b: u8) -> u8 {
            rustfs_erasure_codec::galois_8::div(a, b)
        }
        fn exp(a: u8, n: usize) -> u8 {
            rustfs_erasure_codec::galois_8::exp(a, n)
        }
        fn zero() -> u8 {
            WORK.with(|work| {
                let (add, mul, zero) = work.get();
                work.set((add, mul, zero + 1));
            });
            0
        }
        fn one() -> u8 {
            1
        }
        fn nth_internal(n: usize) -> u8 {
            n as u8
        }
        fn mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
            rustfs_erasure_codec::galois_8::Field::mul_slice(c, input, out);
        }
        fn mul_slice_add(c: u8, input: &[u8], out: &mut [u8]) {
            WORK.with(|work| {
                let (add, mul, zero) = work.get();
                work.set((add, mul + input.len(), zero));
            });
            rustfs_erasure_codec::galois_8::Field::mul_slice_add(c, input, out);
        }
    }
    let codec = ReedSolomon::<CountedField>::new(4, 2).unwrap();
    let len = 64 * 1024 + 17;
    let original = data::<CountedField>(4, len);
    let mut changed = original.clone();
    changed[0][len - 1] ^= 0x5a;
    let mut parity = vec![vec![0; len]; 2];
    codec.encode_sep(&original, &mut parity).unwrap();
    let mut expected = parity.clone();
    codec.encode_sep(&changed, &mut expected).unwrap();
    let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
    let changes = [Some(changed[0].as_slice()), None, None, None];
    WORK.with(|work| work.set((0, 0, 0)));
    codec.update(&refs, &changes, &mut parity).unwrap();
    let (adds, parity_input_elements, zero_initializations) = WORK.with(Cell::get);
    eprintln!(
        "RSE_UPDATE_WORK adds={adds} parity_input_elements={parity_input_elements} zero_initializations={zero_initializations}"
    );
    assert_eq!(parity, expected);
    assert_eq!(
        adds, 17,
        "the unchanged full chunk needs no delta computation"
    );
    assert_eq!(
        parity_input_elements,
        2 * 17,
        "only the changed tail enters the parity kernels"
    );
    assert_eq!(
        zero_initializations, 0,
        "compute directly into initialized Vec elements"
    );
}

#[test]
fn no_changes_allocate_nothing_and_byte_delta_storage_is_bounded() {
    check_requests::<rustfs_erasure_codec::galois_8::Field>();
}

#[test]
fn no_changes_allocate_nothing_and_tower_delta_storage_is_bounded() {
    check_requests::<rustfs_erasure_codec::galois_16::Field>();
}

#[test]
fn wide_parity_updates_do_not_allocate_row_metadata() {
    for parity_count in [32, 33] {
        let codec = rustfs_erasure_codec::galois_8::ReedSolomon::new(4, parity_count).unwrap();
        let original = data::<rustfs_erasure_codec::galois_8::Field>(4, 65);
        let mut changed = original.clone();
        changed[3][64] ^= 0x5a;
        let mut parity = vec![vec![0; 65]; parity_count];
        codec.encode_sep(&original, &mut parity).unwrap();
        let mut expected = parity.clone();
        codec.encode_sep(&changed, &mut expected).unwrap();
        let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
        let changes = [None, None, None, Some(changed[3].as_slice())];
        let (result, requests) = counted(|| codec.update(&refs, &changes, &mut parity));
        result.unwrap();
        assert_eq!(parity, expected, "parity={parity_count}");
        assert_eq!(
            requests.calls, 1,
            "only delta storage is needed: {requests:?}"
        );
        assert_eq!(requests.bytes, 65, "parity={parity_count}, {requests:?}");
    }
}

#[test]
fn complete_validation_precedes_empty_update_and_any_parity_write() {
    let codec = rustfs_erasure_codec::galois_8::ReedSolomon::new(4, 2).unwrap();
    let data = vec![vec![7; 65]; 4];
    let short = [9; 64];
    let refs: Vec<_> = data.iter().map(Vec::as_slice).collect();
    let invalid = [Some(refs[0]), None, None, Some(short.as_slice())];
    let mut parity = vec![vec![0xa5; 65]; 2];
    let before = parity.clone();
    assert_eq!(
        codec.update(&refs, &invalid, &mut parity),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(parity, before);
    let none = [None; 4];
    assert_eq!(
        codec.update(&refs[..3], &none, &mut parity),
        Err(Error::TooFewDataShards)
    );
    assert_eq!(
        codec.update(&refs, &none[..3], &mut parity),
        Err(Error::TooFewDataShards)
    );
    assert_eq!(
        codec.update(&refs, &none, &mut parity[..1]),
        Err(Error::TooFewParityShards)
    );
    parity[1].truncate(64);
    let before = parity.clone();
    assert_eq!(
        codec.update(&refs, &none, &mut parity),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(parity, before);
    let empty = [&[][..]; 4];
    assert_eq!(
        codec.update(&empty, &none, &mut parity),
        Err(Error::EmptyShard)
    );
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let codec = rustfs_erasure_codec::galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        assert_eq!(
            codec.update(&refs, &none, &mut parity),
            Err(Error::UnsupportedCodecFamily)
        );
    }
}

#[test]
fn empty_update_rejects_a_first_shard_view_that_becomes_empty_after_layout_validation() {
    struct ShrinkingView {
        bytes: [u8; 65],
        remaining: Cell<usize>,
    }
    impl AsRef<[u8]> for ShrinkingView {
        fn as_ref(&self) -> &[u8] {
            let remaining = self.remaining.get();
            self.remaining.set(remaining.saturating_sub(1));
            if remaining == 0 { &[] } else { &self.bytes }
        }
    }

    let codec = rustfs_erasure_codec::galois_8::ReedSolomon::new(4, 2).unwrap();
    let original: [_; 4] = std::array::from_fn(|index| ShrinkingView {
        bytes: [7; 65],
        // The first view is valid for the existing layout/count checks, then
        // becomes empty when update obtains its length. AsRef need not be stable.
        remaining: Cell::new(if index == 0 { 3 } else { usize::MAX }),
    });
    let changes: [Option<ShrinkingView>; 4] = std::array::from_fn(|_| None);
    let mut parity = vec![vec![0xa5; 65]; 2];
    let before = parity.clone();
    assert_eq!(
        codec.update(&original, &changes, &mut parity),
        Err(Error::EmptyShard)
    );
    assert_eq!(parity, before);
}

fn tail_requests<F: Field>(field: &str) -> bool {
    let codec = ReedSolomon::<F>::new(4, 2).unwrap();
    let mut within_budget = true;
    for (len, tail) in [
        (16 * 1024 + 1, 1),
        (64 * 1024 + 1, 1),
        (64 * 1024 + 65, 65),
        (1024 * 1024 + 4097, 4097),
        (4 * 1024 * 1024 + 65, 65),
    ] {
        let original = data::<F>(4, len);
        let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
        let mut changed = original[3].clone();
        changed[len - 1] = F::add(changed[len - 1], F::one());
        let changes = [None, None, None, Some(changed.as_slice())];
        let mut changed_refs = refs.clone();
        changed_refs[3] = &changed;
        let restore = [None, None, None, Some(refs[3])];
        let mut parity = vec![vec![F::zero(); len]; 2];
        codec.encode_sep(&refs, &mut parity).unwrap();
        let original_parity = parity.clone();
        let mut expected = parity.clone();
        codec.encode_sep(&changed_refs, &mut expected).unwrap();
        for repetition in 0..3 {
            let (result, forward) = counted(|| codec.update(&refs, &changes, &mut parity));
            result.unwrap();
            assert_eq!(parity, expected);
            let (result, reverse) = counted(|| codec.update(&changed_refs, &restore, &mut parity));
            result.unwrap();
            assert_eq!(parity, original_parity);
            let budget = tail * size_of::<F::Elem>();
            println!(
                "UPDATE_TAIL_RESOURCE field={field} len={len} tail={tail} repetition={repetition} forward_calls={} forward_bytes={} reverse_calls={} reverse_bytes={} budget={budget}",
                forward.calls, forward.bytes, reverse.calls, reverse.bytes
            );
            within_budget &= forward.calls <= 1 && reverse.calls <= 1;
            within_budget &= forward.bytes <= budget && reverse.bytes <= budget;
        }
    }
    within_budget
}

#[test]
fn tail_only_updates_allocate_only_the_active_delta_range() {
    // Keep collecting both fields and all shapes before deciding the resource
    // gate, so an old implementation produces a complete comparison catalog.
    let byte = tail_requests::<rustfs_erasure_codec::galois_8::Field>("gf8");
    let tower = tail_requests::<rustfs_erasure_codec::galois_16::Field>("tower-gf16");
    assert!(byte && tower, "short deltas reserved a full coding chunk");
}

#[test]
fn a_short_delta_can_grow_for_a_later_full_chunk_and_be_reused() {
    fn check<F: Field>(field: &str) {
        let codec = ReedSolomon::<F>::new(4, 2).unwrap();
        let len = 64 * 1024 + 65;
        let original = data::<F>(4, len);
        let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
        let mut first = original[0].clone();
        first[len - 1] = F::add(first[len - 1], F::one());
        let mut later = original[3].clone();
        // Change a full block after the earlier shard's short tail. The full
        // block and the final short tail must reuse the same scratch.
        later[0] = F::add(later[0], F::one());
        later[len - 1] = F::add(later[len - 1], F::one());
        let changes = [Some(first.as_slice()), None, None, Some(later.as_slice())];
        let mut changed_refs = refs.clone();
        changed_refs[0] = &first;
        changed_refs[3] = &later;
        let restore = [Some(refs[0]), None, None, Some(refs[3])];
        let mut parity = vec![vec![F::zero(); len]; 2];
        codec.encode_sep(&refs, &mut parity).unwrap();
        let original_parity = parity.clone();
        let mut expected = parity.clone();
        codec.encode_sep(&changed_refs, &mut expected).unwrap();
        for repetition in 0..3 {
            let (result, forward) = counted(|| codec.update(&refs, &changes, &mut parity));
            result.unwrap();
            assert_eq!(parity, expected);
            let (result, reverse) = counted(|| codec.update(&changed_refs, &restore, &mut parity));
            result.unwrap();
            assert_eq!(parity, original_parity);
            let budget = (65 + 64 * 1024) * size_of::<F::Elem>();
            println!(
                "UPDATE_GROW_RESOURCE field={field} repetition={repetition} forward_calls={} forward_bytes={} reverse_calls={} reverse_bytes={} budget={budget}",
                forward.calls, forward.bytes, reverse.calls, reverse.bytes
            );
            assert!(forward.calls <= 2 && reverse.calls <= 2);
            assert!(forward.bytes <= budget && reverse.bytes <= budget);
        }
    }
    check::<rustfs_erasure_codec::galois_8::Field>("gf8");
    check::<rustfs_erasure_codec::galois_16::Field>("tower-gf16");
}
