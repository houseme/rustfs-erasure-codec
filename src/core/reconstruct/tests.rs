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
fn systematic_decode_matches_full_inverse_for_all_small_gf8_survivors() {
    all_small_survivors::<galois_8::Field>();
}

#[test]
fn systematic_decode_matches_full_inverse_for_all_small_gf16_survivors() {
    all_small_survivors::<galois_16::Field>();
}

#[test]
fn systematic_decode_matches_full_inverse_across_large_erasure_blocks() {
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
    // Two known data rows and two identical parity equations: the reduced
    // 2x2 system is singular. No missing data or parity may be initialized.
    let mut shards = vec![
        None,
        None,
        Some(vec![1; 33]),
        Some(vec![2; 33]),
        None,
        Some(vec![3; 33]),
        Some(vec![4; 33]),
        None,
    ];
    let before = shards.clone();
    assert_eq!(
        codec.reconstruct(&mut shards),
        Err(Error::InvalidCustomMatrix)
    );
    assert_eq!(shards, before, "reduced singular solve must not publish");
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

#[test]
fn systematic_decode_retains_general_tower_pivot_swaps_and_singular_errors() {
    let codec = galois_16::ReedSolomon::with_custom_matrix(
        4,
        4,
        &[
            vec![[0, 0], [0, 1], [1, 3], [2, 5]],
            vec![[0, 1], [0, 0], [3, 7], [4, 9]],
            vec![[0, 1], [0, 0], [3, 7], [4, 9]],
            vec![[0, 0]; 4],
        ],
        CodecOptions::builder().inversion_cache(false).build(),
    )
    .unwrap();
    for mask in 0u16..256 {
        if mask.count_ones() == 4 {
            let valid: Vec<_> = (0..8).filter(|&i| mask & (1 << i) != 0).collect();
            matches_full_inverse(&codec, &valid);
        }
    }
}

#[cfg(feature = "std")]
mod reduced_work {
    use super::*;
    use std::cell::Cell;

    std::thread_local! {
        static PRODUCTS: Cell<usize> = const { Cell::new(0) };
    }

    struct CountedField<const BYTE_ROWS: bool>;
    impl<const BYTE_ROWS: bool> Field for CountedField<BYTE_ROWS> {
        const ORDER: usize = 256;
        type Elem = u8;
        fn as_gf8_slice(input: &[u8]) -> Option<&[u8]> {
            BYTE_ROWS.then_some(input)
        }
        fn as_gf8_slice_mut(input: &mut [u8]) -> Option<&mut [u8]> {
            BYTE_ROWS.then_some(input)
        }
        fn add(a: u8, b: u8) -> u8 {
            galois_8::add(a, b)
        }
        fn mul(a: u8, b: u8) -> u8 {
            PRODUCTS.with(|count| count.set(count.get() + 1));
            galois_8::mul(a, b)
        }
        fn div(a: u8, b: u8) -> u8 {
            galois_8::div(a, b)
        }
        fn exp(a: u8, n: usize) -> u8 {
            galois_8::exp(a, n)
        }
        fn zero() -> u8 {
            0
        }
        fn one() -> u8 {
            1
        }
        fn nth_internal(n: usize) -> u8 {
            n as u8
        }
        fn mul_slice_add(scale: u8, input: &[u8], output: &mut [u8]) {
            PRODUCTS.with(|count| count.set(count.get() + input.len()));
            <galois_8::Field as Field>::mul_slice_add(scale, input, output);
        }
    }

    #[test]
    fn two_missing_data_eliminates_only_unknown_equations() {
        fn check<const BYTE_ROWS: bool>() {
            for start in [0, 7, 16, 30] {
                let codec = crate::ReedSolomon::<CountedField<BYTE_ROWS>>::new(32, 16).unwrap();
                let valid: Vec<_> = (0..48)
                    .filter(|&index| !(start..start + 2).contains(&index))
                    .take(32)
                    .collect();
                PRODUCTS.with(|count| count.set(0));
                let actual = codec.build_data_decode_matrix(&valid).unwrap();
                let products = PRODUCTS.with(Cell::get);
                // A 2-row solve, independent of where the missing data lie.
                // Count vector element products too; a SIMD call cannot hide
                // elimination of the 30 known systematic equations.
                assert!(products <= 4 * 2 * 2 * (32 + 2), "{products} products");
                let reference = galois_8::ReedSolomon::new(32, 16).unwrap();
                let mut selected = crate::matrix::Matrix::<CountedField<BYTE_ROWS>>::new(32, 32);
                for (row, &index) in valid.iter().enumerate() {
                    for column in 0..32 {
                        selected.set(
                            row,
                            column,
                            reference.encoding_format().matrix().get(index, column),
                        );
                    }
                }
                PRODUCTS.with(|count| count.set(0));
                let expected = selected.invert().unwrap();
                let dense_products = PRODUCTS.with(Cell::get);
                assert!(products < dense_products);
                println!(
                    "systematic decode byte_rows={BYTE_ROWS} missing_start={start} products={products} dense_products={dense_products}"
                );
                for row in 0..32 {
                    assert_eq!(actual.get_row(row), expected.get_row(row));
                }
            }
        }
        check::<false>();
        check::<true>();
    }
}
