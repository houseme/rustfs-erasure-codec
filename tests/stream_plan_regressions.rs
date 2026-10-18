#![cfg(feature = "std")]

use std::io::Cursor;

use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::stream::{StreamErrorKind, StreamIoMode, StreamOptions};
use rustfs_erasure_codec::{CodecOptions, Error, MatrixMode};

const BLOCK_SIZE: usize = 1024;

fn options(mode: StreamIoMode) -> StreamOptions {
    StreamOptions::new()
        .with_block_size(BLOCK_SIZE)
        .with_io_mode(mode)
}

fn encoded(codec: &ReedSolomon, len: usize) -> Vec<Vec<u8>> {
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|index| {
            (0..len)
                .map(|offset| (index * 71 + offset * 19 + offset / 251) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}

fn cursors(expected: &[Vec<u8>], missing_mask: usize) -> Vec<Cursor<Vec<u8>>> {
    expected
        .iter()
        .enumerate()
        .map(|(index, shard)| {
            Cursor::new(if missing_mask & (1 << index) != 0 {
                Vec::with_capacity(shard.len())
            } else {
                shard.clone()
            })
        })
        .collect()
}

fn check_all_recoverable_patterns(codec: &ReedSolomon) {
    let expected = encoded(codec, BLOCK_SIZE * 3 + 37);
    for mask in 0usize..1 << codec.total_shard_count() {
        if mask.count_ones() as usize > codec.parity_shard_count() {
            continue;
        }
        let mut normal: Vec<_> = expected
            .iter()
            .enumerate()
            .map(|(index, shard)| (mask & (1 << index) == 0).then(|| shard.clone()))
            .collect();
        codec.reconstruct(&mut normal).unwrap();

        for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
            let mut streamed = cursors(&expected, mask);
            codec
                .reconstruct_stream(&mut streamed, &options(mode))
                .unwrap();
            for (index, shard) in streamed.iter().enumerate() {
                assert_eq!(
                    shard.get_ref(),
                    normal[index].as_ref().unwrap(),
                    "stream and whole-shard reconstruction differ: mask {mask}, shard {index}"
                );
                assert_eq!(shard.get_ref(), &expected[index]);
            }
        }
    }
}

#[test]
fn stream_plan_matches_normal_reconstruction_for_every_recoverable_pattern() {
    for cache in [true, false] {
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
        ] {
            let codec = ReedSolomon::with_options(
                4,
                2,
                CodecOptions::builder()
                    .matrix_mode(mode)
                    .inversion_cache(cache)
                    .build(),
            )
            .unwrap();
            check_all_recoverable_patterns(&codec);
        }
        let custom = ReedSolomon::with_custom_matrix(
            4,
            2,
            &[vec![1; 4], vec![1, 2, 4, 8]],
            CodecOptions::builder().inversion_cache(cache).build(),
        )
        .unwrap();
        check_all_recoverable_patterns(&custom);
    }
}

#[test]
fn stream_plan_uses_effective_fast_single_parity_matrix() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::with_options(
            4,
            1,
            CodecOptions::builder()
                .matrix_mode(mode)
                .fast_one_parity(true)
                .build(),
        )
        .unwrap();
        check_all_recoverable_patterns(&codec);
    }
    let custom = ReedSolomon::with_custom_matrix(
        4,
        1,
        &[vec![1; 4]],
        CodecOptions::builder().fast_one_parity(true).build(),
    )
    .unwrap();
    check_all_recoverable_patterns(&custom);
}

#[test]
fn stream_plan_does_not_write_the_block_with_a_truncated_present_shard() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK_SIZE * 2 + 37);
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        for short_len in [BLOCK_SIZE - 1, BLOCK_SIZE + 17, BLOCK_SIZE * 2] {
            let mut shards = cursors(&expected, (1 << 1) | (1 << 5));
            shards[2].get_mut().truncate(short_len);
            let err = codec
                .reconstruct_stream(&mut shards, &options(mode))
                .unwrap_err();
            assert_eq!(err.shard_index, 2);
            assert!(matches!(
                err.kind,
                StreamErrorKind::Codec(Error::IncorrectShardSize)
            ));
            let recovered_prefix = short_len / BLOCK_SIZE * BLOCK_SIZE;
            for index in [1, 5] {
                assert_eq!(
                    shards[index].get_ref(),
                    &expected[index][..recovered_prefix]
                );
            }
        }
    }
}

#[test]
fn stream_plan_keeps_empty_all_present_and_unrecoverable_semantics() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut empty = vec![Cursor::new(Vec::new()); 6];
        codec
            .reconstruct_stream(&mut empty, &options(mode))
            .unwrap();
        assert!(empty.iter().all(|shard| shard.get_ref().is_empty()));

        let expected = encoded(&codec, BLOCK_SIZE + 37);
        let mut present = cursors(&expected, 0);
        for shard in &mut present {
            shard.set_position(17);
        }
        codec
            .reconstruct_stream(&mut present, &options(mode))
            .unwrap();
        assert!(present.iter().zip(&expected).all(|(actual, expected)| {
            actual.get_ref() == expected && actual.position() == expected.len() as u64
        }));

        let mut insufficient = cursors(&expected, (1 << 0) | (1 << 1) | (1 << 2));
        let err = codec
            .reconstruct_stream(&mut insufficient, &options(mode))
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(Error::TooFewShardsPresent)
        ));
        assert!(
            insufficient[..3]
                .iter()
                .all(|shard| shard.get_ref().is_empty())
        );

        let mut unequal = cursors(&expected, 0);
        unequal[3].get_mut().truncate(BLOCK_SIZE);
        let err = codec
            .reconstruct_stream(&mut unequal, &options(mode))
            .unwrap_err();
        assert_eq!(err.shard_index, 3);
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(Error::IncorrectShardSize)
        ));
    }
}

#[test]
fn stream_plan_rejects_singular_survivor_matrix_before_recovered_output() {
    let codec =
        ReedSolomon::with_custom_matrix(2, 2, &[vec![1, 1], vec![1, 1]], CodecOptions::default())
            .unwrap();
    let expected = encoded(&codec, BLOCK_SIZE * 2 + 37);
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut shards = cursors(&expected, (1 << 0) | (1 << 1));
        // A failed decode plan must not preallocate complete missing outputs.
        for shard in &mut shards[..2] {
            *shard = Cursor::new(Vec::new());
        }
        let err = codec
            .reconstruct_stream(&mut shards, &options(mode))
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(Error::InvalidCustomMatrix)
        ));
        assert!(
            shards[..2]
                .iter()
                .all(|shard| { shard.get_ref().is_empty() && shard.get_ref().capacity() == 0 })
        );
        assert!(
            shards[2..]
                .iter()
                .zip(&expected[2..])
                .all(|(actual, expected)| { actual.get_ref() == expected })
        );
    }
}

#[cfg(feature = "benchmark-metrics")]
#[test]
fn stream_plan_requests_the_decode_matrix_once_per_stream() {
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let codec = ReedSolomon::new(4, 2).unwrap();
        let expected = encoded(&codec, BLOCK_SIZE * 7 + 37);
        let mut shards = cursors(&expected, (1 << 1) | (1 << 5));
        codec
            .reconstruct_stream(&mut shards, &options(mode))
            .unwrap();
        assert!(
            shards
                .iter()
                .zip(&expected)
                .all(|(actual, expected)| actual.get_ref() == expected)
        );
        let stats = codec.reconstruction_cache_stats();
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.inserts, 1);
        assert_eq!(stats.hits, 0);

        let mut repeated = cursors(&expected, (1 << 1) | (1 << 5));
        codec
            .reconstruct_stream(&mut repeated, &options(mode))
            .unwrap();
        let stats = codec.reconstruction_cache_stats();
        assert_eq!(stats.requests, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.inserts, 1);

        let mut parity_only = cursors(&expected, (1 << 4) | (1 << 5));
        codec
            .reconstruct_stream(&mut parity_only, &options(mode))
            .unwrap();
        let mut all_present = cursors(&expected, 0);
        codec
            .reconstruct_stream(&mut all_present, &options(mode))
            .unwrap();
        assert_eq!(codec.reconstruction_cache_stats().requests, 2);
    }
}

#[cfg(feature = "benchmark-metrics")]
#[test]
fn stream_plan_is_not_prepared_until_the_first_read_is_validated() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK_SIZE * 2 + 37);
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let mut shards = cursors(&expected, 1 << 1);
        shards[2].get_mut().truncate(BLOCK_SIZE - 1);
        codec
            .reconstruct_stream(&mut shards, &options(mode))
            .unwrap_err();
        assert_eq!(codec.reconstruction_cache_stats().requests, 0);
        assert!(shards[1].get_ref().is_empty());
    }
}
