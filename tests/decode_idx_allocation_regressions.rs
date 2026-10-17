#![cfg(feature = "std")]

use rustfs_erasure_codec::{Error, galois_8::ReedSolomon};
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

#[test]
fn warmed_progressive_decode_keeps_small_metadata_inline_above_32_total_shards() {
    let codec = ReedSolomon::new(32, 16).unwrap();
    let mut original: Vec<Vec<u8>> = (0..48)
        .map(|index| {
            (0..64)
                .map(|offset| (index * 31 + offset * 17) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut original).unwrap();
    let mut expected = vec![false; 48];
    for (index, expected) in expected.iter_mut().enumerate().take(34) {
        *expected = index != 0 && index != 2;
    }
    let inputs: Vec<Vec<Option<Vec<u8>>>> = expected
        .iter()
        .enumerate()
        .filter(|(_, expected)| **expected)
        .map(|(index, _)| {
            let mut input = vec![None; 48];
            input[index] = Some(original[index].clone());
            input
        })
        .collect();
    let mut dst = vec![None; 48];
    dst[0] = Some(vec![0; 64]);
    dst[2] = Some(vec![0; 64]);
    for input in &inputs {
        codec.decode_idx(&mut dst, Some(&expected), input).unwrap();
    }
    for shard in dst.iter_mut().flatten() {
        shard.fill(0);
    }

    ALLOCATIONS.with(|count| count.set(Some(0)));
    for input in &inputs {
        codec.decode_idx(&mut dst, Some(&expected), input).unwrap();
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());

    assert_eq!(
        allocations, 0,
        "preallocated progressive recovery must not allocate"
    );
    assert_eq!(dst[0].as_ref(), Some(&original[0]));
    assert_eq!(dst[2].as_ref(), Some(&original[2]));
    assert!(
        dst.iter()
            .enumerate()
            .all(|(index, shard)| index == 0 || index == 2 || shard.is_none())
    );
}

#[test]
fn expectation_count_validation_preserves_no_input_and_no_output_behavior() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let mut dst = vec![None; 6];
    dst[0] = Some(vec![0; 64]);
    let before = dst.clone();
    let mut input = vec![None; 6];
    input[1] = Some(vec![1; 64]);
    let overcomplete = [true; 6];
    assert_eq!(
        codec.decode_idx(&mut dst, Some(&overcomplete), &input),
        Err(Error::TooFewShardsPresent)
    );
    assert_eq!(dst, before);

    input[1] = None;
    assert_eq!(
        codec.decode_idx(&mut dst, Some(&overcomplete), &input),
        Ok(())
    );
    assert_eq!(dst, before);
    assert_eq!(
        codec.decode_idx(
            &mut dst,
            Some(&[true, false, false, false, false, false]),
            &input
        ),
        Err(Error::TooFewShardsPresent)
    );
    assert_eq!(dst, before);

    let mut no_outputs = vec![None; 6];
    input[1] = Some(vec![1; 64]);
    assert_eq!(
        codec.decode_idx(&mut no_outputs, Some(&overcomplete), &input),
        Ok(())
    );
    assert!(no_outputs.iter().all(Option::is_none));
}
