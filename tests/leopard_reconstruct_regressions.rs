use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, ShardSlot, galois_8::ReedSolomon};

const DATA: usize = 4;
const PARITY: usize = 3;
const TOTAL: usize = DATA + PARITY;
#[cfg(target_endian = "little")]
const FAMILIES: [CodecFamily; 2] = [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16];
#[cfg(target_endian = "big")]
const FAMILIES: [CodecFamily; 1] = [CodecFamily::LeopardGF8];

fn fixture(family: CodecFamily, shard_len: usize, jobs: usize) -> (ReedSolomon, Vec<Vec<u8>>) {
    let codec = ReedSolomon::with_options(
        DATA,
        PARITY,
        CodecOptions::builder()
            .codec_family(family)
            .max_parallel_jobs(jobs)
            .build(),
    )
    .unwrap();
    let mut shards: Vec<Vec<u8>> = (0..TOTAL)
        .map(|shard| {
            (0..shard_len)
                .map(|offset| ((offset * 17 + shard * 53 + offset / 11) % 256) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    (codec, shards)
}

#[test]
fn leopard_rejects_invalid_present_lengths_before_initializing_missing_shards() {
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        for invalid_len in [0, 63, 64, 129, 192] {
            let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
            shards[0] = None;
            shards[2] = Some(vec![0; invalid_len]);
            let expected = if invalid_len == 0 {
                Error::EmptyShard
            } else {
                Error::IncorrectShardSize
            };
            let before = shards.clone();
            assert_eq!(
                codec.reconstruct(&mut shards),
                Err(expected),
                "{family:?}, {invalid_len}"
            );
            assert_eq!(shards, before);
        }
    }
}

#[test]
fn leopard_validates_all_present_inputs_before_returning() {
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
        codec.reconstruct(&mut shards).unwrap();
        assert_eq!(
            shards,
            original.iter().cloned().map(Some).collect::<Vec<_>>()
        );
        shards[3] = Some(vec![0; 64]);
        assert_eq!(
            codec.reconstruct(&mut shards),
            Err(Error::IncorrectShardSize)
        );
    }
}

#[test]
fn leopard_rejects_invalid_counts_empty_shards_and_insufficient_inputs() {
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        for count in [0, TOTAL - 1, TOTAL + 1] {
            let mut shards = vec![Some(vec![0; 128]); count];
            let error = if count < TOTAL {
                Error::TooFewShards
            } else {
                Error::TooManyShards
            };
            assert_eq!(codec.reconstruct(&mut shards), Err(error));
        }
        let mut empty = vec![Some(Vec::<u8>::new()); TOTAL];
        assert_eq!(codec.reconstruct(&mut empty), Err(Error::EmptyShard));
        let mut missing = vec![None; TOTAL];
        assert_eq!(
            codec.reconstruct(&mut missing),
            Err(Error::TooFewShardsPresent)
        );
        missing[..DATA - 1].clone_from_slice(
            &original[..DATA - 1]
                .iter()
                .cloned()
                .map(Some)
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            codec.reconstruct(&mut missing),
            Err(Error::TooFewShardsPresent)
        );
    }
}

#[test]
fn leopard_data_only_recovery_preserves_missing_parity() {
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
        shards[0] = None;
        shards[DATA] = None;
        codec.reconstruct_data(&mut shards).unwrap();
        assert_eq!(shards[0].as_ref(), Some(&original[0]));
        assert!(shards[DATA].is_none());
        let before = shards.clone();
        codec.reconstruct_data(&mut shards).unwrap();
        assert_eq!(shards, before);
    }
}

#[test]
fn leopard_selective_recovery_honors_data_and_full_masks() {
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        for (mask_len, required_index) in [(DATA, 0), (TOTAL, DATA)] {
            let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
            for index in [0, 1, DATA] {
                shards[index] = None;
            }
            let mut required = vec![false; mask_len];
            required[required_index] = true;
            codec.reconstruct_some(&mut shards, &required).unwrap();
            for index in [0, 1, DATA] {
                if index == required_index {
                    assert_eq!(shards[index].as_ref(), Some(&original[index]));
                } else {
                    assert!(shards[index].is_none());
                }
            }
        }
        let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
        shards[0] = None;
        let before = shards.clone();
        codec
            .reconstruct_some(&mut shards, &[false; TOTAL])
            .unwrap();
        assert_eq!(shards, before);
        assert_eq!(
            codec.reconstruct_some(&mut shards, &[true]),
            Err(Error::InvalidShardFlags)
        );
        assert_eq!(shards, before);
    }
}

#[test]
fn leopard_recovers_multichunk_tails_in_place_and_retains_unrequested_slots() {
    for family in FAMILIES {
        for jobs in [0, 1, 2] {
            // Cross both engines' chunk boundaries, leaving a short final block.
            let (codec, original) = fixture(family, 128 * 1024 + 64, jobs);
            let mut slots: Vec<_> = original
                .iter()
                .cloned()
                .map(ShardSlot::new_present)
                .collect();
            for index in [0, 1, DATA] {
                slots[index].mark_missing();
                slots[index].as_inner_mut().fill(0xa5);
            }
            let target_ptr = slots[0].as_inner().as_ptr();
            let parity_ptr = slots[DATA].as_inner().as_ptr();
            let mut required = vec![false; TOTAL];
            required[0] = true;
            required[DATA] = true;
            codec.reconstruct_some(&mut slots, &required).unwrap();
            assert_eq!(slots[0].as_inner(), &original[0]);
            assert_eq!(slots[DATA].as_inner(), &original[DATA]);
            assert!(slots[0].is_present());
            assert!(slots[DATA].is_present());
            assert_eq!(slots[0].as_inner().as_ptr(), target_ptr);
            assert_eq!(slots[DATA].as_inner().as_ptr(), parity_ptr);
            assert!(!slots[1].is_present());
            assert!(slots[1].as_inner().iter().all(|byte| *byte == 0xa5));
            codec.reconstruct(&mut slots).unwrap();
            assert_eq!(
                slots.iter().map(ShardSlot::as_inner).collect::<Vec<_>>(),
                original.iter().collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn leopard_checks_actual_storage_in_custom_shard_containers() {
    struct MisreportedShard(Vec<u8>);
    impl rustfs_erasure_codec::ReconstructShard<rustfs_erasure_codec::galois_8::Field>
        for MisreportedShard
    {
        fn len(&self) -> Option<usize> {
            Some(128)
        }
        fn get(&mut self) -> Option<&mut [u8]> {
            Some(&mut self.0)
        }
        fn get_or_initialize(
            &mut self,
            _: usize,
        ) -> rustfs_erasure_codec::ReconstructInitResult<'_, rustfs_erasure_codec::galois_8::Field>
        {
            Ok(&mut self.0)
        }
    }
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        let mut shards: Vec<_> = original.into_iter().map(MisreportedShard).collect();
        shards[2].0.truncate(64);
        assert_eq!(
            codec.reconstruct(&mut shards),
            Err(Error::IncorrectShardSize)
        );
    }
}

#[test]
fn leopard_gf8_recovers_at_the_full_transform_order() {
    let codec = ReedSolomon::with_options(
        252,
        4,
        CodecOptions::builder()
            .codec_family(CodecFamily::LeopardGF8)
            .build(),
    )
    .unwrap();
    let mut original: Vec<Vec<u8>> = (0..256)
        .map(|shard| {
            (0..64)
                .map(|offset| ((shard * 13 + offset * 7) % 256) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut original).unwrap();
    let mut shards: Vec<_> = original.iter().cloned().map(Some).collect();
    for index in [0, 128, 251, 255] {
        shards[index] = None;
    }
    codec.reconstruct(&mut shards).unwrap();
    assert_eq!(shards, original.into_iter().map(Some).collect::<Vec<_>>());
}

#[test]
fn leopard_never_marks_stale_fixed_buffers_present_after_later_output_error() {
    for family in FAMILIES {
        let (codec, original) = fixture(family, 128, 0);
        let mut slots: Vec<_> = original
            .iter()
            .cloned()
            .map(ShardSlot::new_present)
            .collect();
        slots[0] = ShardSlot::new_missing(vec![0xa5; 128]);
        slots[1] = ShardSlot::new_missing(vec![0xa5; 64]);
        assert_eq!(
            codec.reconstruct(&mut slots),
            Err(Error::IncorrectShardSize)
        );
        assert!(slots[0].is_present());
        assert_eq!(slots[0].as_inner(), &original[0]);
        assert!(!slots[1].is_present());
        slots[1].as_inner_mut().resize(128, 0);
        codec.reconstruct(&mut slots).unwrap();
        assert_eq!(slots[1].as_inner(), &original[1]);

        let mut tuples: Vec<_> = original
            .iter()
            .cloned()
            .map(|bytes| (bytes, true))
            .collect();
        tuples[0] = (vec![0xa5; 128], false);
        tuples[1] = (vec![0xa5; 64], false);
        assert_eq!(
            codec.reconstruct(&mut tuples),
            Err(Error::IncorrectShardSize)
        );
        assert!(tuples[0].1);
        assert_eq!(tuples[0].0, original[0]);
        assert!(!tuples[1].1);
    }
}
