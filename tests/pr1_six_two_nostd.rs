#![cfg(all(feature = "experimental-pr1-kernels", not(feature = "std")))]

use rustfs_erasure_codec::{CodecOptions, Field, MatrixMode, ReedSolomon, galois_8};

struct GenericGf8;
impl Field for GenericGf8 {
    const ORDER: usize = 256;
    type Elem = u8;
    fn add(a: u8, b: u8) -> u8 {
        galois_8::add(a, b)
    }
    fn mul(a: u8, b: u8) -> u8 {
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
}

fn check<F: Field<Elem = u8>>() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ] {
        let options = CodecOptions::builder().matrix_mode(mode).build();
        let codec = if mode == MatrixMode::Custom {
            let reference = ReedSolomon::<F>::new(6, 2).unwrap();
            let mut rows: Vec<_> = (6..8)
                .map(|row| {
                    reference
                        .encoding_format()
                        .matrix_row(row)
                        .unwrap()
                        .to_vec()
                })
                .collect();
            rows.reverse();
            ReedSolomon::<F>::with_custom_matrix(6, 2, &rows, options).unwrap()
        } else {
            ReedSolomon::<F>::with_options(6, 2, options).unwrap()
        };
        for len in [4095, 4096, 4097] {
            let data: Vec<Vec<u8>> = (0..6)
                .map(|row| {
                    (0..len)
                        .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                        .collect()
                })
                .collect();
            let mut expected = vec![vec![0; len]; 2];
            for (index, input) in data.iter().enumerate() {
                codec
                    .encode_single_sep(index, input, &mut expected)
                    .unwrap();
            }
            let mut parity = vec![vec![0; len]; 2];
            codec.encode_sep(&data, &mut parity).unwrap();
            assert_eq!(parity, expected, "mode={mode:?}, len={len}");
            let mut shards: Vec<_> = data
                .iter()
                .cloned()
                .map(Some)
                .chain(parity.into_iter().map(Some))
                .collect();
            shards[3] = None;
            codec.reconstruct_data(&mut shards).unwrap();
            assert_eq!(shards[3].as_deref(), Some(data[3].as_slice()));
        }
    }
}

#[test]
fn six_plus_two_matches_reference_and_generic_field_without_std() {
    check::<galois_8::Field>();
    check::<GenericGf8>();
}
