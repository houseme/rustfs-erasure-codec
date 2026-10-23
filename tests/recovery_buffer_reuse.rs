#![cfg(feature = "std")]

use rustfs_erasure_codec::{Field, ReedSolomon, ShardSlot, VerifyWorkspace, galois_8, galois_16};
use smallvec::SmallVec;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;

thread_local! {
    static REQUESTS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn record_request() {
    let _ = REQUESTS.try_with(|requests| {
        if let Some(count) = requests.get() {
            requests.set(Some(count + 1));
        }
    });
}

// SAFETY: every operation forwards the original allocation contract to System.
// The counter stores only integers and never accesses allocation contents.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_request();
        // SAFETY: forward the caller's unchanged valid allocation layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_request();
        // SAFETY: forward the caller's unchanged valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_request();
        // SAFETY: forward the live System allocation and requested new size.
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout still describe the original System allocation.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn requests(operation: impl FnOnce()) -> usize {
    REQUESTS.with(|counter| {
        assert!(counter.replace(Some(0)).is_none());
    });
    operation();
    REQUESTS.with(|counter| counter.replace(None).unwrap())
}

fn exercise_reuse<F: Field>() {
    for (data, parity) in [(4, 2), (10, 4)] {
        for len in [1, 65, 65536] {
            let codec = ReedSolomon::<F>::new(data, parity).unwrap();
            let mut expected: Vec<Vec<F::Elem>> = (0..data + parity)
                .map(|index| {
                    (0..len)
                        .map(|offset| F::nth_internal((index * 37 + offset * 13) % F::ORDER))
                        .collect()
                })
                .collect();
            codec.encode(&mut expected).unwrap();
            let mut slots: Vec<_> = expected
                .iter()
                .cloned()
                .map(ShardSlot::new_present)
                .collect();
            let addresses: Vec<_> = slots.iter().map(|slot| slot.as_inner().as_ptr()).collect();
            let mut verify = VerifyWorkspace::new(&codec, len);

            // Populate each inversion-cache pattern and initialize any lazy
            // backend state before counting steady-state allocation requests.
            for measured in [false, true] {
                let mut cycle = || {
                    // Encode into the same caller-owned parity buffers.
                    {
                        let mut views: SmallVec<[&mut [F::Elem]; 16]> = slots
                            .iter_mut()
                            .map(|slot| slot.as_inner_mut().as_mut_slice())
                            .collect();
                        codec.encode(&mut views).unwrap();
                    }
                    for index in [0, data] {
                        slots[index].as_inner_mut().fill(F::one());
                        slots[index].mark_missing();
                    }
                    codec.reconstruct(&mut slots).unwrap();
                    for index in [0, 1] {
                        slots[index].as_inner_mut().fill(F::one());
                        slots[index].mark_missing();
                    }
                    codec.reconstruct_data(&mut slots).unwrap();
                    let views: SmallVec<[&[F::Elem]; 16]> = slots
                        .iter()
                        .map(|slot| slot.as_inner().as_slice())
                        .collect();
                    assert!(codec.verify_with_workspace(&views, &mut verify).unwrap());
                };
                if measured {
                    let count = requests(|| {
                        for _ in 0..8 {
                            cycle();
                        }
                    });
                    assert_eq!(count, 0, "warmed owned-buffer encode/recovery/verify");
                } else {
                    cycle();
                }
            }
            for ((slot, expected), address) in slots.iter().zip(&expected).zip(addresses) {
                assert!(slot.is_present());
                assert_eq!(slot.as_inner(), expected);
                assert_eq!(slot.as_inner().as_ptr(), address);
            }

            // This deliberately includes caller cloning and dropping missing
            // buffers. It is a different ownership contract, not codec-only
            // allocation accounting or a wall-clock performance comparison.
            let cloning_requests = requests(|| {
                let mut owned: Vec<_> = expected.iter().cloned().map(Some).collect();
                owned[0] = None;
                owned[1] = None;
                codec.reconstruct_data(&mut owned).unwrap();
                assert!(
                    owned
                        .iter()
                        .zip(&expected)
                        .all(|(got, want)| got.as_ref() == Some(want))
                );
            });
            assert!(cloning_requests >= data + parity + 3);
        }
    }
}

#[test]
fn gf8_warmed_owned_buffers_keep_encode_recovery_and_verify_allocation_free() {
    exercise_reuse::<galois_8::Field>();
}

#[test]
fn gf16_warmed_owned_buffers_keep_encode_recovery_and_verify_allocation_free() {
    exercise_reuse::<galois_16::Field>();
}
