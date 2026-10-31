#![cfg(feature = "std")]

use rustfs_erasure_codec::{CodecOptions, MatrixMode, galois_8::ReedSolomon};

fn assert_compatible(codec: &ReedSolomon) {
    for len in [1, 63, 4095, 4096, 4097, 32768] {
        let storage: Vec<Vec<u8>> = (0..6)
            .map(|row| {
                (0..len + 2)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        let data: Vec<&[u8]> = storage.iter().map(|shard| &shard[1..1 + len]).collect();
        let mut expected = vec![vec![0; len]; 2];
        for (index, input) in data.iter().enumerate() {
            codec
                .encode_single_sep(index, input, &mut expected)
                .unwrap();
        }
        for parallel in [false, true] {
            let mut parity = vec![vec![0xa5; len + 2]; 2];
            let mut outputs: Vec<_> = parity
                .iter_mut()
                .map(|shard| &mut shard[1..1 + len])
                .collect();
            if parallel {
                codec.encode_sep_par(&data, &mut outputs).unwrap();
            } else {
                codec.encode_sep(&data, &mut outputs).unwrap();
            }
            for (actual, reference) in parity.iter().zip(&expected) {
                assert_eq!(actual[0], 0xa5);
                assert_eq!(actual[len + 1], 0xa5);
                assert_eq!(&actual[1..1 + len], reference);
            }
        }
        let mut shards: Vec<Option<Vec<u8>>> = data
            .iter()
            .map(|s| Some(s.to_vec()))
            .chain(expected.iter().cloned().map(Some))
            .collect();
        shards[3] = None;
        codec.reconstruct_data_opt(&mut shards).unwrap();
        assert_eq!(shards[3].as_deref(), Some(data[3]));
    }
}

#[test]
fn six_plus_two_matches_reference_for_builtin_and_custom_matrices() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::with_options(
            6,
            2,
            CodecOptions::builder()
                .matrix_mode(mode)
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        assert_compatible(&codec);
    }
    let basis: Vec<Vec<u8>> = (0..6)
        .map(|row| (0..6).map(|column| u8::from(row == column)).collect())
        .collect();
    let mut rows = vec![vec![0; 6]; 2];
    ReedSolomon::new(6, 2)
        .unwrap()
        .encode_sep(&basis, &mut rows)
        .unwrap();
    rows.reverse();
    let codec = ReedSolomon::with_custom_matrix(
        6,
        2,
        &rows,
        CodecOptions::builder().max_parallel_jobs(4).build(),
    )
    .unwrap();
    assert_compatible(&codec);
}
