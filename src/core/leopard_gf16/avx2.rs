//! AVX2 GF16 decode layout conversion and multiplication.
use super::mul_simd::NibbleTables16;
use core::arch::x86_64::{
    __m256i, _mm_loadu_si128, _mm256_and_si256, _mm256_broadcastsi128_si256, _mm256_loadu_si256,
    _mm256_packus_epi16, _mm256_permute2x128_si256, _mm256_permute4x64_epi64, _mm256_set1_epi8,
    _mm256_shuffle_epi8, _mm256_srli_epi16, _mm256_storeu_si256, _mm256_unpackhi_epi8,
    _mm256_unpacklo_epi8, _mm256_xor_si256,
};

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn multiply(
    lo: __m256i,
    hi: __m256i,
    l: &[__m256i; 4],
    h: &[__m256i; 4],
) -> (__m256i, __m256i) {
    let mask = _mm256_set1_epi8(15);
    let n = [
        _mm256_and_si256(lo, mask),
        _mm256_and_si256(_mm256_srli_epi16::<4>(lo), mask),
        _mm256_and_si256(hi, mask),
        _mm256_and_si256(_mm256_srli_epi16::<4>(hi), mask),
    ];
    let product = |t: &[__m256i; 4]| {
        _mm256_xor_si256(
            _mm256_xor_si256(
                _mm256_shuffle_epi8(t[0], n[0]),
                _mm256_shuffle_epi8(t[1], n[1]),
            ),
            _mm256_xor_si256(
                _mm256_shuffle_epi8(t[2], n[2]),
                _mm256_shuffle_epi8(t[3], n[3]),
            ),
        )
    };
    (product(l), product(h))
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load_tables(table: &NibbleTables16) -> ([__m256i; 4], [__m256i; 4]) {
    // SAFETY: Every table row has 16 bytes; unaligned loads and broadcasts
    // are valid after the caller has verified AVX2.
    unsafe {
        let lo = core::array::from_fn(|i| {
            _mm256_broadcastsi128_si256(_mm_loadu_si128(table.lo[i].as_ptr().cast()))
        });
        let hi = core::array::from_fn(|i| {
            _mm256_broadcastsi128_si256(_mm_loadu_si128(table.hi[i].as_ptr().cast()))
        });
        (lo, hi)
    }
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn user_to_work(user: &[u8], out: &mut [u16], table: &NibbleTables16) {
    // SAFETY: Caller validates AVX2, disjoint equal-sized slices, and whole
    // 64-byte blocks. Each unaligned read and write stays within one block.
    unsafe {
        let (l, h) = load_tables(table);
        let dst = out.as_mut_ptr().cast::<u8>();
        for off in (0..user.len()).step_by(64) {
            let lo = _mm256_loadu_si256(user.as_ptr().add(off).cast());
            let hi = _mm256_loadu_si256(user.as_ptr().add(off + 32).cast());
            let (lo, hi) = multiply(lo, hi, &l, &h);
            let a = _mm256_unpacklo_epi8(lo, hi);
            let b = _mm256_unpackhi_epi8(lo, hi);
            _mm256_storeu_si256(dst.add(off).cast(), _mm256_permute2x128_si256::<0x20>(a, b));
            _mm256_storeu_si256(
                dst.add(off + 32).cast(),
                _mm256_permute2x128_si256::<0x31>(a, b),
            );
        }
    }
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn work_to_user(work: &[u16], user: &mut [u8], table: &NibbleTables16) {
    // SAFETY: Caller validates AVX2, disjoint equal-sized slices, and whole
    // 64-byte blocks. Each unaligned read and write stays within one block.
    unsafe {
        let (l, h) = load_tables(table);
        let byte_mask = _mm256_srli_epi16::<8>(_mm256_set1_epi8(-1));
        let src = work.as_ptr().cast::<u8>();
        for off in (0..user.len()).step_by(64) {
            let a = _mm256_loadu_si256(src.add(off).cast());
            let b = _mm256_loadu_si256(src.add(off + 32).cast());
            let lo = _mm256_permute4x64_epi64::<0xd8>(_mm256_packus_epi16(
                _mm256_and_si256(a, byte_mask),
                _mm256_and_si256(b, byte_mask),
            ));
            let hi = _mm256_permute4x64_epi64::<0xd8>(_mm256_packus_epi16(
                _mm256_srli_epi16::<8>(a),
                _mm256_srli_epi16::<8>(b),
            ));
            let (lo, hi) = multiply(lo, hi, &l, &h);
            _mm256_storeu_si256(user.as_mut_ptr().add(off).cast(), lo);
            _mm256_storeu_si256(user.as_mut_ptr().add(off + 32).cast(), hi);
        }
    }
}

#[test]
fn avx2_kernels_match_independent_reference() {
    extern crate alloc;
    use super::{init_leopard_gf16_tables, ops};
    use alloc::vec;
    use alloc::vec::Vec;
    if !std::is_x86_feature_detected!("avx2") {
        std::println!("AVX2 unavailable: kernels were not executed");
        return;
    }
    std::println!("AVX2=true: kernels executed");
    let tables = init_leopard_gf16_tables();
    let work: Vec<u16> = (0..=u16::MAX).collect();
    let mut user = vec![0; work.len() * 2 + 1];
    ops::work_u16_to_user_bytes(&work, &mut user[1..]);
    for log_m in [0, 1, 31, 2048, 32768, 65534] {
        let nibble = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        let mut expected = vec![0; work.len()];
        let mut actual = vec![0; work.len()];
        ops::mulgf16(&mut expected, &work, log_m, tables);
        // SAFETY: AVX2 was detected; the slices are disjoint whole blocks.
        unsafe { user_to_work(&user[1..], &mut actual, &nibble) };
        assert_eq!(actual, expected, "AVX2 input, log={log_m}");
        let mut expected_user = vec![0; work.len() * 2];
        let mut actual_user = vec![0; work.len() * 2 + 1];
        ops::work_u16_to_user_bytes(&expected, &mut expected_user);
        // SAFETY: AVX2 was detected; the slices are disjoint whole blocks.
        unsafe { work_to_user(&work, &mut actual_user[1..], &nibble) };
        assert_eq!(
            &actual_user[1..],
            &expected_user,
            "AVX2 output, log={log_m}"
        );
    }
}

#[test]
fn avx2_kernels_match_every_multiplier_and_boundaries() {
    extern crate alloc;
    use super::{init_leopard_gf16_tables, ops};
    use alloc::vec;
    use alloc::vec::Vec;
    if !std::is_x86_feature_detected!("avx2") {
        std::println!("AVX2 unavailable: kernels were not executed");
        return;
    }
    std::println!("AVX2=true: kernels executed");
    let tables = init_leopard_gf16_tables();
    let mut work = [0u16; 32];
    for (i, value) in work.iter_mut().enumerate() {
        *value = (i as u16).wrapping_mul(2_113).rotate_left((i % 16) as u32);
    }
    let mut user = [0u8; 64];
    ops::work_u16_to_user_bytes(&work, &mut user);
    let mut expected = [0u16; 32];
    let mut actual = [0u16; 32];
    let mut expected_user = [0u8; 64];
    let mut actual_user = [0u8; 64];
    // MODULUS16 is the caller's identity fast path, not a nibble-table input.
    for log_m in 0..u16::MAX {
        let nibble = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        ops::mulgf16(&mut expected, &work, log_m, tables);
        // SAFETY: AVX2 was detected; slices are disjoint whole blocks.
        unsafe { user_to_work(&user, &mut actual, &nibble) };
        assert_eq!(actual, expected, "AVX2 input, log={log_m}");
        ops::work_u16_to_user_bytes(&expected, &mut expected_user);
        // SAFETY: AVX2 was detected; slices are disjoint whole blocks.
        unsafe { work_to_user(&work, &mut actual_user, &nibble) };
        assert_eq!(actual_user, expected_user, "AVX2 output, log={log_m}");
    }
    for len in [0, 64, 128, 192, 32_768, 65_536, 65_600] {
        let bytes: Vec<u8> = (0..len + 1)
            .map(|i| ((i * 29 + i / 7) % 256) as u8)
            .collect();
        let mut converted = vec![0u16; len / 2];
        let mut expected = vec![0u16; len / 2];
        let mut actual = vec![0u16; len / 2];
        let nibble = NibbleTables16::build(12345, &tables.log_lut, &tables.exp_lut);
        ops::user_bytes_to_work_u16_into(&bytes[1..], &mut converted);
        ops::mulgf16(&mut expected, &converted, 12345, tables);
        // SAFETY: AVX2 was detected; slices are disjoint whole blocks.
        unsafe { user_to_work(&bytes[1..], &mut actual, &nibble) };
        assert_eq!(actual, expected, "AVX2 boundary input, len={len}");
        let mut expected_user = vec![0u8; len];
        let mut actual_user = vec![0u8; len + 1];
        ops::work_u16_to_user_bytes(&expected, &mut expected_user);
        // SAFETY: AVX2 was detected; slices are disjoint whole blocks.
        unsafe { work_to_user(&converted, &mut actual_user[1..], &nibble) };
        assert_eq!(
            &actual_user[1..],
            &expected_user,
            "AVX2 boundary output, len={len}"
        );
    }
}
