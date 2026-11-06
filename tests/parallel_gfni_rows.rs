#![cfg(all(feature = "std", feature = "experimental-parallel-multi-output"))]

use std::sync::atomic::{AtomicUsize, Ordering};

use rustfs_erasure_codec::{CodecOptions, Field, MatrixMode, ReedSolomon, galois_8};

static MULTIPLICATION_CALLS: AtomicUsize = AtomicUsize::new(0);
static OUTPUT_VIEWS: AtomicUsize = AtomicUsize::new(0);

// MODE 0 supplies valid byte views. Modes 1–3 reject an input, coefficient,
// or late output view. MODE 4 supplies valid views and observes fallback writes
// after the caller's AsMut implementation changes its reported length.
struct ObservedField<const MODE: u8>;

impl<const MODE: u8> Field for ObservedField<MODE> {
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
            1 if slice.len() > 32 => Some(&slice[..slice.len() - 1]),
            2 if slice.len() == 10 => None,
            _ => Some(slice),
        }
    }

    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        if MODE == 3 && !slice.is_empty() && OUTPUT_VIEWS.fetch_add(1, Ordering::Relaxed) == 3 {
            let len = slice.len();
            return Some(&mut slice[..len - 1]);
        }
        Some(slice)
    }

    fn mul_slice(value: u8, input: &[u8], output: &mut [u8]) {
        if MODE != 0 {
            assert!(
                output.iter().all(|&byte| byte == 0xa5),
                "fallback observed a partially written output"
            );
        }
        MULTIPLICATION_CALLS.fetch_add(1, Ordering::Relaxed);
        galois_8::Field::mul_slice(value, input, output);
    }

    fn mul_slice_add(value: u8, input: &[u8], output: &mut [u8]) {
        MULTIPLICATION_CALLS.fetch_add(1, Ordering::Relaxed);
        galois_8::Field::mul_slice_add(value, input, output);
    }
}

fn inputs(count: usize, len: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|row| {
            (0..len)
                .map(|offset| (row * 71 + offset * 19 + 11) as u8)
                .collect()
        })
        .collect()
}

fn reference(codec: &galois_8::ReedSolomon, inputs: &[&[u8]], outputs: usize) -> Vec<Vec<u8>> {
    let mut parity = vec![vec![0; inputs[0].len()]; outputs];
    for (index, input) in inputs.iter().enumerate() {
        codec.encode_single_sep(index, input, &mut parity).unwrap();
    }
    parity
}

fn is_gfni_backend() -> bool {
    matches!(
        galois_8::active_backend_id(),
        galois_8::BackendId::RustGfniAvx2 | galois_8::BackendId::RustGfniAvx512
    )
}

#[test]
fn gfni_parallel_rows_use_native_dispatch_and_preserve_contracts() {
    const CHILD: &str = "RSE_PARALLEL_GFNI_ROWS_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Each backend is resolved once per process. Child processes also isolate
        // the observing Field counters from unrelated parallel tests.
        for backend in [
            "unset",
            "auto",
            "rust-gfni-avx2",
            "rust-gfni-avx512",
            "scalar",
            "rust-avx2",
            "rust-avx512",
            "rust-neon",
            "bogus",
            #[cfg(unix)]
            "non-unicode",
        ] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "gfni_parallel_rows_use_native_dispatch_and_preserve_contracts",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("RSE_BACKEND_OVERRIDE", backend)
                .env("RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES", "1")
                .env("RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB", "16384")
                .env("RS_PARALLEL_POLICY_L2_CACHE_BYTES", "262144");
            if backend == "unset" {
                child.env_remove("RSE_BACKEND_OVERRIDE");
            }
            #[cfg(unix)]
            if backend == "non-unicode" {
                use std::os::unix::ffi::OsStringExt;
                child.env(
                    "RSE_BACKEND_OVERRIDE",
                    std::ffi::OsString::from_vec(vec![0xff]),
                );
            }
            let status = child.status().unwrap();
            assert!(status.success(), "backend {backend}");
        }
        return;
    }
    if std::thread::available_parallelism().unwrap().get() < 2 {
        println!("SKIP GFNI parallel rows: fewer than two available CPUs");
        return;
    }
    let requested = match std::env::var("RSE_BACKEND_OVERRIDE") {
        Ok(value) => value,
        Err(std::env::VarError::NotUnicode(_)) => "non-unicode".to_owned(),
        Err(std::env::VarError::NotPresent) => "auto".to_owned(),
    };
    if !is_gfni_backend() {
        println!(
            "SKIP GFNI dispatch: requested={requested}, active={:?}",
            galois_8::active_backend_id()
        );
        if matches!(requested.as_str(), "auto" | "bogus" | "non-unicode") {
            return;
        }
        // Explicit non-GFNI or unavailable ISA overrides continue to exercise
        // their own arithmetic backend, without invoking the GFNI row adapter.
        check_fallback::<0>(10, 4, 256 * 1024 + 1, 2);
        return;
    }
    let requested_matches_backend = matches!(
        (requested.as_str(), galois_8::active_backend_id()),
        ("auto", _)
            | ("rust-gfni-avx2", galois_8::BackendId::RustGfniAvx2)
            | ("rust-gfni-avx512", galois_8::BackendId::RustGfniAvx512)
    );
    if !requested_matches_backend {
        check_fallback::<0>(10, 4, 256 * 1024 + 1, 2);
        println!(
            "GFNI_OVERRIDE_GENERIC requested={requested}, active={:?}",
            galois_8::active_backend_id()
        );
        return;
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        for (data, parity) in [(4, 2), (6, 3), (8, 3), (8, 4), (10, 4), (16, 4), (32, 4)] {
            for len in [256 * 1024 + 1, 1024 * 1024 + 63] {
                check_native(data, parity, len);
            }
        }
        // A custom coefficient matrix covers zero, unit and general products.
        check_custom_matrix();
        for (data, parity, len, jobs) in [
            (3, 2, 262145, 2),
            (33, 4, 262145, 2),
            (10, 1, 262145, 2),
            (10, 5, 262145, 2),
            (10, 4, 65599, 2),
        ] {
            check_fallback::<0>(data, parity, len, jobs);
        }
        check_fallback::<1>(10, 4, 262145, 2);
        check_fallback::<2>(10, 4, 262145, 2);
        check_fallback::<3>(10, 4, 262145, 2);
        check_changing_output_view();
    });
    println!(
        "GFNI_PARALLEL_ROWS backend={:?} completed native, fallback, matrix, tail and job-budget cases",
        galois_8::active_backend_id()
    );
}

fn check_native(data: usize, parity: usize, len: usize) {
    let storage = inputs(data, len + 1);
    let input_views: Vec<_> = storage.iter().map(|input| &input[1..]).collect();
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let options = CodecOptions::builder().matrix_mode(mode).build();
        let expected = reference(
            &galois_8::ReedSolomon::with_options(data, parity, options).unwrap(),
            &input_views,
            parity,
        );
        for jobs in [2, 3, 4] {
            let codec = ReedSolomon::<ObservedField<0>>::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .matrix_mode(mode)
                    .max_parallel_jobs(jobs)
                    .build(),
            )
            .unwrap();
            let decision = codec.parallel_policy(len, parity);
            assert!(decision.use_parallel);
            assert!(decision.jobs <= jobs);
            let mut output_storage = vec![vec![0xa5; len + 6]; parity];
            let mut output_views: Vec<_> = output_storage
                .iter_mut()
                .map(|output| &mut output[3..3 + len])
                .collect();
            MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
            codec
                .encode_sep_par(&input_views, &mut output_views)
                .unwrap();
            assert_eq!(
                MULTIPLICATION_CALLS.load(Ordering::Relaxed),
                0,
                "expected GFNI row dispatch {data}+{parity}, len={len}, jobs={jobs}"
            );
            assert!(
                output_views
                    .iter()
                    .zip(&expected)
                    .all(|(actual, expected)| *actual == expected.as_slice())
            );
            assert!(
                output_storage
                    .iter()
                    .all(|output| output[..3] == [0xa5; 3] && output[3 + len..] == [0xa5; 3])
            );

            if jobs == 2 {
                let mut complete: Vec<_> = input_views.iter().map(|input| input.to_vec()).collect();
                complete.extend(expected.iter().cloned());
                let mut scratch = vec![vec![0; len]; parity];
                MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
                assert!(
                    codec
                        .verify_with_buffer_par(&complete, &mut scratch)
                        .unwrap()
                );
                assert_eq!(MULTIPLICATION_CALLS.load(Ordering::Relaxed), 0);
                complete[data][len - 1] ^= 1;
                assert!(
                    !codec
                        .verify_with_buffer_par(&complete, &mut scratch)
                        .unwrap()
                );
            }
        }
    }
}

fn check_custom_matrix() {
    let rows = vec![
        vec![0; 10],
        vec![1; 10],
        (0..10).map(|column| (column * 19 + 2) as u8).collect(),
    ];
    let options = CodecOptions::builder().max_parallel_jobs(3).build();
    let codec = ReedSolomon::<ObservedField<0>>::with_custom_matrix(10, 3, &rows, options).unwrap();
    let reference_codec = galois_8::ReedSolomon::with_custom_matrix(10, 3, &rows, options).unwrap();
    let storage = inputs(10, 262145);
    let views: Vec<_> = storage.iter().map(Vec::as_slice).collect();
    let expected = reference(&reference_codec, &views, 3);
    let mut outputs = vec![vec![0xa5; views[0].len()]; 3];
    MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
    codec.encode_sep_par(&views, &mut outputs).unwrap();
    assert_eq!(MULTIPLICATION_CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(outputs, expected);
}

fn check_fallback<const MODE: u8>(data: usize, parity: usize, len: usize, jobs: usize) {
    let codec = ReedSolomon::<ObservedField<MODE>>::with_options(
        data,
        parity,
        CodecOptions::builder().max_parallel_jobs(jobs).build(),
    )
    .unwrap();
    let storage = inputs(data, len);
    let views: Vec<_> = storage.iter().map(Vec::as_slice).collect();
    let expected = reference(
        &galois_8::ReedSolomon::new(data, parity).unwrap(),
        &views,
        parity,
    );
    let mut outputs = vec![vec![0xa5; len]; parity];
    assert!(codec.parallel_policy(len, parity).use_parallel);
    MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
    OUTPUT_VIEWS.store(0, Ordering::Relaxed);
    codec.encode_sep_par(&views, &mut outputs).unwrap();
    assert!(
        MULTIPLICATION_CALLS.load(Ordering::Relaxed) > 0,
        "expected generic fallback {data}+{parity}, len={len}, mode={MODE}"
    );
    assert_eq!(outputs, expected);
}

fn check_changing_output_view() {
    struct ChangingOutput {
        bytes: Vec<u8>,
        short_first_view: bool,
        mutable_views: usize,
    }

    impl AsRef<[u8]> for ChangingOutput {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }

    impl AsMut<[u8]> for ChangingOutput {
        fn as_mut(&mut self) -> &mut [u8] {
            self.mutable_views += 1;
            let len = self.bytes.len();
            if self.short_first_view && self.mutable_views == 1 {
                &mut self.bytes[..len - 1]
            } else {
                &mut self.bytes
            }
        }
    }

    let len = 256 * 1024 + 1;
    let codec = ReedSolomon::<ObservedField<4>>::with_options(
        10,
        4,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let storage = inputs(10, len);
    let input_views: Vec<_> = storage.iter().map(Vec::as_slice).collect();
    let expected = reference(&galois_8::ReedSolomon::new(10, 4).unwrap(), &input_views, 4);
    let mut outputs: Vec<_> = (0..4)
        .map(|index| ChangingOutput {
            bytes: vec![0xa5; len],
            short_first_view: index == 3,
            mutable_views: 0,
        })
        .collect();
    assert!(codec.parallel_policy(len, 4).use_parallel);
    MULTIPLICATION_CALLS.store(0, Ordering::Relaxed);
    codec.encode_sep_par(&input_views, &mut outputs).unwrap();
    assert!(MULTIPLICATION_CALLS.load(Ordering::Relaxed) > 0);
    for (output, expected) in outputs.iter().zip(&expected) {
        // Public length validation uses AsRef. Native preparation takes the
        // first AsMut view; after its late failure the generic scheduler takes
        // the second complete view. ObservedField<4> checks every output chunk's
        // sentinel immediately before its first generic arithmetic write.
        assert_eq!(output.mutable_views, 2);
        assert_eq!(&output.bytes, expected);
    }
}
