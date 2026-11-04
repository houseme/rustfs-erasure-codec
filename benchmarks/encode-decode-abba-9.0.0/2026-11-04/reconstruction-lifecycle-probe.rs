//! Count caller-thread allocation requests for two distinct reconstruction lifecycles.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct Counts {
    calls: usize,
    bytes: usize,
}
thread_local! { static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) }; }
struct Allocator;
fn record(bytes: usize) {
    let _ = COUNTS.try_with(|state| {
        if let Some(mut value) = state.get() {
            value.calls += 1;
            value.bytes += bytes;
            state.set(Some(value));
        }
    });
}
// SAFETY: All allocations and deallocations are forwarded to System without modification.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: The caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: The caller supplies a valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: The caller supplies a live allocation and its original layout.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies a live allocation and its original layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn counted<T>(f: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = COUNTS.try_with(|state| state.set(None));
        }
    }
    COUNTS.with(|state| {
        assert!(state.get().is_none());
        state.set(Some(Counts::default()));
    });
    let reset = Reset;
    let value = f();
    let counts = COUNTS.with(|state| state.get().unwrap());
    drop(reset);
    (value, counts)
}
macro_rules! check_version {
    ($lib:ident, $name:literal, $data:expr, $parity:expr, $len:expr, $original:expr) => {{
        let codec = $lib::galois_8::ReedSolomon::with_options(
            $data,
            $parity,
            $lib::CodecOptions::builder().max_parallel_jobs(1).build(),
        )
        .unwrap();
        let original = $original;
        let mut encoded = original.to_vec();
        codec.encode(&mut encoded).unwrap();
        let mut slots: Vec<_> = encoded
            .iter()
            .cloned()
            .map($lib::ShardSlot::new_present)
            .collect();
        let make_options = || {
            let mut shards: Vec<Option<Vec<u8>>> = encoded.iter().cloned().map(Some).collect();
            shards[0] = None;
            shards[$data] = None;
            shards
        };
        let make_selected = || {
            encoded
                .iter()
                .enumerate()
                .map(|(index, shard)| {
                    if index == 0 || index == $data {
                        None
                    } else {
                        Some(shard.clone())
                    }
                })
                .collect::<Vec<_>>()
        };
        for _ in 0..2 {
            let mut warm = make_options();
            codec.reconstruct(&mut warm).unwrap();
            slots[0].as_inner_mut()[0] ^= 0x5a;
            slots[$data].as_inner_mut()[$len - 1] ^= 0xa5;
            slots[0].mark_missing();
            slots[$data].mark_missing();
            codec.reconstruct(&mut slots).unwrap();
        }
        for repetition in 0..3 {
            let (prepared, clone_only) = counted(make_options);
            assert!(prepared[0].is_none() && prepared[$data].is_none());
            let (selected, selective_clone) = counted(make_selected);
            assert_eq!(selected, prepared);
            drop(prepared);
            drop(selected);
            let (recovered, clone_recover) = counted(|| {
                let mut shards = make_options();
                codec.reconstruct(&mut shards).unwrap();
                shards
            });
            assert!(
                recovered
                    .iter()
                    .zip(&encoded)
                    .all(|(got, want)| got.as_ref() == Some(want))
            );
            let (selected_recovered, selective_recover) = counted(|| {
                let mut shards = make_selected();
                codec.reconstruct(&mut shards).unwrap();
                shards
            });
            assert_eq!(selected_recovered, recovered);
            slots[0].as_inner_mut()[0] ^= 0x5a;
            slots[$data].as_inner_mut()[$len - 1] ^= 0xa5;
            let (_, reuse) = counted(|| {
                slots[0].mark_missing();
                slots[$data].mark_missing();
                codec.reconstruct(&mut slots).unwrap();
            });
            assert!(
                slots
                    .iter()
                    .zip(&encoded)
                    .all(|(got, want)| got.is_present() && got.as_inner() == want)
            );
            println!(
                concat!(
                    "{{\"version\":\"{}\",\"backend\":\"{}\",\"data\":{},\"parity\":{},",
                    "\"len\":{},\"repetition\":{},\"clone_calls\":{},\"clone_bytes\":{},",
                    "\"clone_recover_calls\":{},\"clone_recover_bytes\":{},",
                    "\"selective_calls\":{},\"selective_bytes\":{},",
                    "\"selective_recover_calls\":{},\"selective_recover_bytes\":{},",
                    "\"reuse_calls\":{},\"reuse_bytes\":{}}}"
                ),
                $name,
                $lib::galois_8::active_backend_name(),
                $data,
                $parity,
                $len,
                repetition,
                clone_only.calls,
                clone_only.bytes,
                clone_recover.calls,
                clone_recover.bytes,
                selective_clone.calls,
                selective_clone.bytes,
                selective_recover.calls,
                selective_recover.bytes,
                reuse.calls,
                reuse.bytes
            );
        }
    }};
}
fn main() {
    assert_eq!(
        base_codec::galois_8::active_backend_name(),
        next_codec::galois_8::active_backend_name(),
        "cross-version backend"
    );
    for (data, parity, len) in [
        (4, 2, 64),
        (4, 2, 4096),
        (4, 2, 65536),
        (10, 4, 64),
        (10, 4, 4096),
        (10, 4, 65536),
        (10, 4, 1048576),
    ] {
        let original: Vec<Vec<u8>> = (0..data + parity)
            .map(|row| (0..len).map(|i| (row * 37 + i * 13) as u8).collect())
            .collect();
        let mut old_encoded = original.clone();
        let mut new_encoded = original.clone();
        base_codec::galois_8::ReedSolomon::new(data, parity)
            .unwrap()
            .encode(&mut old_encoded)
            .unwrap();
        next_codec::galois_8::ReedSolomon::new(data, parity)
            .unwrap()
            .encode(&mut new_encoded)
            .unwrap();
        assert_eq!(old_encoded, new_encoded, "cross-version parity");
        check_version!(base_codec, "9.0.0", data, parity, len, &original);
        check_version!(next_codec, "9.1.0", data, parity, len, &original);
    }
}
