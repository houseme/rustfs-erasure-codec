#![cfg(feature = "std")]

use rustfs_erasure_codec::{CodecOptions, MatrixMode, galois_8::ReedSolomon};

#[test]
fn generated_encode_matches_incremental_reference_and_reconstructs() {
    for (data_count, parity_count) in [(4, 2), (10, 4), (12, 4)] {
        for mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
        ] {
            let codec = ReedSolomon::with_options(
                data_count,
                parity_count,
                CodecOptions::builder()
                    .matrix_mode(mode)
                    .max_parallel_jobs(4)
                    .build(),
            )
            .unwrap();
            for len in [256, 32767, 32768, 32769] {
                let data: Vec<Vec<u8>> = (0..data_count)
                    .map(|row| {
                        (0..len)
                            .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                            .collect()
                    })
                    .collect();
                let mut reference = vec![vec![0; len]; parity_count];
                for (index, shard) in data.iter().enumerate() {
                    codec
                        .encode_single_sep(index, shard, &mut reference)
                        .unwrap();
                }
                let mut serial = vec![vec![0xa5; len]; parity_count];
                codec.encode_sep(&data, &mut serial).unwrap();
                assert_eq!(
                    serial, reference,
                    "serial shape={data_count}+{parity_count} mode={mode:?} len={len}"
                );
                let mut parallel = vec![vec![0x5a; len]; parity_count];
                codec.encode_sep_par(&data, &mut parallel).unwrap();
                assert_eq!(
                    parallel, reference,
                    "parallel shape={data_count}+{parity_count} mode={mode:?} len={len}"
                );
                let mut shards: Vec<Option<Vec<u8>>> =
                    data.iter().chain(&reference).cloned().map(Some).collect();
                shards[data_count / 2] = None;
                codec.reconstruct_data_opt(&mut shards).unwrap();
                assert_eq!(
                    shards[data_count / 2].as_ref().unwrap(),
                    &data[data_count / 2]
                );
            }
        }
    }
}

#[cfg(all(rse_x86_avx512, not(rse_x86_avx2)))]
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
        if !(std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        const CHILD: &str = "RSE_AVX512_SIZE_DISPATCH_CHILD";
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
        assert_eq!(
            galois_8::active_backend_id(),
            galois_8::BackendId::RustAvx512
        );
        let codec = rustfs_erasure_codec::ReedSolomon::<CountedByteField>::new(4, 2).unwrap();
        for (len, native) in [(32767, false), (32768, true), (32769, true)] {
            let data = vec![vec![0x5a; len]; 4];
            let mut parity = vec![vec![0; len]; 2];
            BYTE_VIEWS.with(|calls| calls.set(0));
            codec.encode_sep(&data, &mut parity).unwrap();
            assert_eq!(BYTE_VIEWS.with(Cell::get) > 0, native, "len={len}");
            let mut reference = vec![vec![0; len]; 2];
            galois_8::ReedSolomon::new(4, 2)
                .unwrap()
                .encode_sep(&data, &mut reference)
                .unwrap();
            assert_eq!(parity, reference);
        }
    }
}
