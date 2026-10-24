extern crate alloc;

use alloc::vec;

#[cfg(feature = "std")]
use super::profile::{RS_NEON_MUL_SLICE_XOR_SCHEDULE_ENV, parse_rust_neon_xor_unroll};
use super::*;
use crate::tests::fill_random;
use rand;

#[cfg(all(
    feature = "std",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
fn backend_test_child(name: &str) -> std::process::Command {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .env_remove("RSE_BACKEND_OVERRIDE")
        .env_remove("RSE_STRICT_BACKEND_OVERRIDE")
        .arg("--exact")
        .arg(name)
        .arg("--nocapture");
    child
}

#[cfg(all(
    feature = "std",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    any(
        feature = "simd-neon",
        feature = "simd-ssse3",
        feature = "simd-avx2",
        feature = "simd-avx512",
        feature = "simd-gfni"
    )
))]
fn assert_backend_override_in_child(
    value: &str,
    expected_name: Option<&str>,
    expected_id: Option<BackendId>,
) {
    let parent_override = std::env::var_os("RSE_BACKEND_OVERRIDE");
    let output =
        backend_test_child("galois_8::tests::test_backend_override_affects_active_backend")
            .env("RSE_GALOIS8_OVERRIDE_CHILD", "1")
            .env("RSE_BACKEND_OVERRIDE", value)
            .env("RSE_GALOIS8_EXPECTED_NAME", expected_name.unwrap_or("none"))
            .env("RSE_GALOIS8_EXPECTED_ID", format!("{expected_id:?}"))
            .output()
            .unwrap();
    assert!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "override {value:?} child failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::env::var_os("RSE_BACKEND_OVERRIDE"), parent_override);
}

static BACKBLAZE_LOG_TABLE: [u8; 256] = [
    //-1,    0,    1,   25,    2,   50,   26,  198,
    // first value is changed from -1 to 0
    0, 0, 1, 25, 2, 50, 26, 198, 3, 223, 51, 238, 27, 104, 199, 75, 4, 100, 224, 14, 52, 141, 239,
    129, 28, 193, 105, 248, 200, 8, 76, 113, 5, 138, 101, 47, 225, 36, 15, 33, 53, 147, 142, 218,
    240, 18, 130, 69, 29, 181, 194, 125, 106, 39, 249, 185, 201, 154, 9, 120, 77, 228, 114, 166, 6,
    191, 139, 98, 102, 221, 48, 253, 226, 152, 37, 179, 16, 145, 34, 136, 54, 208, 148, 206, 143,
    150, 219, 189, 241, 210, 19, 92, 131, 56, 70, 64, 30, 66, 182, 163, 195, 72, 126, 110, 107, 58,
    40, 84, 250, 133, 186, 61, 202, 94, 155, 159, 10, 21, 121, 43, 78, 212, 229, 172, 115, 243,
    167, 87, 7, 112, 192, 247, 140, 128, 99, 13, 103, 74, 222, 237, 49, 197, 254, 24, 227, 165,
    153, 119, 38, 184, 180, 124, 17, 68, 146, 217, 35, 32, 137, 46, 55, 63, 209, 91, 149, 188, 207,
    205, 144, 135, 151, 178, 220, 252, 190, 97, 242, 86, 211, 171, 20, 42, 93, 158, 132, 60, 57,
    83, 71, 109, 65, 162, 31, 45, 67, 216, 183, 123, 164, 118, 196, 23, 73, 236, 127, 12, 111, 246,
    108, 161, 59, 82, 41, 157, 85, 170, 251, 96, 134, 177, 187, 204, 62, 90, 203, 89, 95, 176, 156,
    169, 160, 81, 11, 245, 22, 235, 122, 117, 44, 215, 79, 174, 213, 233, 230, 231, 173, 232, 116,
    214, 244, 234, 168, 80, 88, 175,
];

const PROPERTY_TEST_VALUES: [u8; 16] = [
    0, 1, 2, 3, 7, 15, 31, 63, 85, 127, 128, 129, 170, 192, 254, 255,
];

#[test]
fn log_table_same_as_backblaze() {
    for i in 0..256 {
        assert_eq!(LOG_TABLE[i], BACKBLAZE_LOG_TABLE[i]);
    }
}

#[test]
fn test_associativity() {
    for a in 0..256 {
        let a = a as u8;
        for b in 0..256 {
            let b = b as u8;
            for &c in &PROPERTY_TEST_VALUES {
                let x = add(a, add(b, c));
                let y = add(add(a, b), c);
                assert_eq!(x, y);
                let x = mul(a, mul(b, c));
                let y = mul(mul(a, b), c);
                assert_eq!(x, y);
            }
        }
    }
}

quickcheck! {
    fn qc_add_associativity(a: u8, b: u8, c: u8) -> bool {
        add(a, add(b, c)) == add(add(a, b), c)
    }

    fn qc_mul_associativity(a: u8, b: u8, c: u8) -> bool {
        mul(a, mul(b, c)) == mul(mul(a, b), c)
    }
}

#[test]
fn test_identity() {
    for a in 0..256 {
        let a = a as u8;
        let b = sub(0, a);
        let c = sub(a, b);
        assert_eq!(c, 0);
        if a != 0 {
            let b = div(1, a);
            let c = mul(a, b);
            assert_eq!(c, 1);
        }
    }
}

quickcheck! {
    fn qc_additive_identity(a: u8) -> bool {
        sub(a, sub(0, a)) == 0
    }

    fn qc_multiplicative_identity(a: u8) -> bool {
        if a == 0 { true }
        else      { mul(a, div(1, a)) == 1 }
    }
}

#[test]
fn test_commutativity() {
    for a in 0..256 {
        let a = a as u8;
        for b in 0..256 {
            let b = b as u8;
            let x = add(a, b);
            let y = add(b, a);
            assert_eq!(x, y);
            let x = mul(a, b);
            let y = mul(b, a);
            assert_eq!(x, y);
        }
    }
}

quickcheck! {
    fn qc_add_commutativity(a: u8, b: u8) -> bool {
        add(a, b) == add(b, a)
    }

    fn qc_mul_commutativity(a: u8, b: u8) -> bool {
        mul(a, b) == mul(b, a)
    }
}

#[test]
fn test_distributivity() {
    for a in 0..256 {
        let a = a as u8;
        for b in 0..256 {
            let b = b as u8;
            for &c in &PROPERTY_TEST_VALUES {
                let x = mul(a, add(b, c));
                let y = add(mul(a, b), mul(a, c));
                assert_eq!(x, y);
            }
        }
    }
}

quickcheck! {
    fn qc_add_distributivity(a: u8, b: u8, c: u8) -> bool {
        mul(a, add(b, c)) == add(mul(a, b), mul(a, c))
    }
}

#[test]
fn test_exp() {
    for a in 0..256 {
        let a = a as u8;
        let mut power = 1u8;
        for j in 0..256 {
            let x = exp(a, j);
            assert_eq!(x, power);
            power = mul(power, a);
        }
    }
}

#[test]
fn test_galois() {
    assert_eq!(mul(3, 4), 12);
    assert_eq!(mul(7, 7), 21);
    assert_eq!(mul(23, 45), 41);

    let input = [
        0, 1, 2, 3, 4, 5, 6, 10, 50, 100, 150, 174, 201, 255, 99, 32, 67, 85, 200, 199, 198, 197,
        196, 195, 194, 193, 192, 191, 190, 189, 188, 187, 186, 185,
    ];
    let mut output1 = vec![0; input.len()];
    let mut output2 = vec![0; input.len()];
    mul_slice(25, &input, &mut output1);
    let expect = [
        0x0, 0x19, 0x32, 0x2b, 0x64, 0x7d, 0x56, 0xfa, 0xb8, 0x6d, 0xc7, 0x85, 0xc3, 0x1f, 0x22,
        0x7, 0x25, 0xfe, 0xda, 0x5d, 0x44, 0x6f, 0x76, 0x39, 0x20, 0xb, 0x12, 0x11, 0x8, 0x23,
        0x3a, 0x75, 0x6c, 0x47,
    ];
    for i in 0..input.len() {
        assert_eq!(expect[i], output1[i]);
    }
    mul_slice(25, &input, &mut output2);
    for i in 0..input.len() {
        assert_eq!(expect[i], output2[i]);
    }

    let expect_xor = [
        0x0, 0x2d, 0x5a, 0x77, 0xb4, 0x99, 0xee, 0x2f, 0x79, 0xf2, 0x7, 0x51, 0xd4, 0x19, 0x31,
        0xc9, 0xf8, 0xfc, 0xf9, 0x4f, 0x62, 0x15, 0x38, 0xfb, 0xd6, 0xa1, 0x8c, 0x96, 0xbb, 0xcc,
        0xe1, 0x22, 0xf, 0x78,
    ];
    mul_slice_xor(52, &input, &mut output1);
    for i in 0..input.len() {
        assert_eq!(expect_xor[i], output1[i]);
    }
    mul_slice_xor(52, &input, &mut output2);
    for i in 0..input.len() {
        assert_eq!(expect_xor[i], output2[i]);
    }

    let expect = [
        0x0, 0xb1, 0x7f, 0xce, 0xfe, 0x4f, 0x81, 0x9e, 0x3, 0x6, 0xe8, 0x75, 0xbd, 0x40, 0x36,
        0xa3, 0x95, 0xcb, 0xc, 0xdd, 0x6c, 0xa2, 0x13, 0x23, 0x92, 0x5c, 0xed, 0x1b, 0xaa, 0x64,
        0xd5, 0xe5, 0x54, 0x9a,
    ];
    mul_slice(177, &input, &mut output1);
    for i in 0..input.len() {
        assert_eq!(expect[i], output1[i]);
    }
    mul_slice(177, &input, &mut output2);
    for i in 0..input.len() {
        assert_eq!(expect[i], output2[i]);
    }

    let expect_xor = [
        0x0, 0xc4, 0x95, 0x51, 0x37, 0xf3, 0xa2, 0xfb, 0xec, 0xc5, 0xd0, 0xc7, 0x53, 0x88, 0xa3,
        0xa5, 0x6, 0x78, 0x97, 0x9f, 0x5b, 0xa, 0xce, 0xa8, 0x6c, 0x3d, 0xf9, 0xdf, 0x1b, 0x4a,
        0x8e, 0xe8, 0x2c, 0x7d,
    ];
    mul_slice_xor(117, &input, &mut output1);
    for i in 0..input.len() {
        assert_eq!(expect_xor[i], output1[i]);
    }
    mul_slice_xor(117, &input, &mut output2);
    for i in 0..input.len() {
        assert_eq!(expect_xor[i], output2[i]);
    }

    assert_eq!(exp(2, 2), 4);
    assert_eq!(exp(5, 20), 235);
    assert_eq!(exp(13, 7), 43);
}

#[test]
fn test_slice_add() {
    let length_list = [16, 32, 34];
    for len in length_list.iter() {
        let mut input = vec![0; *len];
        fill_random(&mut input);
        let mut output = vec![0; *len];
        fill_random(&mut output);
        let mut expect = vec![0; *len];
        for i in 0..expect.len() {
            expect[i] = input[i] ^ output[i];
        }
        scalar::slice_xor(&input, &mut output);
        for i in 0..expect.len() {
            assert_eq!(expect[i], output[i]);
        }
        fill_random(&mut output);
        for i in 0..expect.len() {
            expect[i] = input[i] ^ output[i];
        }
        scalar::slice_xor(&input, &mut output);
        for i in 0..expect.len() {
            assert_eq!(expect[i], output[i]);
        }
    }
}

#[test]
fn test_div_a_is_0() {
    assert_eq!(0, div(0, 100));
}

#[test]
fn test_div_b_is_0() {
    assert_eq!(0, div(1, 0));
}

#[test]
fn test_active_backend_matches_scalar_reference() {
    let lengths = [0usize, 1, 15, 16, 17, 31, 32, 33, 255, 1024, 10_003];
    for &len in &lengths {
        for _ in 0..16 {
            let c = rand::random::<u8>();
            let mut input = vec![0; len];
            fill_random(&mut input);

            let mut scalar = vec![0; len];
            let mut active = vec![0; len];
            mul_slice_scalar_for_test(c, &input, &mut scalar);
            mul_slice(c, &input, &mut active);
            assert_eq!(scalar, active, "mul_slice mismatch len={len} coeff={c}");

            fill_random(&mut scalar);
            let seed = scalar.clone();
            let mut active_xor = seed.clone();
            mul_slice_xor_scalar_for_test(c, &input, &mut scalar);
            mul_slice_xor(c, &input, &mut active_xor);
            assert_eq!(
                scalar, active_xor,
                "mul_slice_xor mismatch len={len} coeff={c}"
            );
        }
    }
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
#[test]
fn test_rust_neon_matches_scalar_mul_slice() {
    let lengths = [0usize, 1, 15, 16, 17, 31, 32, 33, 255, 1024, 10_003];
    for &len in &lengths {
        for _ in 0..16 {
            let c = rand::random::<u8>();
            let mut input = vec![0; len];
            fill_random(&mut input);
            let mut scalar = vec![0; len];
            let mut neon = vec![0; len];

            mul_slice_scalar_for_test(c, &input, &mut scalar);
            aarch64::neon::rust_neon_mul_slice(c, &input, &mut neon);

            assert_eq!(scalar, neon);
        }
    }
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
#[test]
fn test_rust_neon_mul_slice_xor_c1_vectorized_fastpath() {
    let lengths = [0usize, 1, 15, 16, 17, 31, 32, 33, 255, 1024, 10_003];
    for &len in &lengths {
        for _ in 0..16 {
            let mut input = vec![0; len];
            fill_random(&mut input);
            let mut scalar = vec![0; len];
            fill_random(&mut scalar);
            let mut neon = scalar.clone();

            mul_slice_xor_scalar_for_test(1, &input, &mut scalar);
            aarch64::neon::rust_neon_mul_slice_xor(1, &input, &mut neon);

            assert_eq!(scalar, neon);
        }
    }
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
#[test]
fn test_rust_neon_matches_scalar_mul_slice_xor() {
    let lengths = [0usize, 1, 15, 16, 17, 31, 32, 33, 255, 1024, 10_003];
    for &len in &lengths {
        for _ in 0..16 {
            let c = rand::random::<u8>();
            let mut input = vec![0; len];
            fill_random(&mut input);
            let mut scalar = vec![0; len];
            let mut neon = vec![0; len];
            fill_random(&mut scalar);
            neon.copy_from_slice(&scalar);

            mul_slice_xor_scalar_for_test(c, &input, &mut scalar);
            aarch64::neon::rust_neon_mul_slice_xor(c, &input, &mut neon);

            assert_eq!(scalar, neon);
        }
    }
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
#[test]
fn test_rust_neon_profile_stats_track_vector_vs_tail() {
    let c = 25u8;
    let mut input = vec![0u8; 65];
    fill_random(&mut input);
    let mut out = vec![0u8; 65];
    let mut out_xor = vec![0u8; 65];

    // Per-thread capture isolates this measurement from any concurrent test that
    // hits the shared global NEON metrics (see `profile::capture`), so the exact
    // per-call deltas below are deterministic under parallel `cargo test`.
    super::profile::capture::reset();
    aarch64::neon::rust_neon_mul_slice(c, &input, &mut out);
    aarch64::neon::rust_neon_mul_slice_xor(c, &input, &mut out_xor);
    let delta = super::profile::capture::snapshot();

    assert_eq!(1, delta.mul_calls);
    assert_eq!(1, delta.mul_xor_calls);
    assert_eq!(130, delta.total_bytes);
    assert_eq!(2, delta.vector_64b_chunks);
    assert_eq!(0, delta.vector_16b_chunks);
    assert_eq!(2, delta.tail_bytes);
    assert_eq!(2, delta.tail_calls);
    assert_eq!(16, delta.table_lookups);
}

#[cfg(feature = "std")]
#[test]
fn test_parse_rust_neon_xor_unroll() {
    assert_eq!(Some(2), parse_rust_neon_xor_unroll("2"));
    assert_eq!(Some(4), parse_rust_neon_xor_unroll("4"));
    assert_eq!(None, parse_rust_neon_xor_unroll("1"));
    assert_eq!(None, parse_rust_neon_xor_unroll("8"));
    assert_eq!(None, parse_rust_neon_xor_unroll("abc"));
}

#[cfg(feature = "std")]
#[test]
fn test_rust_neon_xor_schedule_env_constant() {
    assert_eq!(
        "RS_NEON_MUL_SLICE_XOR_SCHEDULE",
        RS_NEON_MUL_SLICE_XOR_SCHEDULE_ENV
    );
}

#[test]
fn test_active_backend_metadata() {
    // The production selection is immutable. A fresh process supplies an auto
    // environment without racing other tests or inheriting a cached override.
    #[cfg(all(
        feature = "std",
        any(target_arch = "x86_64", target_arch = "aarch64"),
        not(target_env = "msvc"),
        not(any(target_os = "android", target_os = "ios"))
    ))]
    if std::env::var("RSE_GALOIS8_METADATA_CHILD").as_deref() != Ok("1") {
        let output = backend_test_child("galois_8::tests::test_active_backend_metadata")
            .env("RSE_GALOIS8_METADATA_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
            "auto metadata child failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    #[cfg(all(
        any(
            feature = "simd-neon",
            feature = "simd-ssse3",
            feature = "simd-avx2",
            feature = "simd-avx512",
            feature = "simd-gfni"
        ),
        any(target_arch = "x86_64", target_arch = "aarch64"),
        not(target_env = "msvc"),
        not(any(target_os = "android", target_os = "ios"))
    ))]
    {
        #[cfg(all(feature = "std", target_arch = "x86_64"))]
        {
            let has_gfni = std::is_x86_feature_detected!("gfni");
            let has_avx512 = std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw");
            let has_avx2 = std::is_x86_feature_detected!("avx2");

            // GFNI backends are auto-selected with higher priority when available.
            if cfg!(rse_x86_gfni) && has_gfni && has_avx512 {
                assert_eq!(active_backend_name(), "rust-gfni-avx512");
                assert_eq!(active_backend_kind(), BackendKind::RustSimd);
            } else if cfg!(rse_x86_gfni) && has_gfni && has_avx2 {
                assert_eq!(active_backend_name(), "rust-gfni-avx2");
                assert_eq!(active_backend_kind(), BackendKind::RustSimd);
            } else if cfg!(rse_x86_avx2) && has_avx2 {
                assert_eq!(active_backend_name(), "rust-avx2");
                assert_eq!(active_backend_kind(), BackendKind::RustSimd);
            } else if cfg!(rse_x86_avx512) && has_avx512 {
                assert_eq!(active_backend_name(), "rust-avx512");
                assert_eq!(active_backend_kind(), BackendKind::RustSimd);
            } else if cfg!(rse_x86_ssse3) && std::is_x86_feature_detected!("ssse3") {
                assert_eq!(active_backend_name(), "rust-ssse3");
                assert_eq!(active_backend_kind(), BackendKind::RustSimd);
            } else {
                assert_eq!(active_backend_name(), "scalar-rust");
                assert_eq!(active_backend_kind(), BackendKind::Scalar);
            }
        }

        #[cfg(all(feature = "std", target_arch = "aarch64"))]
        {
            if cfg!(rse_aarch64_neon) {
                assert_eq!(active_backend_name(), "rust-neon");
                assert_eq!(active_backend_kind(), BackendKind::RustSimd);
            } else {
                assert_eq!(active_backend_name(), "scalar-rust");
                assert_eq!(active_backend_kind(), BackendKind::Scalar);
            }
        }

        #[cfg(not(feature = "std"))]
        {
            assert_eq!(active_backend_name(), "scalar-rust");
            assert_eq!(active_backend_kind(), BackendKind::Scalar);
        }
    }

    #[cfg(not(all(
        any(
            feature = "simd-neon",
            feature = "simd-ssse3",
            feature = "simd-avx2",
            feature = "simd-avx512",
            feature = "simd-gfni"
        ),
        any(target_arch = "x86_64", target_arch = "aarch64"),
        not(target_env = "msvc"),
        not(any(target_os = "android", target_os = "ios"))
    )))]
    {
        assert_eq!(active_backend_name(), "scalar-rust");
        assert_eq!(active_backend_kind(), BackendKind::Scalar);
    }
}

#[cfg(all(
    any(
        feature = "simd-neon",
        feature = "simd-ssse3",
        feature = "simd-avx2",
        feature = "simd-avx512",
        feature = "simd-gfni"
    ),
    feature = "std",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[test]
fn test_backend_override_affects_active_backend() {
    if std::env::var("RSE_GALOIS8_OVERRIDE_CHILD").as_deref() == Ok("1") {
        let expected = std::env::var("RSE_GALOIS8_EXPECTED_NAME").unwrap();
        let expected = (expected != "none").then_some(expected.as_str());
        let name = super::backend::runtime_override_backend_name_for_test();
        let id = super::backend::runtime_override_backend_id_for_test();
        assert_eq!(name, expected);
        assert_eq!(
            format!("{id:?}"),
            std::env::var("RSE_GALOIS8_EXPECTED_ID").unwrap()
        );
        // This process has not initialized the cached backend before its
        // environment was installed. Check the real selection as well.
        if let Some(name) = name {
            assert_eq!(active_backend_name(), name);
            assert_eq!(Some(active_backend_id()), id);
        }
        return;
    }

    assert_backend_override_in_child("scalar", Some("scalar-rust"), Some(BackendId::ScalarRust));
    assert_backend_override_in_child(
        "scalar-rust",
        Some("scalar-rust"),
        Some(BackendId::ScalarRust),
    );
    assert_backend_override_in_child("auto", None, None);
    assert_backend_override_in_child("unknown-backend", None, None);

    #[cfg(target_arch = "aarch64")]
    {
        let supported = cfg!(rse_aarch64_neon);
        assert_backend_override_in_child(
            "rust-neon",
            supported.then_some("rust-neon"),
            supported.then_some(BackendId::RustNeon),
        );
    }
    #[cfg(target_arch = "x86_64")]
    {
        let avx2 = std::is_x86_feature_detected!("avx2");
        let avx512 =
            std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw");
        let gfni = std::is_x86_feature_detected!("gfni");
        for (name, id, supported) in [
            (
                "rust-gfni-avx2",
                BackendId::RustGfniAvx2,
                cfg!(rse_x86_gfni) && gfni && avx2,
            ),
            (
                "rust-gfni-avx512",
                BackendId::RustGfniAvx512,
                cfg!(rse_x86_gfni) && gfni && avx512,
            ),
            (
                "rust-avx512",
                BackendId::RustAvx512,
                cfg!(rse_x86_avx512) && avx512,
            ),
            ("rust-avx2", BackendId::RustAvx2, cfg!(rse_x86_avx2) && avx2),
            (
                "rust-ssse3",
                BackendId::RustSsse3,
                cfg!(rse_x86_ssse3) && std::is_x86_feature_detected!("ssse3"),
            ),
        ] {
            assert_backend_override_in_child(
                name,
                supported.then_some(name),
                supported.then_some(id),
            );
        }
    }
}

#[cfg(all(
    feature = "simd-neon",
    feature = "std",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[test]
fn test_active_backend_metadata_fresh_process() {
    use std::process::Command;

    if std::env::var("RSE_GALOIS8_CHILD_CHECK").as_deref() == Ok("active-backend-metadata") {
        println!("child_backend={}", active_backend_name());
        println!("child_backend_kind={:?}", active_backend_kind());
        return;
    }

    let current_exe = std::env::current_exe().unwrap();
    let output = Command::new(current_exe)
        .env_remove("RSE_BACKEND_OVERRIDE")
        .env_remove("RSE_STRICT_BACKEND_OVERRIDE")
        .env("RSE_GALOIS8_CHILD_CHECK", "active-backend-metadata")
        .arg("--exact")
        .arg("galois_8::tests::test_active_backend_metadata_fresh_process")
        .arg("--nocapture")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "child active backend metadata check failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("child_backend=rust-neon"), "{stdout}");
    assert!(stdout.contains("child_backend_kind=RustSimd"), "{stdout}");
}

#[cfg(all(
    feature = "simd-neon",
    feature = "std",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[test]
fn test_aarch64_backend_override_metadata_matches_expected_ids() {
    assert_backend_override_in_child("scalar", Some("scalar-rust"), Some(BackendId::ScalarRust));
    assert_backend_override_in_child("rust-neon", Some("rust-neon"), Some(BackendId::RustNeon));
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
#[test]
fn test_x86_cross_backend_conformance_matrix() {
    let has_ssse3 = std::is_x86_feature_detected!("ssse3");
    let has_avx2 = std::is_x86_feature_detected!("avx2");
    let has_avx512 =
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw");
    let has_gfni = std::is_x86_feature_detected!("gfni") && has_avx2;

    let lengths = [
        0usize, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 255, 1024, 10_003,
    ];
    let coeffs = [0u8, 1, 2, 15, 16, 31, 127, 173, 255];

    for &len in &lengths {
        for &c in &coeffs {
            let mut input = vec![0; len];
            fill_random(&mut input);

            let mut scalar = vec![0; len];
            mul_slice_scalar_for_test(c, &input, &mut scalar);

            let mut scalar_xor = vec![0; len];
            fill_random(&mut scalar_xor);
            let xor_seed = scalar_xor.clone();
            mul_slice_xor_scalar_for_test(c, &input, &mut scalar_xor);

            if has_ssse3 {
                let mut ssse3 = vec![0; len];
                x86::ssse3::rust_ssse3_mul_slice(c, &input, &mut ssse3);
                assert_eq!(scalar, ssse3, "ssse3 mismatch len={len} coeff={c}");

                let mut ssse3_xor = xor_seed.clone();
                x86::ssse3::rust_ssse3_mul_slice_xor(c, &input, &mut ssse3_xor);
                assert_eq!(
                    scalar_xor, ssse3_xor,
                    "ssse3 xor mismatch len={len} coeff={c}"
                );
            }

            if has_avx2 {
                let mut avx2 = vec![0; len];
                x86::avx2::rust_avx2_mul_slice(c, &input, &mut avx2);
                assert_eq!(scalar, avx2, "avx2 mismatch len={len} coeff={c}");

                let mut avx2_xor = xor_seed.clone();
                x86::avx2::rust_avx2_mul_slice_xor(c, &input, &mut avx2_xor);
                assert_eq!(
                    scalar_xor, avx2_xor,
                    "avx2 xor mismatch len={len} coeff={c}"
                );
            }

            if has_avx512 {
                let mut avx512 = vec![0; len];
                x86::avx512::rust_avx512_mul_slice(c, &input, &mut avx512);
                assert_eq!(scalar, avx512, "avx512 mismatch len={len} coeff={c}");

                let mut avx512_xor = xor_seed.clone();
                x86::avx512::rust_avx512_mul_slice_xor(c, &input, &mut avx512_xor);
                assert_eq!(
                    scalar_xor, avx512_xor,
                    "avx512 xor mismatch len={len} coeff={c}"
                );
            }

            if has_gfni {
                let mut gfni = vec![0; len];
                x86::gfni::rust_gfni_avx2_mul_slice(c, &input, &mut gfni);
                assert_eq!(scalar, gfni, "gfni mismatch len={len} coeff={c}");

                let mut gfni_xor = xor_seed.clone();
                x86::gfni::rust_gfni_avx2_mul_slice_xor(c, &input, &mut gfni_xor);
                assert_eq!(
                    scalar_xor, gfni_xor,
                    "gfni xor mismatch len={len} coeff={c}"
                );
            }
        }
    }
}

#[test]
#[should_panic]
fn scalar_zero_multiply_rejects_short_input() {
    mul_slice_scalar_for_test(0, &[], &mut [0xa5]);
}

#[test]
#[should_panic]
fn scalar_zero_accumulate_rejects_short_output() {
    mul_slice_xor_scalar_for_test(0, &[0xa5], &mut []);
}

#[cfg(feature = "std")]
#[test]
fn byte_field_slice_lengths_are_checked_before_output_mutation() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let operations: [(&str, backend::MulSliceFn); 6] = [
        ("scalar multiply", mul_slice_scalar_for_test),
        ("scalar accumulate", mul_slice_xor_scalar_for_test),
        ("dispatched multiply", mul_slice),
        ("dispatched accumulate", mul_slice_xor),
        ("Field multiply", <Field as crate::Field>::mul_slice),
        ("Field accumulate", <Field as crate::Field>::mul_slice_add),
    ];
    for (name, operation) in operations {
        for coefficient in [0, 1, 2, 255] {
            for (input_len, output_len) in [(0, 1), (1, 0), (15, 16), (16, 15), (31, 32), (32, 31)]
            {
                let input = vec![0x5a; input_len];
                let mut output = vec![0xa5; output_len];
                let result = catch_unwind(AssertUnwindSafe(|| {
                    operation(coefficient, &input, &mut output)
                }));
                assert!(
                    result.is_err(),
                    "{name}, coefficient {coefficient}, {input_len}/{output_len}"
                );
                assert!(
                    output.iter().all(|&byte| byte == 0xa5),
                    "{name} mutated mismatched output"
                );
            }
        }
    }
}

#[cfg(feature = "simd-gfni")]
#[test]
fn gfni_linear_matrices_match_every_field_product() {
    for coefficient in 0..=u8::MAX {
        let matrix = GFNI_MULTIPLY_MATRICES[coefficient as usize] as u64;
        for input in 0..=u8::MAX {
            let mut actual = 0u8;
            for bit in 0..8 {
                let row = (matrix >> ((7 - bit) * 8)) as u8;
                actual |= (((row & input).count_ones() & 1) as u8) << bit;
            }
            assert_eq!(
                actual,
                mul(coefficient, input),
                "coefficient={coefficient}, input={input}"
            );
        }
    }
}
