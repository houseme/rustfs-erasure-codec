#![cfg(feature = "experimental-matrix-builder")]

use rustfs_erasure_codec::{CodecOptions, Field, MatrixMode, ReedSolomon};
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

struct ThreeByteField;
impl ThreeByteField {
    fn wrap(value: u8) -> [u8; 3] {
        [value, value.rotate_left(1), value.rotate_left(3)]
    }
}
impl Field for ThreeByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 3];

    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::add(a[0], b[0]))
    }
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::mul(a[0], b[0]))
    }
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::div(a[0], b[0]))
    }
    fn exp(value: Self::Elem, n: usize) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::exp(value[0], n))
    }
    fn zero() -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::zero())
    }
    fn one() -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::one())
    }
    fn nth_internal(n: usize) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::nth_internal(n))
    }
}

struct OptInThreeByteField;
impl OptInThreeByteField {
    fn wrap(value: u8) -> [u8; 3] {
        [value, value.rotate_left(1), value.rotate_left(3)]
    }
}
impl Field for OptInThreeByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 3];
    fn supports_matrix_workspace_reuse() -> bool {
        true
    }

    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::add(a[0], b[0]))
    }
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::mul(a[0], b[0]))
    }
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::div(a[0], b[0]))
    }
    fn exp(value: Self::Elem, n: usize) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::exp(value[0], n))
    }
    fn zero() -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::zero())
    }
    fn one() -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::one())
    }
    fn nth_internal(n: usize) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::nth_internal(n))
    }
}

fn check<F: Field>(field: &str) {
    for (data_count, parity_count) in [(4, 2), (6, 2), (10, 4), (16, 8), (32, 16)] {
        let reference = ReedSolomon::<F>::new(data_count, parity_count).unwrap();
        let mut rows: Vec<Vec<F::Elem>> = (data_count..data_count + parity_count)
            .map(|row| {
                reference
                    .encoding_format()
                    .matrix_row(row)
                    .unwrap()
                    .to_vec()
            })
            .collect();
        rows.reverse();
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ] {
            let build = || {
                let options = CodecOptions::builder().matrix_mode(mode).build();
                if mode == MatrixMode::Custom {
                    ReedSolomon::<F>::with_custom_matrix(data_count, parity_count, &rows, options)
                } else {
                    ReedSolomon::<F>::with_options(data_count, parity_count, options)
                }
            };
            drop(build().unwrap());
            drop(build().unwrap());
            for repetition in 0..3 {
                let (codec, requests) = counted(build);
                let codec = codec.unwrap();
                if data_count == 32 && parity_count == 16 && mode == MatrixMode::Vandermonde {
                    // Compare against the same-size Cauchy constructor so
                    // platform-specific codec setup allocations cancel out.
                    let options = CodecOptions::builder()
                        .matrix_mode(MatrixMode::Cauchy)
                        .build();
                    let (_, control) = counted(|| {
                        ReedSolomon::<F>::with_options(data_count, parity_count, options).unwrap()
                    });
                    let extra = if field == "custom3" { 4 } else { 2 };
                    assert!(
                        requests.calls <= control.calls + extra,
                        "{field}: {requests:?} control={control:?}"
                    );
                }
                let mut shards: Vec<Vec<F::Elem>> = (0..data_count + parity_count)
                    .map(|row| {
                        (0..65)
                            .map(|i| F::nth((row * 37 + i * 13 + 1) % F::ORDER))
                            .collect()
                    })
                    .collect();
                codec.encode(&mut shards).unwrap();
                assert!(codec.verify(&shards).unwrap());
                let original = shards.clone();
                let mut missing: Vec<_> = shards.into_iter().map(Some).collect();
                missing[0] = None;
                codec.reconstruct_data(&mut missing).unwrap();
                assert_eq!(missing[0].as_ref(), Some(&original[0]));
                println!(
                    "MATRIX_RESOURCE field={field} data={data_count} parity={parity_count} mode={mode:?} repetition={repetition} calls={} bytes={}",
                    requests.calls, requests.bytes
                );
            }
        }
    }
}

#[test]
fn construction_requests_preserve_encoding_and_recovery() {
    check::<rustfs_erasure_codec::galois_8::Field>("gf8");
    check::<rustfs_erasure_codec::galois_16::Field>("gf16");
    check::<ThreeByteField>("custom3");
    check::<OptInThreeByteField>("custom3_opted");
}
