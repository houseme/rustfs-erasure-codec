use super::*;
use crate::{CodecOptions, MatrixMode, galois_8, galois_16};

fn matches_full_inverse<F: Field>(codec: &crate::ReedSolomon<F>, valid: &[usize]) {
    let n = codec.data_shard_count();
    let mut selected = crate::matrix::Matrix::<F>::new(n, n);
    for (row, &index) in valid.iter().enumerate() {
        for col in 0..n {
            selected.set(row, col, codec.encoding_format().matrix().get(index, col));
        }
    }
    let expected = selected.invert().map_err(|_| Error::InvalidCustomMatrix);
    match (codec.build_data_decode_matrix(valid), expected) {
        (Ok(actual), Ok(expected)) => {
            for row in 0..n {
                assert_eq!(
                    actual.get_row(row),
                    expected.get_row(row),
                    "survivors {valid:?}"
                );
            }
        }
        (Err(actual), Err(expected)) => assert_eq!(actual, expected),
        _ => panic!("different singularity result for survivors {valid:?}"),
    }
}

fn all_small_survivors<F: Field>() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = crate::ReedSolomon::<F>::with_options(
            4,
            4,
            CodecOptions::builder()
                .matrix_mode(mode)
                .inversion_cache(false)
                .build(),
        )
        .unwrap();
        for mask in 0u16..256 {
            if mask.count_ones() < 4 {
                continue;
            }
            let valid: Vec<_> = (0..8).filter(|&i| mask & (1 << i) != 0).take(4).collect();
            matches_full_inverse(&codec, &valid);
        }
    }
}

#[test]
fn single_data_decode_matches_full_inverse_for_all_small_gf8_survivors() {
    all_small_survivors::<galois_8::Field>();
}

#[test]
fn single_data_decode_matches_full_inverse_for_all_small_gf16_survivors() {
    all_small_survivors::<galois_16::Field>();
}

#[test]
fn single_data_decode_matches_full_inverse_across_large_erasure_blocks() {
    fn check<F: Field>() {
        for mode in [MatrixMode::Vandermonde, MatrixMode::Cauchy] {
            let codec = crate::ReedSolomon::<F>::with_options(
                32,
                16,
                CodecOptions::builder()
                    .matrix_mode(mode)
                    .inversion_cache(false)
                    .build(),
            )
            .unwrap();
            for count in 1..=16 {
                for start in [0, 7, 16] {
                    let valid: Vec<_> = (0..48)
                        .filter(|&i| !(start..start + count).contains(&i))
                        .take(32)
                        .collect();
                    matches_full_inverse(&codec, &valid);
                }
            }
        }
    }
    check::<galois_8::Field>();
    check::<galois_16::Field>();
}

#[test]
fn custom_decode_retains_pivot_swaps_and_singular_errors() {
    let codec = galois_8::ReedSolomon::with_custom_matrix(
        4,
        4,
        &[
            vec![0, 1, 3, 5],
            vec![1, 0, 7, 9],
            vec![1, 0, 7, 9],
            vec![0, 0, 0, 0],
        ],
        CodecOptions::builder().inversion_cache(false).build(),
    )
    .unwrap();
    for mask in 0u16..256 {
        if mask.count_ones() != 4 {
            continue;
        }
        let valid: Vec<_> = (0..8).filter(|&i| mask & (1 << i) != 0).collect();
        matches_full_inverse(&codec, &valid);
    }
    let mut shards = vec![
        None,
        Some(vec![1; 33]),
        Some(vec![2; 33]),
        Some(vec![3; 33]),
        None,
        None,
        None,
        Some(vec![0; 33]),
    ];
    let before = shards.clone();
    assert_eq!(
        codec.reconstruct(&mut shards),
        Err(Error::InvalidCustomMatrix)
    );
    assert_eq!(
        shards, before,
        "singular recovery must not publish a destination"
    );
}

#[test]
fn single_data_gf16_decode_retains_general_tower_coefficients() {
    for mode in [MatrixMode::Vandermonde, MatrixMode::Cauchy] {
        let codec = galois_16::ReedSolomon::with_options(
            2,
            255,
            CodecOptions::builder()
                .matrix_mode(mode)
                .inversion_cache(false)
                .build(),
        )
        .unwrap();
        for valid in [[0, 256], [1, 256], [255, 256]] {
            matches_full_inverse(&codec, &valid);
        }
    }
}
