use rustfs_erasure_codec::{
    CodecFamily, CodecOptions, ENCODING_FORMAT_VERSION, Field, LeopardMode, MatrixMode,
    ReedSolomon, galois_8, galois_16,
};
use std::cell::Cell;

// Version-1 wire fixtures are literal bytes, independent of serializer helpers.
// Both use a 1+1 custom matrix, exercising high coefficient bytes and the tower
// field's [X coefficient, constant] element order without native endian casts.
const GF8_ID_V1: &[u8] = &[
    0x52, 0x53, 0x45, 0x46, 0x4d, 0x54, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x19, 0x72,
    0x75, 0x73, 0x74, 0x66, 0x73, 0x2e, 0x67, 0x66, 0x38, 0x2e, 0x70, 0x6f, 0x6c, 0x79, 0x2d, 0x31,
    0x31, 0x64, 0x2e, 0x75, 0x38, 0x2e, 0x76, 0x31, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x01, 0x87,
];
const TOWER_GF16_ID_V1: &[u8] = &[
    0x52, 0x53, 0x45, 0x46, 0x4d, 0x54, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x28, 0x72,
    0x75, 0x73, 0x74, 0x66, 0x73, 0x2e, 0x67, 0x66, 0x31, 0x36, 0x2e, 0x74, 0x6f, 0x77, 0x65, 0x72,
    0x2d, 0x31, 0x31, 0x64, 0x2d, 0x78, 0x32, 0x2d, 0x30, 0x32, 0x78, 0x2d, 0x38, 0x30, 0x2e, 0x61,
    0x78, 0x2d, 0x61, 0x63, 0x2e, 0x76, 0x31, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x01, 0x12, 0x34,
];

#[test]
fn built_in_stable_ids_match_versioned_byte_fixtures() {
    let gf8 =
        galois_8::ReedSolomon::with_custom_matrix(1, 1, &[vec![0x87]], CodecOptions::default())
            .unwrap();
    let tower = galois_16::ReedSolomon::with_custom_matrix(
        1,
        1,
        &[vec![[0x12, 0x34]]],
        CodecOptions::default(),
    )
    .unwrap();
    assert_eq!(ENCODING_FORMAT_VERSION, 1);
    assert_eq!(gf8.encoding_format().descriptor_version(), 1);
    assert_eq!(gf8.encoding_format().stable_id_bytes().unwrap(), GF8_ID_V1);
    assert_eq!(
        tower.encoding_format().stable_id_bytes().unwrap(),
        TOWER_GF16_ID_V1
    );
    assert_eq!(gf8.encoding_format().matrix_shape(), (2, 1));
    assert_eq!(gf8.encoding_format().matrix_row(1), Some([0x87].as_slice()));
    assert_eq!(gf8.encoding_format().matrix_row(2), None);
    assert_eq!(
        tower.encoding_format().matrix_row(1),
        Some([[0x12, 0x34]].as_slice())
    );
    assert_eq!(galois_8::Field::write_format_element(1, &mut []), None);
    assert_eq!(
        galois_16::Field::write_format_element([0, 1], &mut [0]),
        None
    );
}

fn parity_rows(codec: &galois_8::ReedSolomon) -> Vec<Vec<u8>> {
    (codec.data_shard_count()..codec.total_shard_count())
        .map(|index| codec.encoding_format().matrix_row(index).unwrap().to_vec())
        .collect()
}

fn encoded(codec: &galois_8::ReedSolomon) -> Vec<Vec<u8>> {
    let mut shards: Vec<_> = (0..codec.total_shard_count())
        .map(|index| {
            (0..128)
                .map(|offset| (index * 31 + offset * 7) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}

#[test]
fn effective_coefficients_define_identity_independently_of_matrix_and_execution_options() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let original = galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder().matrix_mode(mode).build(),
        )
        .unwrap();
        let equivalent = galois_8::ReedSolomon::with_custom_matrix(
            4,
            2,
            &parity_rows(&original),
            CodecOptions::builder()
                .inversion_cache(false)
                .inversion_cache_capacity(7)
                .max_parallel_jobs(3)
                .fast_one_parity(true)
                .leopard_mode(LeopardMode::PreferLeopard)
                .build(),
        )
        .unwrap();
        assert_eq!(original.encoding_format(), equivalent.encoding_format());
        assert_eq!(original, equivalent);
        assert_eq!(
            original.encoding_format().stable_id_bytes(),
            equivalent.encoding_format().stable_id_bytes()
        );
        assert_eq!(encoded(&original), encoded(&equivalent));
        let cloned = original.encoding_format().clone();
        assert_eq!(cloned, *original.encoding_format());
        assert!(std::ptr::eq(
            cloned.matrix_row(4).unwrap(),
            original.encoding_format().matrix_row(4).unwrap()
        ));
        drop(original);
        assert_eq!(
            cloned.stable_id_bytes(),
            equivalent.encoding_format().stable_id_bytes()
        );
    }
}

#[test]
fn different_effective_formats_have_different_complete_identifiers() {
    let ordinary = galois_8::ReedSolomon::new(4, 1).unwrap();
    let fast = galois_8::ReedSolomon::with_options(
        4,
        1,
        CodecOptions::builder().fast_one_parity(true).build(),
    )
    .unwrap();
    let custom_fast =
        galois_8::ReedSolomon::with_custom_matrix(4, 1, &[vec![1; 4]], CodecOptions::default())
            .unwrap();
    assert_ne!(
        ordinary.encoding_format().stable_id_bytes(),
        fast.encoding_format().stable_id_bytes()
    );
    assert_eq!(
        fast.encoding_format().stable_id_bytes(),
        custom_fast.encoding_format().stable_id_bytes()
    );
    let cauchy = galois_8::ReedSolomon::with_options(
        4,
        1,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .build(),
    )
    .unwrap();
    assert_ne!(
        ordinary.encoding_format().stable_id_bytes(),
        cauchy.encoding_format().stable_id_bytes()
    );
    let different_counts = galois_8::ReedSolomon::new(3, 2).unwrap();
    assert_ne!(
        ordinary.encoding_format().stable_id_bytes(),
        different_counts.encoding_format().stable_id_bytes()
    );
}

#[test]
fn fft_formats_record_resolved_families_without_classic_matrix_coefficients() {
    let mut identities = Vec::new();
    identities.push(
        galois_8::ReedSolomon::new(4, 2)
            .unwrap()
            .encoding_format()
            .stable_id_bytes()
            .unwrap(),
    );
    for (family, mode) in [
        (CodecFamily::LeopardGF8, LeopardMode::PreferLeopard),
        (CodecFamily::LeopardGF16, LeopardMode::PreferGF16),
    ] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let explicit = galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder().codec_family(family).build(),
        )
        .unwrap();
        let automatic = galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder()
                .leopard_mode(mode)
                .matrix_mode(MatrixMode::Cauchy)
                .fast_one_parity(true)
                .build(),
        )
        .unwrap();
        assert_eq!(explicit.encoding_format(), automatic.encoding_format());
        assert_eq!(explicit.encoding_format().codec_family(), family);
        assert_eq!(explicit.encoding_format().matrix_shape(), (0, 0));
        assert_eq!(explicit.encoding_format().matrix_row(0), None);
        let id = explicit.encoding_format().stable_id_bytes().unwrap();
        assert_eq!(
            Some(id.clone()),
            automatic.encoding_format().stable_id_bytes()
        );
        assert!(!identities.contains(&id));
        identities.push(id);
        assert_eq!(encoded(&explicit), encoded(&automatic));
    }
    let tower = galois_16::ReedSolomon::new(4, 2).unwrap();
    assert!(!identities.contains(&tower.encoding_format().stable_id_bytes().unwrap()));
}

macro_rules! native_field_math {
    () => {
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
            Some(slice)
        }
        fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
            Some(slice)
        }
    };
}

// No Clone, PartialEq, or stable metadata opt-in is required from a Field.
struct LocalField;
impl Field for LocalField {
    native_field_math!();
}

#[test]
fn custom_fields_without_opt_in_keep_typed_identity_but_have_no_persistent_id() {
    let codec = ReedSolomon::<LocalField>::new(4, 2).unwrap();
    let format = codec.encoding_format().clone();
    assert!(codec == codec.clone());
    assert!(codec.encoding_format() == &format);
    assert_eq!(format.stable_id_bytes(), None);
    let fft = ReedSolomon::<LocalField>::with_options(
        4,
        2,
        CodecOptions::builder()
            .codec_family(CodecFamily::LeopardGF8)
            .build(),
    )
    .unwrap();
    assert_eq!(fft.encoding_format().stable_id_bytes(), None);
}

thread_local! {
    static PORTABLE_HOOK_CALLS: Cell<usize> = const { Cell::new(0) };
}

struct ReportedField<const MODE: u8>;
impl<const MODE: u8> Field for ReportedField<MODE> {
    native_field_math!();
    fn stable_format() -> Option<(&'static str, usize)> {
        PORTABLE_HOOK_CALLS.with(|calls| calls.set(calls.get() + 1));
        Some(match MODE {
            4 => ("test.gf8.poly11d.u8.v1", 0),
            5 => ("test.gf8.poly11d.u8.v1", usize::MAX),
            6 => ("", 1),
            7 => ("test.gf8.poly11d.u8.v1", 64 * 1024 * 1024),
            _ => ("test.gf8.poly11d.u8.v1", 1),
        })
    }
    fn write_format_element(element: u8, output: &mut [u8]) -> Option<usize> {
        match MODE {
            2 => Some(0),
            3 => Some(output.len() + 1),
            8 => None,
            _ => galois_8::Field::write_format_element(element, output),
        }
    }
}

fn rejected_descriptor<const MODE: u8>() {
    let codec = ReedSolomon::<ReportedField<MODE>>::new(1, 1).unwrap();
    assert_eq!(codec.encoding_format().stable_id_bytes(), None);
}

#[test]
fn malformed_field_descriptors_fail_without_panicking_or_large_allocations() {
    rejected_descriptor::<2>();
    rejected_descriptor::<3>();
    rejected_descriptor::<4>();
    rejected_descriptor::<5>();
    rejected_descriptor::<6>();
    rejected_descriptor::<7>();
    rejected_descriptor::<8>();
}

#[test]
fn portable_hooks_run_only_for_explicit_serialization() {
    PORTABLE_HOOK_CALLS.with(|calls| calls.set(0));
    let codec = ReedSolomon::<ReportedField<1>>::new(4, 2).unwrap();
    let cloned = codec.clone();
    assert!(codec == cloned);
    let format = codec.encoding_format().clone();
    assert!(format == *cloned.encoding_format());
    let mut original = vec![vec![7; 64]; 6];
    codec.encode(&mut original).unwrap();
    let mut missing: Vec<_> = original.iter().cloned().map(Some).collect();
    missing[0] = None;
    cloned.reconstruct(&mut missing).unwrap();
    assert_eq!(
        missing.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
        original
    );
    PORTABLE_HOOK_CALLS.with(|calls| assert_eq!(calls.get(), 0));
    let id = format.stable_id_bytes().unwrap();
    PORTABLE_HOOK_CALLS.with(|calls| assert_eq!(calls.get(), 1));
    assert_ne!(
        id,
        galois_8::ReedSolomon::new(4, 2)
            .unwrap()
            .encoding_format()
            .stable_id_bytes()
            .unwrap()
    );
}

#[cfg(feature = "std")]
#[test]
fn reconstruction_workspace_uses_the_same_shared_effective_format() {
    let source = galois_8::ReedSolomon::with_options(
        4,
        2,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .build(),
    )
    .unwrap();
    let equivalent = galois_8::ReedSolomon::with_custom_matrix(
        4,
        2,
        &parity_rows(&source),
        CodecOptions::default(),
    )
    .unwrap();
    let complete = encoded(&source);
    let mut missing: Vec<_> = complete.iter().cloned().map(Some).collect();
    missing[0] = None;
    missing[5] = None;
    let workspace = source.prepare_reconstruct_opt_workspace(&missing).unwrap();
    assert_eq!(workspace.encoding_format(), source.encoding_format());
    assert!(std::ptr::eq(
        workspace.encoding_format().matrix_row(4).unwrap(),
        source.encoding_format().matrix_row(4).unwrap()
    ));
    assert_eq!(
        workspace.encoding_format().stable_id_bytes(),
        equivalent.encoding_format().stable_id_bytes()
    );
    equivalent
        .reconstruct_opt_with_workspace(&mut missing, &workspace)
        .unwrap();
    assert_eq!(
        missing.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
        complete
    );
}

#[test]
fn verification_workspace_is_scratch_and_can_be_reused_across_formats() {
    let first = galois_8::ReedSolomon::new(4, 2).unwrap();
    let second = galois_8::ReedSolomon::with_options(
        4,
        2,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .build(),
    )
    .unwrap();
    let mut scratch = rustfs_erasure_codec::VerifyWorkspace::new(&first, 128);
    assert_ne!(first.encoding_format(), second.encoding_format());
    assert!(
        first
            .verify_with_workspace(&encoded(&first), &mut scratch)
            .unwrap()
    );
    assert!(
        second
            .verify_with_workspace(&encoded(&second), &mut scratch)
            .unwrap()
    );
}
