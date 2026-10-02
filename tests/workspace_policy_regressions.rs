#![cfg(feature = "std")]

use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, MatrixMode, VerifyWorkspace};

fn encoded(codec: &ReedSolomon, len: usize) -> Vec<Option<Vec<u8>>> {
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|index| {
            (0..len)
                .map(|offset| ((index * 37 + offset * 13) % 251) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards.into_iter().map(Some).collect()
}

fn codec_with_family(family: CodecFamily) -> ReedSolomon {
    ReedSolomon::with_options(4, 3, CodecOptions::builder().codec_family(family).build()).unwrap()
}

#[test]
fn workspace_reuses_equivalent_codecs_and_clones() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    let complete = encoded(&codec, 128);
    let mut missing = complete.clone();
    missing[0] = None;
    missing[6] = None;
    let workspace = codec.prepare_reconstruct_opt_workspace(&missing).unwrap();
    for compatible in [&codec, &codec.clone(), &ReedSolomon::new(4, 3).unwrap()] {
        let mut candidate = missing.clone();
        compatible
            .reconstruct_opt_with_workspace(&mut candidate, &workspace)
            .unwrap();
        assert_eq!(candidate, complete);
    }
}

#[test]
fn workspace_rejects_different_effective_matrices_without_mutation() {
    let source = ReedSolomon::new(4, 3).unwrap();
    let target = ReedSolomon::with_options(
        4,
        3,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .build(),
    )
    .unwrap();
    let mut source_shards = encoded(&source, 128);
    source_shards[0] = None;
    let workspace = source
        .prepare_reconstruct_opt_workspace(&source_shards)
        .unwrap();
    let mut target_shards = encoded(&target, 128);
    target_shards[0] = None;
    let before = target_shards.clone();
    assert_eq!(
        target.reconstruct_opt_with_workspace(&mut target_shards, &workspace),
        Err(Error::InvalidCustomMatrix)
    );
    assert_eq!(target_shards, before);
}

#[test]
fn workspace_rejects_fast_parity_matrix_mismatch() {
    let regular = ReedSolomon::new(4, 1).unwrap();
    let fast =
        ReedSolomon::with_options(4, 1, CodecOptions::builder().fast_one_parity(true).build())
            .unwrap();
    let mut shards = encoded(&regular, 128);
    shards[0] = None;
    let workspace = regular.prepare_reconstruct_opt_workspace(&shards).unwrap();
    let mut candidate = encoded(&fast, 128);
    candidate[0] = None;
    let before = candidate.clone();
    assert_eq!(
        fast.reconstruct_opt_with_workspace(&mut candidate, &workspace),
        Err(Error::InvalidCustomMatrix)
    );
    assert_eq!(candidate, before);
}

#[test]
fn workspace_rejects_different_data_and_parity_counts() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    let workspace = codec
        .prepare_reconstruct_opt_workspace(&encoded(&codec, 128))
        .unwrap();
    for (data, parity, expected) in [
        (5, 2, Error::TooFewDataShards),
        (3, 4, Error::TooManyDataShards),
        (4, 2, Error::TooManyParityShards),
        (4, 4, Error::TooFewParityShards),
    ] {
        let target = ReedSolomon::new(data, parity).unwrap();
        let mut candidate = encoded(&target, 128);
        let before = candidate.clone();
        assert_eq!(
            target.reconstruct_opt_with_workspace(&mut candidate, &workspace),
            Err(expected)
        );
        assert_eq!(candidate, before);
    }
}

#[test]
fn workspace_validates_pattern_and_size_even_for_all_present_plans() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    for originally_missing in [false, true] {
        let mut initial = encoded(&codec, 128);
        if originally_missing {
            initial[0] = None;
        }
        let workspace = codec.prepare_reconstruct_opt_workspace(&initial).unwrap();
        let mut candidate = initial.clone();
        codec
            .reconstruct_opt_with_workspace(&mut candidate, &workspace)
            .unwrap();

        let mut changed_pattern = initial.clone();
        changed_pattern[1] = None;
        assert_eq!(
            codec.reconstruct_opt_with_workspace(&mut changed_pattern, &workspace),
            Err(Error::InvalidShardFlags)
        );
        for shard in candidate.iter_mut().flatten() {
            shard.resize(256, 0);
        }
        if originally_missing {
            candidate[0] = None;
        }
        let before = candidate.clone();
        assert_eq!(
            codec.reconstruct_opt_with_workspace(&mut candidate, &workspace),
            Err(Error::IncorrectShardSize)
        );
        assert_eq!(candidate, before);
        let mut invalid_length = initial.clone();
        invalid_length[1].as_mut().unwrap().pop();
        assert_eq!(
            codec.reconstruct_opt_with_workspace(&mut invalid_length, &workspace),
            Err(Error::IncorrectShardSize)
        );
    }
}

#[test]
fn opt_reconstruction_entrypoints_validate_counts_before_noop() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    let workspace = codec
        .prepare_reconstruct_opt_workspace(&encoded(&codec, 128))
        .unwrap();
    for (count, error) in [
        (0, Error::TooFewShards),
        (6, Error::TooFewShards),
        (8, Error::TooManyShards),
    ] {
        let mut candidate = vec![Some(vec![1; 128]); count];
        let before = candidate.clone();
        assert_eq!(codec.reconstruct_opt(&mut candidate), Err(error));
        assert_eq!(codec.reconstruct_data_opt(&mut candidate), Err(error));
        assert_eq!(
            codec.reconstruct_some_opt(&mut candidate, &[false; 7]),
            Err(error)
        );
        assert!(
            matches!(codec.prepare_reconstruct_opt_workspace(&candidate), Err(e) if e == error)
        );
        assert_eq!(
            codec.reconstruct_opt_with_workspace(&mut candidate, &workspace),
            Err(error)
        );
        assert_eq!(
            codec.plan_option_vec_reconstruct_for_bench(&candidate, None),
            Err(error)
        );
        assert_eq!(
            codec.execute_option_vec_reconstruct_plan_serial_for_bench(&mut candidate, false),
            Err(error)
        );
        assert_eq!(candidate, before);
    }
}

#[test]
fn opt_reconstruction_entrypoints_validate_present_lengths_before_noop() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    for length in [0, 127] {
        let mut candidate = encoded(&codec, 128);
        candidate[1] = Some(vec![1; length]);
        let error = if length == 0 {
            Error::EmptyShard
        } else {
            Error::IncorrectShardSize
        };
        let before = candidate.clone();
        assert_eq!(codec.reconstruct_opt(&mut candidate), Err(error));
        assert_eq!(codec.reconstruct_data_opt(&mut candidate), Err(error));
        assert_eq!(
            codec.reconstruct_some_opt(&mut candidate, &[false; 7]),
            Err(error)
        );
        assert!(
            matches!(codec.prepare_reconstruct_opt_workspace(&candidate), Err(e) if e == error)
        );
        assert_eq!(candidate, before);
    }
}

#[test]
fn leopard_workspace_preparation_is_explicitly_unsupported() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let codec = codec_with_family(family);
        let mut shards = encoded(&codec, 128);
        for missing in [false, true] {
            if missing {
                shards[0] = None;
            }
            assert!(matches!(
                codec.prepare_reconstruct_opt_workspace(&shards),
                Err(Error::UnsupportedCodecFamily)
            ));
            assert_eq!(
                codec.plan_option_vec_reconstruct_for_bench(&shards, None),
                Err(Error::UnsupportedCodecFamily)
            );
        }
    }
}

#[test]
fn leopard_separate_opt_encode_rejects_mismatched_lengths_without_writing() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let codec = codec_with_family(family);
        for invalid_index in [0, 1, 4, 6] {
            let mut shards: Vec<Vec<u8>> = encoded(&codec, 128)
                .into_iter()
                .map(Option::unwrap)
                .collect();
            shards[invalid_index].pop();
            let before = shards.clone();
            let (data, parity) = shards.split_at_mut(4);
            assert_eq!(
                codec.encode_sep_opt(data, parity),
                Err(Error::IncorrectShardSize)
            );
            assert_eq!(shards, before);
        }
        for (data_count, parity_count, error) in [
            (3, 3, Error::TooFewDataShards),
            (5, 3, Error::TooManyDataShards),
            (4, 2, Error::TooFewParityShards),
            (4, 4, Error::TooManyParityShards),
        ] {
            let data = vec![vec![0; 128]; data_count];
            let mut parity = vec![vec![0; 128]; parity_count];
            assert_eq!(codec.encode_sep_opt(&data, &mut parity), Err(error));
        }
    }
}

#[test]
fn selective_opt_reconstruction_publishes_only_required_missing_shards() {
    for family in [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ] {
        let codec = codec_with_family(family);
        let complete = encoded(&codec, 128);
        for requested in [
            [true, false, false, false, false, false, true],
            [false, false, false, false, false, false, true],
        ] {
            let mut candidate = complete.clone();
            candidate[0] = None;
            candidate[1] = None;
            candidate[6] = None;
            codec
                .reconstruct_some_opt(&mut candidate, &requested)
                .unwrap();
            for index in 0..7 {
                if [0, 1, 6].contains(&index) && !requested[index] {
                    assert!(
                        candidate[index].is_none(),
                        "{family:?}: unexpected output {index}"
                    );
                } else {
                    assert_eq!(
                        candidate[index], complete[index],
                        "{family:?}: output {index}"
                    );
                }
            }
        }
    }
}

#[test]
fn selective_opt_reconstruction_normalizes_data_masks_and_rejects_invalid_masks() {
    for family in [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ] {
        let codec = codec_with_family(family);
        let complete = encoded(&codec, 128);
        let mut candidate = complete.clone();
        candidate[0] = None;
        candidate[6] = None;
        let before = candidate.clone();
        assert_eq!(
            codec.reconstruct_some_opt(&mut candidate, &[true]),
            Err(Error::InvalidShardFlags)
        );
        assert_eq!(candidate, before);
        assert_eq!(
            codec.reconstruct_some_opt(&mut candidate, &[false; 4]),
            Ok(())
        );
        assert_eq!(candidate, before);
        codec
            .reconstruct_some_opt(&mut candidate, &[true, false, false, false])
            .unwrap();
        assert_eq!(candidate[0], complete[0]);
        assert!(candidate[6].is_none());
    }
}

#[test]
fn verify_opt_workspace_does_not_resize_before_validation() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    let mut workspace = VerifyWorkspace::new(&codec, 128);
    let before = workspace.clone();
    assert_eq!(
        codec.verify_with_workspace_opt(&[], &mut workspace),
        Err(Error::TooFewShards)
    );
    assert_eq!(workspace, before);
    let invalid = vec![vec![1; 256]; 6];
    assert_eq!(
        codec.verify_with_workspace_opt(&invalid, &mut workspace),
        Err(Error::TooFewShards)
    );
    assert_eq!(workspace, before);
}

#[test]
fn benchmark_plan_entrypoints_validate_masks_and_accept_all_present_execution() {
    let codec = ReedSolomon::new(4, 3).unwrap();
    let mut complete = encoded(&codec, 128);
    assert_eq!(
        codec.plan_option_vec_reconstruct_for_bench(&complete, Some(&[true])),
        Err(Error::InvalidShardFlags)
    );
    assert_eq!(
        codec.execute_option_vec_reconstruct_plan_serial_for_bench(&mut complete, false),
        Ok(())
    );
    assert_eq!(
        codec.plan_option_vec_reconstruct_for_bench(&complete, None),
        Ok((128, 7, 0, 0))
    );
}

#[test]
fn classic_selective_recovery_matches_all_small_erasure_and_request_masks() {
    for matrix_mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::with_options(
            3,
            2,
            CodecOptions::builder().matrix_mode(matrix_mode).build(),
        )
        .unwrap();
        for len in [1, 64, 65] {
            let complete = encoded(&codec, len);
            for missing_mask in 0u32..32 {
                if missing_mask.count_ones() > 2 {
                    continue;
                }
                let mut initial = complete.clone();
                for (index, shard) in initial.iter_mut().enumerate() {
                    if missing_mask & (1 << index) != 0 {
                        *shard = None;
                    }
                }
                for required_mask in 0u32..32 {
                    let required: Vec<bool> = (0..5)
                        .map(|index| required_mask & (1 << index) != 0)
                        .collect();
                    let mut actual = initial.clone();
                    codec.reconstruct_some_opt(&mut actual, &required).unwrap();
                    for index in 0..5 {
                        let expected = if required[index] {
                            &complete[index]
                        } else {
                            &initial[index]
                        };
                        assert_eq!(
                            &actual[index], expected,
                            "matrix={matrix_mode:?}, len={len}, missing={missing_mask}, required={required_mask}, shard={index}"
                        );
                    }
                }
            }
        }
    }
}
