use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecOptions, Error, MatrixMode, ShardSlot};

fn encoded(codec: &ReedSolomon, len: usize) -> Vec<Option<Vec<u8>>> {
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|index| {
            (0..len)
                .map(|offset| (index * 71 + offset * 19 + offset / 251) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards.into_iter().map(Some).collect()
}

fn singular_codec() -> ReedSolomon {
    ReedSolomon::with_custom_matrix(2, 2, &[vec![1, 1], vec![1, 1]], CodecOptions::default())
        .unwrap()
}

#[test]
fn singular_plans_fail_before_initializing_missing_shards() {
    let codec = singular_codec();
    let complete = encoded(&codec, 65);
    for data_only in [false, true] {
        let mut shards = complete.clone();
        shards[0] = None;
        shards[1] = None;
        let original = shards.clone();
        for _ in 0..2 {
            let result = if data_only {
                codec.reconstruct_data(&mut shards)
            } else {
                codec.reconstruct(&mut shards)
            };
            assert_eq!(result, Err(Error::InvalidCustomMatrix));
            assert_eq!(shards, original);
        }
    }
    let mut slots: Vec<_> = complete
        .into_iter()
        .map(|shard| ShardSlot::new_present(shard.unwrap()))
        .collect();
    for slot in &mut slots[..2] {
        slot.mark_missing();
        slot.as_inner_mut().fill(0xa5);
    }
    let original = slots.clone();
    assert_eq!(
        codec.reconstruct(&mut slots),
        Err(Error::InvalidCustomMatrix)
    );
    assert_eq!(slots, original);
}

#[test]
fn empty_and_present_only_requests_skip_unused_singular_matrices() {
    let codec = singular_codec();
    let mut shards = encoded(&codec, 65);
    shards[0] = None;
    shards[1] = None;
    let original = shards.clone();
    for required in [
        &[false, false][..],
        &[false, false, false, false],
        &[false, false, true, false],
    ] {
        assert_eq!(codec.reconstruct_some(&mut shards, required), Ok(()));
        assert_eq!(shards, original);
        #[cfg(feature = "std")]
        {
            assert_eq!(codec.reconstruct_some_opt(&mut shards, required), Ok(()));
            assert_eq!(shards, original);
        }
    }
    shards[2].as_mut().unwrap().pop();
    assert_eq!(
        codec.reconstruct_some(&mut shards, &[false; 4]),
        Err(Error::IncorrectShardSize)
    );
    #[cfg(feature = "std")]
    assert_eq!(
        codec.reconstruct_some_opt(&mut shards, &[false; 4]),
        Err(Error::IncorrectShardSize)
    );
}

#[test]
fn selective_recovery_computes_exactly_requested_outputs_for_all_small_masks() {
    for matrix_mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        for cache in [true, false] {
            let codec = ReedSolomon::with_options(
                3,
                2,
                CodecOptions::builder()
                    .matrix_mode(matrix_mode)
                    .inversion_cache(cache)
                    .build(),
            )
            .unwrap();
            for len in [1, 65] {
                let complete = encoded(&codec, len);
                for missing_mask in 0u32..32 {
                    if missing_mask.count_ones() > 2 {
                        continue;
                    }
                    let initial: Vec<_> = complete
                        .iter()
                        .enumerate()
                        .map(|(index, shard)| {
                            if missing_mask & (1 << index) == 0 {
                                shard.clone()
                            } else {
                                None
                            }
                        })
                        .collect();
                    for required_mask in 0u32..32 {
                        let required: Vec<_> = (0..5)
                            .map(|index| required_mask & (1 << index) != 0)
                            .collect();
                        let expected: Vec<_> = (0..5)
                            .map(|index| {
                                if required[index] {
                                    complete[index].clone()
                                } else {
                                    initial[index].clone()
                                }
                            })
                            .collect();
                        let mut actual = initial.clone();
                        codec.reconstruct_some(&mut actual, &required).unwrap();
                        assert_eq!(
                            actual, expected,
                            "{matrix_mode:?}, cache={cache}, missing={missing_mask}, required={required_mask}"
                        );
                        #[cfg(feature = "std")]
                        {
                            let mut optimized = initial.clone();
                            codec
                                .reconstruct_some_opt(&mut optimized, &required)
                                .unwrap();
                            assert_eq!(optimized, expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn composed_parity_rows_support_gf16_elements() {
    let codec = rustfs_erasure_codec::galois_16::ReedSolomon::new(3, 2).unwrap();
    let mut data: Vec<Vec<[u8; 2]>> = (0..5)
        .map(|index| {
            (0..65)
                .map(|offset| [index * 17 + offset, offset * 3])
                .collect()
        })
        .collect();
    codec.encode(&mut data).unwrap();
    let original: Vec<_> = data.into_iter().map(Some).collect();
    let mut missing = original.clone();
    missing[0] = None;
    missing[4] = None;
    codec
        .reconstruct_some(&mut missing, &[false, false, false, false, true])
        .unwrap();
    assert!(missing[0].is_none());
    assert_eq!(missing[4], original[4]);
}

#[test]
fn late_fixed_buffer_errors_leave_previously_recovered_outputs_valid() {
    let codec = ReedSolomon::new(3, 2).unwrap();
    let complete = encoded(&codec, 65);
    for (first, later) in [(0, 1), (0, 4), (3, 4)] {
        for selective in [false, true] {
            let mut slots: Vec<_> = complete
                .iter()
                .map(|shard| ShardSlot::new_present(shard.clone().unwrap()))
                .collect();
            slots[first] = ShardSlot::new_missing(vec![0xa5; 65]);
            slots[later] = ShardSlot::new_missing(vec![0xa5; 64]);
            let mut required = [false; 5];
            required[first] = true;
            required[later] = true;
            let result = if selective {
                codec.reconstruct_some(&mut slots, &required)
            } else {
                codec.reconstruct(&mut slots)
            };
            assert_eq!(result, Err(Error::IncorrectShardSize));
            assert!(slots[first].is_present());
            assert_eq!(slots[first].as_inner(), complete[first].as_ref().unwrap());
            assert!(!slots[later].is_present());
            assert_eq!(slots[later].as_inner(), &vec![0xa5; 64]);

            let mut tuples: Vec<_> = complete
                .iter()
                .map(|shard| (shard.clone().unwrap(), true))
                .collect();
            tuples[first] = (vec![0xa5; 65], false);
            tuples[later] = (vec![0xa5; 64], false);
            let result = if selective {
                codec.reconstruct_some(&mut tuples, &required)
            } else {
                codec.reconstruct(&mut tuples)
            };
            assert_eq!(result, Err(Error::IncorrectShardSize));
            assert!(tuples[first].1);
            assert_eq!(&tuples[first].0, complete[first].as_ref().unwrap());
            assert!(!tuples[later].1);
        }
    }
}

#[test]
fn wide_layouts_recover_data_parity_and_selected_subsets() {
    let codec = ReedSolomon::with_options(
        40,
        40,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .build(),
    )
    .unwrap();
    let complete = encoded(&codec, 65);
    let missing_indices: Vec<_> = (0..33).chain(73..80).collect();
    let mut initial = complete.clone();
    for &index in &missing_indices {
        initial[index] = None;
    }
    let mut full = initial.clone();
    codec.reconstruct(&mut full).unwrap();
    assert_eq!(full, complete);
    let mut required = vec![false; 80];
    for index in [0, 32, 79] {
        required[index] = true;
    }
    let mut selected = initial.clone();
    codec.reconstruct_some(&mut selected, &required).unwrap();
    for index in 0..80 {
        assert_eq!(
            selected[index],
            if required[index] {
                complete[index].clone()
            } else {
                initial[index].clone()
            }
        );
    }
    #[cfg(feature = "std")]
    {
        let workspace = codec.prepare_reconstruct_opt_workspace(&initial).unwrap();
        codec
            .reconstruct_opt_with_workspace(&mut initial, &workspace)
            .unwrap();
        assert_eq!(initial, complete);
    }
}

#[cfg(all(feature = "std", feature = "benchmark-metrics"))]
#[test]
fn parity_only_and_empty_requests_never_request_an_inverse() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let complete = encoded(&codec, 65);
    let mut initial = complete.clone();
    initial[4] = None;
    initial[5] = None;
    let mut full = initial.clone();
    codec.reconstruct(&mut full).unwrap();
    assert_eq!(full, complete);
    let mut data = initial.clone();
    codec.reconstruct_data(&mut data).unwrap();
    assert_eq!(data, initial);
    codec
        .reconstruct_some(&mut data, &[false, false, false, false, true, false])
        .unwrap();
    assert_eq!(data[4], complete[4]);
    assert!(data[5].is_none());
    let mut optimized = initial.clone();
    codec.reconstruct_data_opt(&mut optimized).unwrap();
    assert_eq!(optimized, initial);
    codec
        .reconstruct_some_opt(&mut optimized, &[false; 6])
        .unwrap();
    codec.reconstruct_opt(&mut optimized).unwrap();
    assert_eq!(optimized, complete);
    let workspace = codec.prepare_reconstruct_opt_workspace(&initial).unwrap();
    codec
        .reconstruct_opt_with_workspace(&mut initial, &workspace)
        .unwrap();
    assert_eq!(initial, complete);
    assert_eq!(codec.reconstruction_cache_stats().requests, 0);
}

#[cfg(feature = "std")]
mod allocation_regressions {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static TRACKING: Cell<bool> = const { Cell::new(false) };
        static CALLS: Cell<usize> = const { Cell::new(0) };
    }

    struct CountingAllocator;

    fn record_allocation() {
        let _ = TRACKING.try_with(|tracking| {
            if tracking.get() {
                CALLS.with(|calls| calls.set(calls.get() + 1));
            }
        });
    }

    // SAFETY: All pointer/layout operations delegate unchanged to System; the
    // thread-local counters neither allocate nor modify allocation ownership.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            // SAFETY: The caller supplies a valid layout.
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            // SAFETY: Preserve the caller's valid layout and zeroing request.
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            record_allocation();
            // SAFETY: The caller supplies a live System allocation and valid size.
            unsafe { System.realloc(pointer, layout, size) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            // SAFETY: Every pointer/layout is from an unchanged System allocation.
            unsafe { System.dealloc(pointer, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    fn allocation_calls(operation: impl FnOnce()) -> usize {
        CALLS.with(|calls| calls.set(0));
        TRACKING.with(|tracking| tracking.set(true));
        operation();
        TRACKING.with(|tracking| tracking.set(false));
        CALLS.with(Cell::get)
    }

    fn assert_no_allocations(operation: impl FnOnce()) {
        assert_eq!(allocation_calls(operation), 0);
    }

    #[test]
    fn wide_parity_recovery_does_not_allocate_an_extra_row_view_array() {
        let codec = ReedSolomon::with_options(
            4,
            40,
            CodecOptions::builder()
                .codec_family(rustfs_erasure_codec::CodecFamily::Classic)
                .build(),
        )
        .unwrap();
        let original = encoded(&codec, 65);
        let mut slots: Vec<_> = original
            .iter()
            .map(|shard| ShardSlot::new_present(shard.clone().unwrap()))
            .collect();
        for measured in [false, true] {
            for slot in &mut slots[4..] {
                slot.mark_missing();
                slot.as_inner_mut().fill(0xa5);
            }
            let calls = if measured {
                allocation_calls(|| codec.reconstruct(&mut slots).unwrap())
            } else {
                codec.reconstruct(&mut slots).unwrap();
                0
            };
            // Wide index/destination metadata may spill. Borrowed coefficient
            // rows must not add a fourth allocation for a temporary view array.
            assert!(
                calls <= 3,
                "unexpected row-view allocation: {calls} requests"
            );
            for (slot, expected) in slots.iter().zip(&original) {
                assert!(slot.is_present());
                assert_eq!(slot.as_inner(), expected.as_ref().unwrap());
            }
        }
    }

    #[derive(Clone, Copy)]
    enum WideRecovery {
        Data,
        Selective,
        All,
    }

    fn check_wide_recovery(mode: WideRecovery, allocation_budget: usize) {
        let parity = if matches!(mode, WideRecovery::Data) {
            40
        } else {
            48
        };
        let codec = ReedSolomon::with_options(
            64,
            parity,
            CodecOptions::builder()
                .codec_family(rustfs_erasure_codec::CodecFamily::Classic)
                .build(),
        )
        .unwrap();
        let original = encoded(&codec, 65);
        let mut slots: Vec<_> = original
            .iter()
            .map(|shard| ShardSlot::new_present(shard.clone().unwrap()))
            .collect();
        let mut required = vec![false; codec.total_shard_count()];
        required[..33].fill(true);
        required[64] = true;
        for measured in [false, true] {
            for slot in &mut slots[..40] {
                slot.mark_missing();
                slot.as_inner_mut().fill(0xa5);
            }
            if !matches!(mode, WideRecovery::Data) {
                slots[64].mark_missing();
                slots[64].as_inner_mut().fill(0xa5);
            }
            let mut recover = || match mode {
                WideRecovery::Data => codec.reconstruct_data(&mut slots),
                WideRecovery::Selective => codec.reconstruct_some(&mut slots, &required),
                WideRecovery::All => codec.reconstruct(&mut slots),
            };
            let calls = if measured {
                allocation_calls(|| recover().unwrap())
            } else {
                recover().unwrap();
                0
            };
            for (index, (slot, expected)) in slots.iter().zip(&original).enumerate() {
                if matches!(mode, WideRecovery::Selective) && (33..40).contains(&index) {
                    assert!(!slot.is_present());
                    assert!(slot.as_inner().iter().all(|&value| value == 0xa5));
                } else {
                    assert!(slot.is_present());
                    assert_eq!(slot.as_inner(), expected.as_ref().unwrap());
                }
            }
            assert!(
                calls <= allocation_budget,
                "unneeded recovered-data views: {calls} requests, budget {allocation_budget}"
            );
        }
    }

    #[test]
    fn wide_recovery_data_only_does_not_retain_unused_output_views() {
        check_wide_recovery(WideRecovery::Data, 5);
    }

    #[test]
    fn wide_recovery_selective_parity_uses_original_survivors_without_retaining_data_views() {
        check_wide_recovery(WideRecovery::Selective, 6);
    }

    #[test]
    fn wide_recovery_full_parity_keeps_its_required_data_views() {
        check_wide_recovery(WideRecovery::All, 7);
    }

    #[test]
    fn complete_layouts_do_not_allocate_recovery_metadata() {
        for (data, parity) in [(10, 4), (40, 8), (128, 64)] {
            let codec = ReedSolomon::new(data, parity).unwrap();
            let mut shards = encoded(&codec, 65);
            let original = shards.clone();
            let required = vec![true; data + parity];
            assert_no_allocations(|| {
                codec.reconstruct(&mut shards).unwrap();
                codec.reconstruct_data(&mut shards).unwrap();
                codec.reconstruct_some(&mut shards, &required).unwrap();
                codec.reconstruct_opt(&mut shards).unwrap();
                codec.reconstruct_data_opt(&mut shards).unwrap();
                codec.reconstruct_some_opt(&mut shards, &required).unwrap();
                let workspace = codec.prepare_reconstruct_opt_workspace(&shards).unwrap();
                codec
                    .reconstruct_opt_with_workspace(&mut shards, &workspace)
                    .unwrap();
            });
            assert_eq!(shards, original);
            // A no-op still validates every present shard, including the last.
            shards.last_mut().unwrap().as_mut().unwrap().pop();
            assert_eq!(
                codec.reconstruct(&mut shards),
                Err(Error::IncorrectShardSize)
            );
            assert_eq!(
                codec.reconstruct_opt(&mut shards),
                Err(Error::IncorrectShardSize)
            );
        }
    }
}

struct LengthCountedShard {
    data: Option<Vec<u8>>,
    length_reads: core::cell::Cell<usize>,
}

impl rustfs_erasure_codec::ReconstructShard<rustfs_erasure_codec::galois_8::Field>
    for LengthCountedShard
{
    fn len(&self) -> Option<usize> {
        self.length_reads.set(self.length_reads.get() + 1);
        self.data.as_ref().map(Vec::len)
    }

    fn get(&mut self) -> Option<&mut [u8]> {
        self.data.as_deref_mut()
    }

    fn get_or_initialize(
        &mut self,
        len: usize,
    ) -> rustfs_erasure_codec::ReconstructInitResult<'_, rustfs_erasure_codec::galois_8::Field>
    {
        <Option<Vec<u8>> as rustfs_erasure_codec::ReconstructShard<
            rustfs_erasure_codec::galois_8::Field,
        >>::get_or_initialize(&mut self.data, len)
    }
}

#[test]
fn recovery_reads_each_length_once_across_complete_first_middle_and_last_erasure_layouts() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let complete = encoded(&codec, 65);
    for missing in [None, Some(0), Some(2), Some(5)] {
        for goal in 0..3 {
            let mut shards: Vec<_> = complete
                .iter()
                .enumerate()
                .map(|(index, shard)| LengthCountedShard {
                    data: if missing == Some(index) {
                        None
                    } else {
                        shard.clone()
                    },
                    length_reads: core::cell::Cell::new(0),
                })
                .collect();
            match goal {
                0 => codec.reconstruct(&mut shards),
                1 => codec.reconstruct_data(&mut shards),
                _ => codec.reconstruct_some(&mut shards, &[true; 6]),
            }
            .unwrap();
            for (index, shard) in shards.iter().enumerate() {
                assert_eq!(
                    shard.length_reads.get(),
                    1,
                    "missing={missing:?}, goal={goal}, shard={index}"
                );
                let requested = goal != 1 || index < codec.data_shard_count();
                if missing == Some(index) && !requested {
                    assert!(shard.data.is_none());
                } else {
                    assert_eq!(shard.data, complete[index]);
                }
            }
        }
    }
}

#[test]
fn later_invalid_lengths_take_priority_over_insufficient_survivors_without_publication() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let complete = encoded(&codec, 65);
    for (invalid_len, expected) in [(0, Error::EmptyShard), (64, Error::IncorrectShardSize)] {
        for goal in 0..3 {
            let mut shards = complete.clone();
            shards[..3].fill(None);
            shards[5] = Some(vec![0; invalid_len]);
            let before = shards.clone();
            let result = match goal {
                0 => codec.reconstruct(&mut shards),
                1 => codec.reconstruct_data(&mut shards),
                _ => codec.reconstruct_some(&mut shards, &[false; 6]),
            };
            assert_eq!(result, Err(expected));
            assert_eq!(shards, before);
            #[cfg(feature = "std")]
            {
                let result = match goal {
                    0 => codec.reconstruct_opt(&mut shards),
                    1 => codec.reconstruct_data_opt(&mut shards),
                    _ => codec.reconstruct_some_opt(&mut shards, &[false; 6]),
                };
                assert_eq!(result, Err(expected));
                assert_eq!(shards, before);
            }
        }
    }
}

#[test]
fn recovery_validation_keeps_mask_then_count_then_lengths_priority() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let mut shards = vec![Some(Vec::<u8>::new()); 5];
    let before = shards.clone();
    assert_eq!(
        codec.reconstruct_some(&mut shards, &[true]),
        Err(Error::InvalidShardFlags)
    );
    assert_eq!(
        codec.reconstruct_some(&mut shards, &[true; 6]),
        Err(Error::TooFewShards)
    );
    assert_eq!(
        codec.reconstruct_data(&mut shards),
        Err(Error::TooFewShards)
    );
    assert_eq!(shards, before);
    shards.push(Some(Vec::new()));
    let before = shards.clone();
    assert_eq!(
        codec.reconstruct_some(&mut shards, &[true; 6]),
        Err(Error::EmptyShard)
    );
    assert_eq!(shards, before);
}
