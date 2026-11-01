#![cfg(not(feature = "std"))]

use rustfs_erasure_codec::{CodecOptions, Error, Field, MatrixMode, ReedSolomon};

fn check<F: Field>() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ] {
        let options = CodecOptions::builder().matrix_mode(mode).build();
        let codec = if mode == MatrixMode::Custom {
            let reference = ReedSolomon::<F>::new(4, 2).unwrap();
            let basis: Vec<Vec<F::Elem>> = (0..4)
                .map(|row| {
                    (0..4)
                        .map(|column| if row == column { F::one() } else { F::zero() })
                        .collect()
                })
                .collect();
            let mut rows = vec![vec![F::zero(); 4]; 2];
            reference.encode_sep(&basis, &mut rows).unwrap();
            rows.reverse();
            ReedSolomon::<F>::with_custom_matrix(4, 2, &rows, options).unwrap()
        } else {
            ReedSolomon::<F>::with_options(4, 2, options).unwrap()
        };
        for len in [2048, 2049, 4096, 4097] {
            let mut shards: Vec<Vec<F::Elem>> = (0..6)
                .map(|row| {
                    (0..len)
                        .map(|offset| F::nth((row * 73 + offset * 19 + 1) % F::ORDER))
                        .collect()
                })
                .collect();
            codec.encode(&mut shards).unwrap();
            let original = shards.clone();
            let mut scratch = vec![vec![F::zero(); len]; 2];
            assert!(codec.verify(&shards).unwrap());
            assert!(codec.verify_with_buffer(&shards, &mut scratch).unwrap());
            for row in [0, 4, 5] {
                for offset in [0, len - 1] {
                    shards[row][offset] = F::add(shards[row][offset], F::one());
                    assert!(
                        !codec.verify(&shards).unwrap(),
                        "mode={mode:?}, len={len}, row={row}"
                    );
                    assert!(!codec.verify_with_buffer(&shards, &mut scratch).unwrap());
                    shards[row][offset] = F::add(shards[row][offset], F::one());
                }
            }
            assert_eq!(shards, original);
            shards[4][0] = F::add(shards[4][0], F::one());
            shards[5].pop();
            let before = shards.clone();
            assert_eq!(codec.verify(&shards), Err(Error::IncorrectShardSize));
            assert_eq!(shards, before);
        }
    }
}

#[test]
fn classic_small_verification_preserves_format_and_validation_without_std() {
    check::<rustfs_erasure_codec::galois_8::Field>();
    check::<rustfs_erasure_codec::galois_16::Field>();
}
