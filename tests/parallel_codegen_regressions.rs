#![cfg(feature = "std")]

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecOptions, Field, MatrixMode, galois_8};

fn data_shards(count: usize, len: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|row| {
            (0..len)
                .map(|offset| (row * 97 + offset * 11) as u8)
                .collect()
        })
        .collect()
}

fn reference_parity(codec: &ReedSolomon, data: &[Vec<u8>], parity: usize) -> Vec<Vec<u8>> {
    let mut expected = vec![vec![0; data[0].len()]; parity];
    // The incremental API uses the multiplication backend rather than the
    // generated whole-codeword kernel under test.
    for (index, shard) in data.iter().enumerate() {
        codec
            .encode_single_sep(index, shard, &mut expected)
            .unwrap();
    }
    expected
}

#[test]
fn supported_and_fallback_shapes_match_incremental_reference_for_unaligned_slices_and_tails() {
    for (data_count, parity_count) in [(10, 4), (12, 4), (8, 3), (8, 4), (6, 2), (6, 3), (4, 2)] {
        let codec = ReedSolomon::with_options(
            data_count,
            parity_count,
            CodecOptions::builder().max_parallel_jobs(4).build(),
        )
        .unwrap();
        let lengths = if (data_count, parity_count) == (10, 4) {
            [1024 * 1024 + 1, 2 * 1024 * 1024 + 63]
        } else {
            [256 * 1024 + 1, 512 * 1024 + 63]
        };
        for len in lengths {
            let storage = data_shards(data_count, len + 1);
            let data: Vec<_> = storage.iter().map(|shard| &shard[1..]).collect();
            let expected = reference_parity(
                &codec,
                &data.iter().map(|slice| slice.to_vec()).collect::<Vec<_>>(),
                parity_count,
            );
            let mut output_storage = vec![vec![0; len + 1]; parity_count];
            let mut output: Vec<_> = output_storage
                .iter_mut()
                .map(|shard| &mut shard[1..])
                .collect();
            codec.encode_sep_par(&data, &mut output).unwrap();
            assert!(
                output
                    .iter()
                    .zip(&expected)
                    .all(|(actual, expected)| *actual == expected.as_slice()),
                "{data_count}+{parity_count}, len {len}"
            );
            assert!(output_storage.iter().all(|shard| shard[0] == 0));
        }
    }
}

#[test]
fn parallel_codegen_uses_each_matrix_and_custom_coefficients() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::with_options(
            10,
            4,
            CodecOptions::builder()
                .matrix_mode(mode)
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        let data = data_shards(10, 1024 * 1024 + 31);
        let expected = reference_parity(&codec, &data, 4);
        let mut actual = vec![vec![0; data[0].len()]; 4];
        codec.encode_sep_par(&data, &mut actual).unwrap();
        assert_eq!(actual, expected, "{mode:?}");
    }

    let source = ReedSolomon::new(10, 4).unwrap();
    let basis: Vec<Vec<u8>> = (0..10)
        .map(|row| (0..10).map(|column| u8::from(row == column)).collect())
        .collect();
    let mut custom_rows = reference_parity(&source, &basis, 4);
    custom_rows.reverse();
    let custom = ReedSolomon::with_custom_matrix(
        10,
        4,
        &custom_rows,
        CodecOptions::builder().max_parallel_jobs(4).build(),
    )
    .unwrap();
    let data = data_shards(10, 1024 * 1024 + 15);
    let expected = reference_parity(&custom, &data, 4);
    let mut actual = vec![vec![0; data[0].len()]; 4];
    custom.encode_sep_par(&data, &mut actual).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn parallel_codegen_handles_job_budgets_and_unsupported_shapes() {
    for (data_count, parity_count) in [(10, 4), (10, 5)] {
        let data = data_shards(data_count, 1024 * 1024 + 17);
        let source = ReedSolomon::new(data_count, parity_count).unwrap();
        let expected = reference_parity(&source, &data, parity_count);
        for budget in [1, 2, 3, 4, 32, 33] {
            let codec = ReedSolomon::with_options(
                data_count,
                parity_count,
                CodecOptions::builder().max_parallel_jobs(budget).build(),
            )
            .unwrap();
            let mut actual = vec![vec![0; data[0].len()]; parity_count];
            codec.encode_sep_par(&data, &mut actual).unwrap();
            assert_eq!(
                actual, expected,
                "{data_count}+{parity_count}, budget {budget}"
            );
            assert!(codec.parallel_policy(data[0].len(), parity_count).jobs <= budget);
        }
    }
}

static MULTIPLICATION_CALLS: AtomicUsize = AtomicUsize::new(0);

// A safe native-byte representation does not imply that the field marker is
// Send or Sync. Parallel encoding must only share validated byte slices.
#[derive(Debug)]
struct LocalNativeField(PhantomData<Rc<()>>);

impl Field for LocalNativeField {
    const ORDER: usize = 256;
    type Elem = u8;
    fn add(a: u8, b: u8) -> u8 {
        galois_8::Field::add(a, b)
    }
    fn mul(a: u8, b: u8) -> u8 {
        galois_8::Field::mul(a, b)
    }
    fn div(a: u8, b: u8) -> u8 {
        galois_8::Field::div(a, b)
    }
    fn exp(a: u8, n: usize) -> u8 {
        galois_8::Field::exp(a, n)
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
        Some(slice)
    }
    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        Some(slice)
    }
    fn mul_slice(elem: u8, input: &[u8], out: &mut [u8]) {
        MULTIPLICATION_CALLS.fetch_add(1, Ordering::Relaxed);
        galois_8::Field::mul_slice(elem, input, out);
    }
    fn mul_slice_add(elem: u8, input: &[u8], out: &mut [u8]) {
        MULTIPLICATION_CALLS.fetch_add(1, Ordering::Relaxed);
        galois_8::Field::mul_slice_add(elem, input, out);
    }
}

#[test]
fn native_field_marker_does_not_need_sync() {
    let codec = rustfs_erasure_codec::ReedSolomon::<LocalNativeField>::with_options(
        10,
        4,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let data = data_shards(10, 1024 * 1024 + 17);
    let expected = reference_parity(&ReedSolomon::new(10, 4).unwrap(), &data, 4);
    let mut actual = vec![vec![0; data[0].len()]; 4];
    codec.encode_sep_par(&data, &mut actual).unwrap();
    assert_eq!(actual, expected);
}

#[cfg(all(feature = "std", rse_aarch64_neon))]
#[test]
fn native_marker_uses_generated_parallel_dispatch() {
    const CHILD: &str = "RSE_PARALLEL_CODEGEN_AUTO_CHILD";
    // The public policy deliberately chooses serial execution on single-core
    // hosts. The direct helper tests cover synthetic budgets independently.
    if std::thread::available_parallelism().unwrap().get() < 2 {
        return;
    }
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_marker_uses_generated_parallel_dispatch",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("RSE_BACKEND_OVERRIDE", "auto")
            .env("RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES", "1")
            .env("RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB", "16384")
            .env("RS_PARALLEL_POLICY_L2_CACHE_BYTES", "262144")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let codec = rustfs_erasure_codec::ReedSolomon::<LocalNativeField>::with_options(
        10,
        4,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let data = data_shards(10, 1024 * 1024 + 17);
    let expected = reference_parity(&ReedSolomon::new(10, 4).unwrap(), &data, 4);
    let decision = codec.parallel_policy(data[0].len(), 4);
    assert!(decision.use_parallel);
    assert_eq!(decision.jobs, 2);
    let mut actual = vec![vec![0; data[0].len()]; 4];
    MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
    codec.encode_sep_par(&data, &mut actual).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        MULTIPLICATION_CALLS.load(Ordering::Relaxed),
        0,
        "eligible native encoding unexpectedly fell back to generic multiplication"
    );
}

#[test]
fn explicit_backend_overrides_keep_the_generic_path() {
    const CHILD: &str = "RSE_PARALLEL_CODEGEN_OVERRIDE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let codec = rustfs_erasure_codec::ReedSolomon::<LocalNativeField>::with_options(
            10,
            4,
            CodecOptions::builder().max_parallel_jobs(2).build(),
        )
        .unwrap();
        let data = data_shards(10, 1024 * 1024 + 31);
        let expected = reference_parity(&ReedSolomon::new(10, 4).unwrap(), &data, 4);
        let mut actual = vec![vec![0; data[0].len()]; 4];
        MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
        codec.encode_sep_par(&data, &mut actual).unwrap();
        assert_eq!(actual, expected);
        assert!(MULTIPLICATION_CALLS.load(Ordering::Relaxed) > 0);
        if std::env::var("RSE_BACKEND_OVERRIDE").unwrap() == "scalar" {
            assert_eq!(galois_8::active_backend_name(), "scalar-rust");
        }
        return;
    }
    for backend in ["scalar", "rust-neon"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "explicit_backend_overrides_keep_the_generic_path",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("RSE_BACKEND_OVERRIDE", backend)
            .status()
            .unwrap();
        assert!(status.success(), "override {backend}");
    }
}
