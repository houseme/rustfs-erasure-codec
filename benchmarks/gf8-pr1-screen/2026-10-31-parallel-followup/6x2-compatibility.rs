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

#[cfg(rse_x86_avx2)]
mod dispatch {
    use rustfs_erasure_codec::{Field, galois_8};
    use std::cell::Cell;

    thread_local! {
        static BYTE_VIEWS: Cell<usize> = const { Cell::new(0) };
    }

    struct CountedByteField;

    impl Field for CountedByteField {
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
        fn as_gf8_slice(slice: &[u8]) -> Option<&[u8]> {
            BYTE_VIEWS.with(|calls| calls.set(calls.get() + 1));
            Some(slice)
        }
        fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
            BYTE_VIEWS.with(|calls| calls.set(calls.get() + 1));
            Some(slice)
        }
    }

    #[test]
    fn native_encoder_is_selected_only_at_and_above_threshold() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        const CHILD: &str = "RSE_AVX2_SIX_TWO_SIZE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "dispatch::native_encoder_is_selected_only_at_and_above_threshold",
                ])
                .env(CHILD, "1")
                .env("RSE_BACKEND_OVERRIDE", "auto")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        assert_eq!(galois_8::active_backend_id(), galois_8::BackendId::RustAvx2);
        let codec = rustfs_erasure_codec::ReedSolomon::<CountedByteField>::new(6, 2).unwrap();
        for (len, native) in [(4095, false), (4096, true), (4097, true)] {
            let data = vec![vec![0x5a; len]; 6];
            let mut parity = vec![vec![0; len]; 2];
            BYTE_VIEWS.with(|calls| calls.set(0));
            codec.encode_sep(&data, &mut parity).unwrap();
            assert_eq!(BYTE_VIEWS.with(Cell::get) > 2, native, "len={len}");
            let mut reference = vec![vec![0; len]; 2];
            galois_8::ReedSolomon::new(6, 2)
                .unwrap()
                .encode_sep(&data, &mut reference)
                .unwrap();
            assert_eq!(parity, reference);
        }
    }
}
