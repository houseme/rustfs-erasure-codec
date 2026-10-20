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

#[test]
fn rejected_merge_can_be_repaired_and_retried_without_duplicate_contributions() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let mut complete: Vec<Vec<u8>> = (0..6)
        .map(|index| {
            (0..64)
                .map(|offset| (index * 31 + offset * 17) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut complete).unwrap();
    let expected = [false, true, true, true, true, false];
    let mut first_input = vec![None; 6];
    first_input[1] = Some(complete[1].clone());
    let mut second_input = vec![None; 6];
    for index in [2, 3, 4] {
        second_input[index] = Some(complete[index].clone());
    }
    let mut first = vec![None; 6];
    first[0] = Some(vec![0; 64]);
    first[5] = Some(vec![0; 64]);
    let mut second = first.clone();
    codec
        .decode_idx(&mut first, Some(&expected), &first_input)
        .unwrap();
    codec
        .decode_idx(&mut second, Some(&expected), &second_input)
        .unwrap();
    assert!(second[0].as_ref().unwrap().iter().any(|&byte| byte != 0));

    for missing_target in [false, true] {
        let mut actual = first.clone();
        let error = if missing_target {
            actual[5] = None;
            Error::TooFewShards
        } else {
            actual[5].as_mut().unwrap().truncate(63);
            Error::IncorrectShardSize
        };
        let before = actual.clone();
        assert_eq!(codec.decode_idx(&mut actual, None, &second), Err(error));
        let rejected_without_mutation = actual == before;
        actual[5] = first[5].clone();
        codec.decode_idx(&mut actual, None, &second).unwrap();
        assert_eq!(
            actual[0].as_ref(),
            Some(&complete[0]),
            "retry applied a contribution twice"
        );
        assert_eq!(actual[5].as_ref(), Some(&complete[5]));
        assert!(
            rejected_without_mutation,
            "rejected merge changed a destination"
        );
        assert!(actual[1..5].iter().all(Option::is_none));
    }
}

#[test]
fn merge_keeps_per_slot_lengths_empty_pairs_and_destination_only_slots() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let mut dst = vec![
        Some(vec![1; 3]),
        Some(vec![2; 5]),
        Some(Vec::new()),
        Some(vec![7; 4]),
        None,
        None,
    ];
    let input = vec![
        Some(vec![3; 3]),
        Some(vec![4; 5]),
        Some(Vec::new()),
        None,
        None,
        None,
    ];
    ALLOCATIONS.with(|count| count.set(Some(0)));
    codec.decode_idx(&mut dst, None, &input).unwrap();
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0, "preflight must not allocate merge scratch");
    assert_eq!(
        dst,
        vec![
            Some(vec![2; 3]),
            Some(vec![6; 5]),
            Some(Vec::new()),
            Some(vec![7; 4]),
            None,
            None
        ]
    );
}

#[test]
fn decode_idx_distinguishes_excess_and_missing_slot_counts_before_mutation() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = [false, true, true, true, true, false];
    for flags in [None, Some(expected.as_slice())] {
        for (dst_count, input_count, error) in [
            (5, 6, Error::TooFewShards),
            (7, 6, Error::TooManyShards),
            (6, 5, Error::TooFewShards),
            (6, 7, Error::TooManyShards),
        ] {
            let mut dst = vec![Some(vec![0x45; 64]); dst_count];
            let input = vec![Some(vec![0x23; 64]); input_count];
            let before = dst.clone();
            assert_eq!(codec.decode_idx(&mut dst, flags, &input), Err(error));
            assert_eq!(dst, before);
        }
    }
}
