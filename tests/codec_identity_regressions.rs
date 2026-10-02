use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, Field, LeopardMode, MatrixMode};

fn data_shards(count: usize, len: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|index| {
            (0..len)
                .map(|offset| (index * 71 + offset * 19) as u8)
                .collect()
        })
        .collect()
}

fn assert_single_parity_consistency(codec: &ReedSolomon) {
    let data = data_shards(4, 256);
    let mut parity = vec![vec![0; 256]];
    codec.encode_sep(&data, &mut parity).unwrap();
    let expected: Vec<u8> = (0..256)
        .map(|offset| data.iter().fold(0, |sum, shard| sum ^ shard[offset]))
        .collect();
    assert_eq!(parity[0], expected);

    let mut full = data.clone();
    full.extend(parity.clone());
    assert!(codec.verify(&full).unwrap());
    for missing_index in 0..full.len() {
        let mut missing: Vec<_> = full.iter().cloned().map(Some).collect();
        missing[missing_index] = None;
        codec.reconstruct(&mut missing).unwrap();
        assert_eq!(
            missing.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
            full
        );
    }

    let mut incremental = vec![vec![0; 256]];
    for (index, shard) in data.iter().enumerate() {
        codec
            .encode_single_sep(index, shard, &mut incremental)
            .unwrap();
    }
    assert_eq!(incremental, parity);

    let mut changed = data.clone();
    changed[2].iter_mut().for_each(|value| *value ^= 0x73);
    let mut updates = vec![None; 4];
    updates[2] = Some(changed[2].clone());
    codec.update(&data, &updates, &mut parity).unwrap();
    let mut recomputed = vec![vec![0; 256]];
    codec.encode_sep(&changed, &mut recomputed).unwrap();
    assert_eq!(parity, recomputed);
}

#[test]
fn fast_single_parity_uses_one_format_for_all_operations() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::with_options(
            4,
            1,
            CodecOptions::builder()
                .matrix_mode(mode)
                .fast_one_parity(true)
                .build(),
        )
        .unwrap();
        assert_single_parity_consistency(&codec);
    }
    let custom = ReedSolomon::with_custom_matrix(
        4,
        1,
        &[vec![1; 4]],
        CodecOptions::builder().fast_one_parity(true).build(),
    )
    .unwrap();
    assert_single_parity_consistency(&custom);
}

#[test]
fn fast_single_parity_rejects_incompatible_custom_matrix() {
    assert_eq!(
        ReedSolomon::with_custom_matrix(
            4,
            1,
            &[vec![1, 2, 3, 4]],
            CodecOptions::builder().fast_one_parity(true).build()
        )
        .unwrap_err(),
        Error::InvalidCustomMatrix
    );
}

#[test]
fn clone_preserves_custom_matrix_and_resolved_family() {
    for mode in [
        LeopardMode::Disabled,
        LeopardMode::PreferLeopard,
        LeopardMode::PreferGF16,
    ] {
        let codec = ReedSolomon::with_custom_matrix(
            4,
            2,
            &[vec![1; 4], vec![1, 2, 4, 8]],
            CodecOptions::builder().leopard_mode(mode).build(),
        )
        .unwrap();
        let cloned = codec.clone();
        assert_eq!(cloned.codec_family(), CodecFamily::Classic);
        assert_eq!(cloned, codec);
        let data = data_shards(4, 64);
        let mut original_parity = vec![vec![0; 64]; 2];
        let mut cloned_parity = original_parity.clone();
        codec.encode_sep(&data, &mut original_parity).unwrap();
        cloned.encode_sep(&data, &mut cloned_parity).unwrap();
        assert_eq!(cloned_parity, original_parity);
        let mut full = data;
        full.extend(original_parity);
        let mut missing: Vec<_> = full.iter().cloned().map(Some).collect();
        missing[1] = None;
        missing[5] = None;
        cloned.reconstruct(&mut missing).unwrap();
        assert_eq!(
            missing.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
            full
        );
    }
}

#[test]
fn clone_preserves_leopard_parity_bytes() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec =
            ReedSolomon::with_options(4, 2, CodecOptions::builder().codec_family(family).build())
                .unwrap();
        let cloned = codec.clone();
        assert_eq!(cloned.codec_family(), family);
        let data = data_shards(4, 64);
        let mut expected = vec![vec![0; 64]; 2];
        let mut actual = expected.clone();
        codec.encode_sep(&data, &mut expected).unwrap();
        cloned.encode_sep(&data, &mut actual).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn codec_equality_compares_effective_coefficients() {
    let ordinary = ReedSolomon::new(4, 1).unwrap();
    let fast =
        ReedSolomon::with_options(4, 1, CodecOptions::builder().fast_one_parity(true).build())
            .unwrap();
    let custom_fast =
        ReedSolomon::with_custom_matrix(4, 1, &[vec![1; 4]], CodecOptions::default()).unwrap();
    assert_ne!(ordinary, fast);
    assert_eq!(fast, custom_fast);
    assert_eq!(ordinary, ReedSolomon::new(4, 1).unwrap());
    let cauchy = ReedSolomon::with_options(
        4,
        1,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .build(),
    )
    .unwrap();
    assert_ne!(ordinary, cauchy);
}

// Deliberately does not implement Clone or PartialEq: the public Field trait
// never required them, so cloning/comparing codecs must not add these bounds.
struct NibbleField;

impl Field for NibbleField {
    const ORDER: usize = 16;
    type Elem = u8;

    fn add(a: u8, b: u8) -> u8 {
        a ^ b
    }
    fn mul(mut a: u8, mut b: u8) -> u8 {
        let mut result = 0;
        for _ in 0..4 {
            if b & 1 != 0 {
                result ^= a;
            }
            b >>= 1;
            a <<= 1;
            if a & 0x10 != 0 {
                a ^= 0x13;
            }
        }
        result
    }
    fn div(a: u8, b: u8) -> u8 {
        Self::mul(a, Self::exp(b, 14))
    }
    fn exp(a: u8, n: usize) -> u8 {
        (0..n).fold(1, |value, _| Self::mul(value, a))
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

#[test]
fn byte_sized_custom_field_keeps_its_own_arithmetic() {
    let rows: Vec<Vec<u8>> = (0..4)
        .map(|row| {
            (0..10)
                .map(|col| ((row * 3 + col) % 15 + 1) as u8)
                .collect()
        })
        .collect();
    let codec = rustfs_erasure_codec::ReedSolomon::<NibbleField>::with_custom_matrix(
        10,
        4,
        &rows,
        CodecOptions::default(),
    )
    .unwrap();
    assert!(codec == codec.clone());
    let data: Vec<Vec<u8>> = (0..10)
        .map(|index| {
            (0..32_768)
                .map(|offset| ((index + offset) % 16) as u8)
                .collect()
        })
        .collect();
    let mut parity = vec![vec![0; 32_768]; 4];
    codec.encode_sep(&data, &mut parity).unwrap();
    for (coefficients, actual) in rows.iter().zip(&parity) {
        let expected: Vec<_> = (0..32_768)
            .map(|offset| {
                coefficients
                    .iter()
                    .zip(&data)
                    .fold(0, |sum, (&coefficient, shard)| {
                        NibbleField::add(sum, NibbleField::mul(coefficient, shard[offset]))
                    })
            })
            .collect();
        assert_eq!(*actual, expected);
    }
}

#[test]
fn byte_sized_custom_field_cannot_select_leopard() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let result = rustfs_erasure_codec::ReedSolomon::<NibbleField>::with_options(
            4,
            2,
            CodecOptions::builder().codec_family(family).build(),
        );
        assert!(matches!(result, Err(Error::UnsupportedCodecFamily)));
    }
    let codec = rustfs_erasure_codec::ReedSolomon::<NibbleField>::with_options(
        4,
        2,
        CodecOptions::builder()
            .leopard_mode(LeopardMode::PreferLeopard)
            .build(),
    )
    .unwrap();
    assert_eq!(codec.codec_family(), CodecFamily::Classic);
}

// Safe trait implementations cannot be trusted to supply a consistent byte
// view. Even after construction has seen valid empty views, each nonempty view
// must be checked before entering a specialized kernel.
struct ShortByteViews;

impl Field for ShortByteViews {
    const ORDER: usize = 256;
    type Elem = u8;

    fn as_gf8_slice(slice: &[u8]) -> Option<&[u8]> {
        Some(&slice[..0])
    }
    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        Some(&mut slice[..0])
    }
    fn add(a: u8, b: u8) -> u8 {
        rustfs_erasure_codec::galois_8::add(a, b)
    }
    fn mul(a: u8, b: u8) -> u8 {
        rustfs_erasure_codec::galois_8::mul(a, b)
    }
    fn div(a: u8, b: u8) -> u8 {
        rustfs_erasure_codec::galois_8::div(a, b)
    }
    fn exp(a: u8, n: usize) -> u8 {
        rustfs_erasure_codec::galois_8::exp(a, n)
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

#[test]
fn inconsistent_safe_byte_views_are_rejected_by_leopard() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec = rustfs_erasure_codec::ReedSolomon::<ShortByteViews>::with_options(
            4,
            2,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let data = data_shards(4, 64);
        let mut parity = vec![vec![0; 64]; 2];
        assert_eq!(
            codec.encode_sep(&data, &mut parity),
            Err(Error::IncorrectShardSize)
        );
        let mut missing: Vec<_> = data.into_iter().map(Some).collect();
        missing.extend([None, None]);
        assert_eq!(
            codec.reconstruct(&mut missing),
            Err(Error::IncorrectShardSize)
        );
    }
}

#[test]
fn inconsistent_safe_byte_views_fall_back_to_generic_classic() {
    let codec = rustfs_erasure_codec::ReedSolomon::<ShortByteViews>::new(10, 4).unwrap();
    let native = ReedSolomon::new(10, 4).unwrap();
    let data = data_shards(10, 32_768);
    let mut actual = vec![vec![0; 32_768]; 4];
    let mut expected = actual.clone();
    codec.encode_sep(&data, &mut actual).unwrap();
    native.encode_sep(&data, &mut expected).unwrap();
    assert_eq!(actual, expected);
}
