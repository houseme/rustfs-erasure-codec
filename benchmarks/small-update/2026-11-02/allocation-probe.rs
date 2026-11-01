use rustfs_erasure_codec::{CodecOptions, Field, ReedSolomon};
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

struct FourByteField;
impl FourByteField {
    fn wrap(value: u8) -> [u8; 4] {
        [
            value,
            value.rotate_left(1),
            value.rotate_left(3),
            value.rotate_left(5),
        ]
    }
}
impl Field for FourByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 4];

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

fn check<F: Field>(name: &str) {
    for (data_count, parity_count) in [(4, 2), (10, 4)] {
        let codec = ReedSolomon::<F>::new(data_count, parity_count).unwrap();
        for len in [1, 64, 255, 256, 257, 1024, 16 * 1024 + 1] {
            let original: Vec<Vec<F::Elem>> = (0..data_count)
                .map(|row| {
                    (0..len)
                        .map(|i| F::nth((row * 37 + i * 13 + 1) % F::ORDER))
                        .collect()
                })
                .collect();
            let mut original_parity = vec![vec![F::zero(); len]; parity_count];
            codec.encode_sep(&original, &mut original_parity).unwrap();
            let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
            for condition in ["none", "identical", "tail", "head", "two"] {
                let mut changed = original.clone();
                let mut changes = vec![None; data_count];
                if condition == "identical" {
                    changes[0] = Some(original[0].as_slice());
                } else if condition != "none" {
                    let offset = if condition == "tail" { len - 1 } else { 0 };
                    changed[0][offset] = F::add(changed[0][offset], F::one());
                    if condition == "two" {
                        changed[data_count - 1][len - 1] =
                            F::add(changed[data_count - 1][len - 1], F::one());
                    }
                    changes[0] = Some(changed[0].as_slice());
                    if condition == "two" {
                        changes[data_count - 1] = Some(changed[data_count - 1].as_slice());
                    }
                }
                let mut expected = original_parity.clone();
                codec.encode_sep(&changed, &mut expected).unwrap();
                let mut parity = original_parity.clone();
                codec.update(&refs, &changes, &mut parity).unwrap();
                assert_eq!(parity, expected, "warmup {name} {condition} len={len}");
                for repetition in 0..3 {
                    parity.clone_from(&original_parity);
                    let (result, requests) = counted(|| codec.update(&refs, &changes, &mut parity));
                    result.unwrap();
                    assert_eq!(parity, expected, "{name} {condition} len={len}");
                    println!(
                        "SMALL_UPDATE field={name} data={data_count} parity={parity_count} elements={len} condition={condition} repetition={repetition} calls={} bytes={}",
                        requests.calls, requests.bytes
                    );
                }
            }
        }
    }
}

fn check_fast<F: Field>(name: &str) {
    let codec =
        ReedSolomon::<F>::with_options(4, 1, CodecOptions::builder().fast_one_parity(true).build())
            .unwrap();
    for len in [64, 256, 257] {
        let original: Vec<Vec<F::Elem>> = (0..4)
            .map(|row| {
                (0..len)
                    .map(|i| F::nth((row * 37 + i * 13 + 1) % F::ORDER))
                    .collect()
            })
            .collect();
        let mut changed = original.clone();
        changed[3][len - 1] = F::add(changed[3][len - 1], F::one());
        let mut parity = vec![vec![F::zero(); len]; 1];
        let mut expected = parity.clone();
        codec.encode_sep(&original, &mut parity).unwrap();
        codec.encode_sep(&changed, &mut expected).unwrap();
        let refs: Vec<_> = original.iter().map(Vec::as_slice).collect();
        let changes = [None, None, None, Some(changed[3].as_slice())];
        let (result, requests) = counted(|| codec.update(&refs, &changes, &mut parity));
        result.unwrap();
        assert_eq!(parity, expected);
        println!(
            "SMALL_FAST_UPDATE field={name} elements={len} calls={} bytes={}",
            requests.calls, requests.bytes
        );
    }
}

#[test]
fn update_requests_and_parity_at_small_delta_boundaries() {
    check::<rustfs_erasure_codec::galois_8::Field>("gf8");
    check::<rustfs_erasure_codec::galois_16::Field>("gf16");
    check::<ThreeByteField>("custom3");
    check::<FourByteField>("custom4");
    check_fast::<rustfs_erasure_codec::galois_8::Field>("gf8");
    check_fast::<rustfs_erasure_codec::galois_16::Field>("gf16");
    check_fast::<ThreeByteField>("custom3");
    check_fast::<FourByteField>("custom4");
}
