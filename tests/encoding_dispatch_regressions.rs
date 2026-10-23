#![cfg(feature = "std")]

use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, Field, ReedSolomon, galois_8};

fn data_shards(count: usize, len: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|row| {
            (0..len)
                .map(|offset| (row * 41 + offset * 13) as u8)
                .collect()
        })
        .collect()
}

// Each failure is a safe Field implementation. Classic must finish preparation
// before any kernel write, then use ordinary field arithmetic on the original
// elements when a native view is missing or inconsistent.
struct InconsistentViews<const MODE: u8>;

impl<const MODE: u8> Field for InconsistentViews<MODE> {
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
        match MODE {
            1 if slice.len() > 16 => Some(&slice[..slice.len() - 1]),
            2 if slice.len() == 10 => None,
            4 if slice.len() == 65 => Some(&[0; 66]),
            5 if slice.len() > 16 => None,
            _ => Some(slice),
        }
    }

    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        if MODE == 3 && !slice.is_empty() {
            let len = slice.len();
            Some(&mut slice[..len - 1])
        } else {
            Some(slice)
        }
    }

    fn mul_slice(elem: u8, input: &[u8], out: &mut [u8]) {
        assert!(
            out.iter().all(|&byte| byte == 0xa5),
            "fallback started after an output was modified"
        );
        galois_8::Field::mul_slice(elem, input, out);
    }
    fn mul_slice_add(elem: u8, input: &[u8], out: &mut [u8]) {
        galois_8::Field::mul_slice_add(elem, input, out);
    }
}

fn check_native_view_fallback<const MODE: u8>(len: usize) {
    let options = CodecOptions::builder().max_parallel_jobs(2).build();
    let codec = ReedSolomon::<InconsistentViews<MODE>>::with_options(10, 4, options).unwrap();
    let reference = galois_8::ReedSolomon::new(10, 4).unwrap();
    let data = data_shards(10, len);
    let mut expected = vec![vec![0; len]; 4];
    reference.encode_sep(&data, &mut expected).unwrap();
    for parallel_entry in [false, true] {
        let mut actual = vec![vec![0xa5; len]; 4];
        if parallel_entry {
            codec.encode_sep_par(&data, &mut actual).unwrap();
        } else {
            codec.encode_sep(&data, &mut actual).unwrap();
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn failed_native_preparation_falls_back_before_any_parity_write() {
    for len in [65, 1024 * 1024 + 1] {
        check_native_view_fallback::<1>(len);
        check_native_view_fallback::<2>(len);
        check_native_view_fallback::<3>(len);
        check_native_view_fallback::<5>(len);
    }
    check_native_view_fallback::<4>(65);
}

#[test]
fn leopard_uses_the_same_checked_views_without_writing_on_error() {
    for codec_family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if codec_family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec = ReedSolomon::<InconsistentViews<3>>::with_options(
            10,
            4,
            CodecOptions::builder().codec_family(codec_family).build(),
        )
        .unwrap();
        let data = data_shards(10, 64);
        let mut parity = vec![vec![0xa5; 64]; 4];
        assert_eq!(
            codec.encode_sep(&data, &mut parity),
            Err(Error::IncorrectShardSize)
        );
        assert!(parity.iter().flatten().all(|&byte| byte == 0xa5));
        assert_eq!(
            codec.encode_sep_par(&data, &mut parity),
            Err(Error::IncorrectShardSize)
        );
        assert!(parity.iter().flatten().all(|&byte| byte == 0xa5));
    }
}

#[cfg(feature = "benchmark-metrics")]
#[test]
fn adaptive_encoding_entries_record_one_policy_decision() {
    let codec = galois_8::ReedSolomon::with_options(
        4,
        4,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let data = data_shards(4, 512 * 1024 + 1);
    let mut parity = vec![vec![0; data[0].len()]; 4];
    codec.reset_runtime_profile_stats();
    codec.encode_sep_par(&data, &mut parity).unwrap();
    assert_eq!(codec.runtime_profile_stats().parallel_policy_calls, 1);

    codec.reset_runtime_profile_stats();
    codec
        .encode_single_sep_opt(0, &data[0], &mut parity)
        .unwrap();
    assert_eq!(codec.runtime_profile_stats().parallel_policy_calls, 1);
    let expected = parity.clone();
    let mut shards = data.clone();
    shards.extend(vec![vec![0; data[0].len()]; 4]);
    codec.reset_runtime_profile_stats();
    codec.encode_single_opt(0, &mut shards).unwrap();
    assert_eq!(codec.runtime_profile_stats().parallel_policy_calls, 1);
    assert_eq!(&shards[4..], &expected);
}

#[cfg(feature = "benchmark-metrics")]
#[test]
fn adaptive_verification_entries_record_one_policy_decision() {
    let codec = galois_8::ReedSolomon::with_options(
        4,
        4,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let mut shards = data_shards(8, 512 * 1024 + 1);
    codec.encode(&mut shards).unwrap();
    let mut buffer = vec![vec![0; shards[0].len()]; 4];
    let mut workspace = rustfs_erasure_codec::VerifyWorkspace::new(&codec, shards[0].len());

    codec.reset_runtime_profile_stats();
    assert!(codec.verify_opt(&shards).unwrap());
    assert_eq!(codec.runtime_profile_stats().parallel_policy_calls, 1);

    codec.reset_runtime_profile_stats();
    assert!(codec.verify_with_buffer_opt(&shards, &mut buffer).unwrap());
    assert_eq!(codec.runtime_profile_stats().parallel_policy_calls, 1);

    codec.reset_runtime_profile_stats();
    assert!(
        codec
            .verify_with_workspace_opt(&shards, &mut workspace)
            .unwrap()
    );
    assert_eq!(codec.runtime_profile_stats().parallel_policy_calls, 1);
}

#[test]
fn serial_optimized_verification_preserves_parity_and_detects_corruption() {
    for (data, parity) in [(4, 2), (6, 3), (8, 3), (8, 4), (10, 4), (12, 4)] {
        for len in [65, 65_536] {
            let codec = galois_8::ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder().max_parallel_jobs(1).build(),
            )
            .unwrap();
            let mut shards = data_shards(data + parity, len);
            codec.encode(&mut shards).unwrap();
            let expected = shards[data..].to_vec();
            let mut buffer = vec![vec![0xa5; len]; parity];
            let mut workspace = rustfs_erasure_codec::VerifyWorkspace::new(&codec, 1);

            for corrupt in [false, true] {
                if corrupt {
                    shards[data + parity - 1][len - 1] ^= 0x73;
                }
                let reference = codec.verify(&shards).unwrap();
                assert_eq!(reference, !corrupt);
                assert_eq!(codec.verify_opt(&shards).unwrap(), reference);
                assert_eq!(
                    codec.verify_with_buffer_opt(&shards, &mut buffer).unwrap(),
                    reference
                );
                assert_eq!(buffer, expected);
                assert_eq!(
                    codec
                        .verify_with_workspace_opt(&shards, &mut workspace)
                        .unwrap(),
                    reference
                );
                assert_eq!(workspace.shard_len(), Some(len));
            }
        }
    }
}

#[cfg(any(rse_aarch64_neon, rse_x86_avx2))]
mod native {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static VIEW_CALLS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    }

    struct CountedViews;
    impl Field for CountedViews {
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
            VIEW_CALLS.with(|calls| {
                let (read, write) = calls.get();
                calls.set((read + 1, write));
            });
            Some(slice)
        }
        fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
            VIEW_CALLS.with(|calls| {
                let (read, write) = calls.get();
                calls.set((read, write + 1));
            });
            Some(slice)
        }
    }

    #[test]
    fn native_views_are_prepared_once_for_serial_and_parallel_entries() {
        #[cfg(rse_x86_avx2)]
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        const CHILD: &str = "RSE_ENCODING_PREPARATION_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "native::native_views_are_prepared_once_for_serial_and_parallel_entries",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("RSE_BACKEND_OVERRIDE", "auto")
                .env("RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES", "1")
                .env("RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB", "65536")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let codec = ReedSolomon::<CountedViews>::with_options(
            10,
            4,
            CodecOptions::builder().max_parallel_jobs(2).build(),
        )
        .unwrap();
        for parallel_entry in [false, true] {
            // x86 keeps the existing generic parallel route. The serial-fallback
            // decision still exercises the same single preparation on that arch.
            let len = if cfg!(rse_x86_avx2) && parallel_entry {
                65
            } else {
                1024 * 1024 + 1
            };
            let entry_codec = if cfg!(rse_x86_avx2) && parallel_entry {
                ReedSolomon::<CountedViews>::with_options(
                    10,
                    4,
                    CodecOptions::builder().max_parallel_jobs(1).build(),
                )
                .unwrap()
            } else {
                codec.clone()
            };
            let data = data_shards(10, len);
            let mut parity = vec![vec![0; len]; 4];
            VIEW_CALLS.with(|calls| calls.set((0, 0)));
            if parallel_entry {
                entry_codec.encode_sep_par(&data, &mut parity).unwrap();
            } else {
                entry_codec.encode_sep(&data, &mut parity).unwrap();
            }
            // One empty capability probe, ten inputs, four coefficient rows;
            // one mutable empty probe and four mutable parity views.
            assert_eq!(VIEW_CALLS.with(Cell::get), (15, 5));
            let mut expected = vec![vec![0; len]; 4];
            galois_8::ReedSolomon::new(10, 4)
                .unwrap()
                .encode_sep(&data, &mut expected)
                .unwrap();
            assert_eq!(parity, expected);
        }
    }
}
