use std::cell::Cell;

use rustfs_erasure_codec::galois_8;
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, Field, LeopardMode, MatrixMode};

thread_local! {
    static CLASSIC_ARITHMETIC_CALLS: Cell<usize> = const { Cell::new(0) };
}

fn record_classic_arithmetic() {
    CLASSIC_ARITHMETIC_CALLS.with(|calls| calls.set(calls.get() + 1));
}

// A valid native-byte field whose arithmetic remains observable. Deliberately
// omit Clone/PartialEq: codec cloning and equality must not require those bounds.
#[derive(Debug)]
struct ObservedField;

impl Field for ObservedField {
    const ORDER: usize = 256;
    type Elem = u8;

    fn as_gf8_slice(slice: &[u8]) -> Option<&[u8]> {
        Some(slice)
    }

    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        Some(slice)
    }

    fn add(a: u8, b: u8) -> u8 {
        record_classic_arithmetic();
        galois_8::add(a, b)
    }

    fn mul(a: u8, b: u8) -> u8 {
        record_classic_arithmetic();
        galois_8::mul(a, b)
    }

    fn div(a: u8, b: u8) -> u8 {
        record_classic_arithmetic();
        galois_8::div(a, b)
    }

    fn exp(a: u8, n: usize) -> u8 {
        record_classic_arithmetic();
        galois_8::exp(a, n)
    }

    fn zero() -> u8 {
        0
    }

    fn one() -> u8 {
        1
    }

    fn nth_internal(n: usize) -> u8 {
        record_classic_arithmetic();
        n as u8
    }
}

type ObservedCodec = rustfs_erasure_codec::ReedSolomon<ObservedField>;

#[test]
fn fft_construction_and_clone_do_not_perform_classic_matrix_arithmetic() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        for (data, parity) in [(4, 2), (32, 16), (200, 32), (128, 128), (252, 4)] {
            CLASSIC_ARITHMETIC_CALLS.with(|calls| calls.set(0));
            let codec = ObservedCodec::with_options(
                data,
                parity,
                CodecOptions::builder().codec_family(family).build(),
            )
            .unwrap();
            let cloned = codec.clone();
            assert_eq!(codec, cloned);
            assert_eq!(codec.codec_family(), family);
            assert_eq!(codec.total_shard_count(), data + parity);
            assert_eq!(
                codec.leopard_setup_matrix_shape(),
                (family == CodecFamily::LeopardGF8).then_some((data + parity, data))
            );
            CLASSIC_ARITHMETIC_CALLS.with(|calls| {
                assert_eq!(calls.get(), 0, "{family:?}, {data}+{parity}");
            });
        }
    }

    // Ensure the observer detects the setup work that Classic still requires.
    CLASSIC_ARITHMETIC_CALLS.with(|calls| calls.set(0));
    let classic = ObservedCodec::new(4, 2).unwrap();
    CLASSIC_ARITHMETIC_CALLS.with(|calls| assert!(calls.get() > 0));
    assert_eq!(classic.leopard_setup_matrix_shape(), None);
}

fn full_shards(data: usize, parity: usize, len: usize) -> Vec<Vec<u8>> {
    (0..data + parity)
        .map(|shard| {
            (0..len)
                .map(|offset| (shard * 37 + offset * 19 + offset / 67) as u8)
                .collect()
        })
        .collect()
}

#[test]
fn resolved_fft_family_preserves_parity_reconstruction_and_equality() {
    for (family, mode) in [
        (CodecFamily::LeopardGF8, LeopardMode::PreferLeopard),
        (CodecFamily::LeopardGF16, LeopardMode::PreferGF16),
    ] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let explicit = galois_8::ReedSolomon::with_options(
            10,
            4,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let mut expected = full_shards(10, 4, 192);
        explicit.encode(&mut expected).unwrap();

        for matrix_mode in [
            MatrixMode::Vandermonde,
            MatrixMode::Cauchy,
            MatrixMode::JerasureLike,
            MatrixMode::Custom,
        ] {
            // Matrix options do not affect FFT coding, including the Custom
            // selector when no user-provided Classic matrix is being supplied.
            let selected = galois_8::ReedSolomon::with_options(
                10,
                4,
                CodecOptions::builder()
                    .leopard_mode(mode)
                    .matrix_mode(matrix_mode)
                    .fast_one_parity(true)
                    .build(),
            )
            .unwrap();
            assert_eq!(selected, explicit);
            let cloned = selected.clone();
            assert_eq!(cloned.codec_family(), family);
            let mut actual = full_shards(10, 4, 192);
            cloned.encode(&mut actual).unwrap();
            assert_eq!(actual, expected);
            assert!(cloned.verify(&expected).unwrap());

            let mut missing: Vec<_> = expected.iter().cloned().map(Some).collect();
            for index in [0, 9, 10, 13] {
                missing[index] = None;
            }
            cloned.reconstruct(&mut missing).unwrap();
            assert_eq!(
                missing.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
                expected
            );
        }
        assert_ne!(explicit, galois_8::ReedSolomon::new(10, 4).unwrap());
    }
}

#[test]
fn removing_unused_setup_preserves_constructor_limits_and_field_checks() {
    let gf8_options = CodecOptions::builder()
        .codec_family(CodecFamily::LeopardGF8)
        .build();
    for (data, parity) in [(200, 33), (253, 4), (1, 256)] {
        assert_eq!(
            ObservedCodec::with_options(data, parity, gf8_options).unwrap_err(),
            Error::TooManyShards
        );
    }
    assert_eq!(
        rustfs_erasure_codec::galois_16::ReedSolomon::with_options(4, 2, gf8_options).unwrap_err(),
        Error::UnsupportedCodecFamily
    );
    assert_eq!(
        ObservedCodec::with_custom_matrix(4, 2, &[vec![1; 4], vec![1, 2, 4, 8]], gf8_options)
            .unwrap_err(),
        Error::UnsupportedCodecFamily
    );

    if cfg!(target_endian = "little") {
        CLASSIC_ARITHMETIC_CALLS.with(|calls| calls.set(0));
        let selected = ObservedCodec::with_options(
            65_532,
            4,
            CodecOptions::builder()
                .leopard_mode(LeopardMode::AsNeeded)
                .build(),
        )
        .unwrap();
        assert_eq!(selected.codec_family(), CodecFamily::LeopardGF16);
        assert_eq!(selected.leopard_setup_matrix_shape(), None);
        assert_eq!(selected, selected.clone());
        CLASSIC_ARITHMETIC_CALLS.with(|calls| assert_eq!(calls.get(), 0));
        assert_eq!(
            ObservedCodec::with_options(
                65_533,
                4,
                CodecOptions::builder()
                    .codec_family(CodecFamily::LeopardGF16)
                    .build(),
            )
            .unwrap_err(),
            Error::TooManyShards
        );
    }
}
