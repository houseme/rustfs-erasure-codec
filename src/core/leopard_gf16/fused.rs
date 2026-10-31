//! Fused decode layout conversion and multiplication on selected targets.
//! Other targets retain the established conversion and multiplication paths.

use super::{LeopardGf16Tables, MODULUS16, ops};

#[cfg(all(
    rse_x86_gfni,
    feature = "std",
    target_arch = "x86_64",
    target_endian = "little"
))]
const GFNI_LAYOUT_MIN_BYTES: usize = 4096;

pub(super) fn user_to_work_mul(
    user: &[u8],
    out: &mut [u16],
    scratch: &mut [u16],
    log_m: u16,
    tables: &LeopardGf16Tables,
) {
    assert_eq!(user.len(), out.len() * 2);
    assert!(user.len().is_multiple_of(64));
    #[cfg(feature = "std")]
    assert_eq!(scratch.len(), out.len());
    #[cfg(not(feature = "std"))]
    let _ = &scratch;
    if log_m == MODULUS16 as u16 {
        ops::user_bytes_to_work_u16_into(user, out);
        return;
    }
    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    if std::is_x86_feature_detected!("avx2") {
        #[cfg(rse_x86_gfni)]
        if user.len() >= GFNI_LAYOUT_MIN_BYTES
            && let Some(matrices) = super::mul_simd::gfni_layout_matrices(log_m, tables)
        {
            // SAFETY: AVX2 and GFNI are detected; the validated slices are disjoint whole blocks.
            unsafe { super::avx2::user_to_work_gfni(user, out, matrices) };
            return;
        }
        let nibble =
            super::mul_simd::NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        // SAFETY: AVX2 is detected and both slices cover complete, disjoint blocks.
        unsafe { super::avx2::user_to_work(user, out, &nibble) };
        return;
    }
    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    if std::is_x86_feature_detected!("ssse3") {
        let nibble =
            super::mul_simd::NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        // SAFETY: SSSE3 is detected and both slices cover complete, disjoint blocks.
        unsafe { x86::user_to_work(user, out, &nibble) };
        return;
    }
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    {
        let nibble =
            super::mul_simd::NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        // SAFETY: NEON is aarch64 baseline; both slices cover the same number
        // of complete 64-byte blocks and do not overlap.
        unsafe { neon::user_to_work(user, out, &nibble) };
    }
    #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
    {
        #[cfg(not(feature = "std"))]
        scalar_user_to_work_mul(user, out, log_m, tables);
        #[cfg(feature = "std")]
        {
            ops::user_bytes_to_work_u16_into(user, scratch);
            ops::mulgf16(out, scratch, log_m, tables);
        }
    }
}

pub(super) fn work_mul_to_user(
    work: &[u16],
    user: &mut [u8],
    scratch: &mut [u16],
    log_m: u16,
    tables: &LeopardGf16Tables,
) {
    assert_eq!(user.len(), work.len() * 2);
    assert!(user.len().is_multiple_of(64));
    #[cfg(feature = "std")]
    assert_eq!(scratch.len(), work.len());
    #[cfg(not(feature = "std"))]
    let _ = &scratch;
    if log_m == MODULUS16 as u16 {
        ops::work_u16_to_user_bytes(work, user);
        return;
    }
    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    if std::is_x86_feature_detected!("avx2") {
        #[cfg(rse_x86_gfni)]
        if user.len() >= GFNI_LAYOUT_MIN_BYTES
            && let Some(matrices) = super::mul_simd::gfni_layout_matrices(log_m, tables)
        {
            // SAFETY: AVX2 and GFNI are detected; the validated slices are disjoint whole blocks.
            unsafe { super::avx2::work_to_user_gfni(work, user, matrices) };
            return;
        }
        let nibble =
            super::mul_simd::NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        // SAFETY: AVX2 is detected and both slices cover complete, disjoint blocks.
        unsafe { super::avx2::work_to_user(work, user, &nibble) };
        return;
    }
    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    if std::is_x86_feature_detected!("ssse3") {
        let nibble =
            super::mul_simd::NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        // SAFETY: SSSE3 is detected and both slices cover complete, disjoint blocks.
        unsafe { x86::work_to_user(work, user, &nibble) };
        return;
    }
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    {
        let nibble =
            super::mul_simd::NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
        // SAFETY: NEON is aarch64 baseline; both slices cover the same number
        // of complete 64-byte blocks and do not overlap.
        unsafe { neon::work_to_user(work, user, &nibble) };
    }
    #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
    {
        #[cfg(not(feature = "std"))]
        scalar_work_mul_to_user(work, user, log_m, tables);
        #[cfg(feature = "std")]
        {
            ops::mulgf16(scratch, work, log_m, tables);
            ops::work_u16_to_user_bytes(scratch, user);
        }
    }
}

// The non-NEON no_std path uses the same log/exp multiplication as the prior
// scalar implementation while processing each split-layout block once.
#[cfg(any(
    test,
    all(
        not(feature = "std"),
        not(all(target_arch = "aarch64", target_endian = "little"))
    )
))]
fn scalar_user_to_work_mul(user: &[u8], out: &mut [u16], log_m: u16, tables: &LeopardGf16Tables) {
    let (blocks, _) = user.as_chunks::<64>();
    let (lanes, _) = out.as_chunks_mut::<32>();
    for (src, dst) in blocks.iter().zip(lanes) {
        for (i, value) in dst.iter_mut().enumerate() {
            let input = u16::from_le_bytes([src[i], src[i + 32]]);
            *value = ops::mul_log16(input, log_m, &tables.log_lut, &tables.exp_lut);
        }
    }
}

#[cfg(any(
    test,
    all(
        not(feature = "std"),
        not(all(target_arch = "aarch64", target_endian = "little"))
    )
))]
fn scalar_work_mul_to_user(work: &[u16], user: &mut [u8], log_m: u16, tables: &LeopardGf16Tables) {
    let (lanes, _) = work.as_chunks::<32>();
    let (blocks, _) = user.as_chunks_mut::<64>();
    for (src, dst) in lanes.iter().zip(blocks) {
        for (i, &value) in src.iter().enumerate() {
            let product = ops::mul_log16(value, log_m, &tables.log_lut, &tables.exp_lut);
            let [lo, hi] = product.to_le_bytes();
            dst[i] = lo;
            dst[i + 32] = hi;
        }
    }
}

#[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
mod x86 {
    use super::super::mul_simd::NibbleTables16;
    use core::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_loadu_si128, _mm_packus_epi16, _mm_set1_epi8, _mm_shuffle_epi8,
        _mm_srli_epi16, _mm_storeu_si128, _mm_unpackhi_epi8, _mm_unpacklo_epi8, _mm_xor_si128,
    };

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn multiply(
        lo: __m128i,
        hi: __m128i,
        l: &[__m128i; 4],
        h: &[__m128i; 4],
    ) -> (__m128i, __m128i) {
        let mask = _mm_set1_epi8(15);
        let n = [
            _mm_and_si128(lo, mask),
            _mm_and_si128(_mm_srli_epi16::<4>(lo), mask),
            _mm_and_si128(hi, mask),
            _mm_and_si128(_mm_srli_epi16::<4>(hi), mask),
        ];
        let product = |t: &[__m128i; 4]| {
            _mm_xor_si128(
                _mm_xor_si128(_mm_shuffle_epi8(t[0], n[0]), _mm_shuffle_epi8(t[1], n[1])),
                _mm_xor_si128(_mm_shuffle_epi8(t[2], n[2]), _mm_shuffle_epi8(t[3], n[3])),
            )
        };
        (product(l), product(h))
    }

    #[target_feature(enable = "ssse3")]
    pub(super) unsafe fn user_to_work(user: &[u8], out: &mut [u16], table: &NibbleTables16) {
        // SAFETY: caller provides SSSE3, equal disjoint slices and complete 64-byte
        // blocks; every unaligned load and store stays within its block.
        unsafe {
            let l = [
                _mm_loadu_si128(table.lo[0].as_ptr().cast()),
                _mm_loadu_si128(table.lo[1].as_ptr().cast()),
                _mm_loadu_si128(table.lo[2].as_ptr().cast()),
                _mm_loadu_si128(table.lo[3].as_ptr().cast()),
            ];
            let h = [
                _mm_loadu_si128(table.hi[0].as_ptr().cast()),
                _mm_loadu_si128(table.hi[1].as_ptr().cast()),
                _mm_loadu_si128(table.hi[2].as_ptr().cast()),
                _mm_loadu_si128(table.hi[3].as_ptr().cast()),
            ];
            let dst = out.as_mut_ptr().cast::<u8>();
            for off in (0..user.len()).step_by(64) {
                for half in [0, 16] {
                    let (lo, hi) = multiply(
                        _mm_loadu_si128(user.as_ptr().add(off + half).cast()),
                        _mm_loadu_si128(user.as_ptr().add(off + 32 + half).cast()),
                        &l,
                        &h,
                    );
                    _mm_storeu_si128(dst.add(off + half * 2).cast(), _mm_unpacklo_epi8(lo, hi));
                    _mm_storeu_si128(
                        dst.add(off + half * 2 + 16).cast(),
                        _mm_unpackhi_epi8(lo, hi),
                    );
                }
            }
        }
    }

    #[target_feature(enable = "ssse3")]
    pub(super) unsafe fn work_to_user(work: &[u16], user: &mut [u8], table: &NibbleTables16) {
        // SAFETY: caller provides SSSE3, equal disjoint slices and complete 64-byte
        // blocks; every unaligned load and store stays within its block.
        unsafe {
            let l = [
                _mm_loadu_si128(table.lo[0].as_ptr().cast()),
                _mm_loadu_si128(table.lo[1].as_ptr().cast()),
                _mm_loadu_si128(table.lo[2].as_ptr().cast()),
                _mm_loadu_si128(table.lo[3].as_ptr().cast()),
            ];
            let h = [
                _mm_loadu_si128(table.hi[0].as_ptr().cast()),
                _mm_loadu_si128(table.hi[1].as_ptr().cast()),
                _mm_loadu_si128(table.hi[2].as_ptr().cast()),
                _mm_loadu_si128(table.hi[3].as_ptr().cast()),
            ];
            let byte_mask = _mm_srli_epi16::<8>(_mm_set1_epi8(-1));
            let src = work.as_ptr().cast::<u8>();
            for off in (0..user.len()).step_by(64) {
                for half in [0, 16] {
                    let a = _mm_loadu_si128(src.add(off + half * 2).cast());
                    let b = _mm_loadu_si128(src.add(off + half * 2 + 16).cast());
                    let lo =
                        _mm_packus_epi16(_mm_and_si128(a, byte_mask), _mm_and_si128(b, byte_mask));
                    let hi = _mm_packus_epi16(_mm_srli_epi16::<8>(a), _mm_srli_epi16::<8>(b));
                    let (lo, hi) = multiply(lo, hi, &l, &h);
                    _mm_storeu_si128(user.as_mut_ptr().add(off + half).cast(), lo);
                    _mm_storeu_si128(user.as_mut_ptr().add(off + 32 + half).cast(), hi);
                }
            }
        }
    }
}

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod neon {
    use super::super::mul_simd::NibbleTables16;
    use core::arch::aarch64::{
        uint8x16_t, vandq_u8, vdupq_n_u8, veorq_u8, vld1q_u8, vqtbl1q_u8, vshrq_n_u8, vst1q_u8,
        vuzp1q_u8, vuzp2q_u8, vzip1q_u8, vzip2q_u8,
    };

    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn multiply(
        lo: uint8x16_t,
        hi: uint8x16_t,
        l: &[uint8x16_t; 4],
        h: &[uint8x16_t; 4],
    ) -> (uint8x16_t, uint8x16_t) {
        let mask = vdupq_n_u8(15);
        let n = [
            vandq_u8(lo, mask),
            vshrq_n_u8::<4>(lo),
            vandq_u8(hi, mask),
            vshrq_n_u8::<4>(hi),
        ];
        let product = |t: &[uint8x16_t; 4]| {
            veorq_u8(
                veorq_u8(vqtbl1q_u8(t[0], n[0]), vqtbl1q_u8(t[1], n[1])),
                veorq_u8(vqtbl1q_u8(t[2], n[2]), vqtbl1q_u8(t[3], n[3])),
            )
        };
        (product(l), product(h))
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn user_to_work(user: &[u8], out: &mut [u16], table: &NibbleTables16) {
        // SAFETY: caller validates equal lengths and complete 64-byte blocks;
        // each load and store is within its block, and u16 storage admits u8 views.
        unsafe {
            let l = [
                vld1q_u8(table.lo[0].as_ptr()),
                vld1q_u8(table.lo[1].as_ptr()),
                vld1q_u8(table.lo[2].as_ptr()),
                vld1q_u8(table.lo[3].as_ptr()),
            ];
            let h = [
                vld1q_u8(table.hi[0].as_ptr()),
                vld1q_u8(table.hi[1].as_ptr()),
                vld1q_u8(table.hi[2].as_ptr()),
                vld1q_u8(table.hi[3].as_ptr()),
            ];
            let dst = out.as_mut_ptr().cast::<u8>();
            for off in (0..user.len()).step_by(64) {
                for half in [0, 16] {
                    let (lo, hi) = multiply(
                        vld1q_u8(user.as_ptr().add(off + half)),
                        vld1q_u8(user.as_ptr().add(off + 32 + half)),
                        &l,
                        &h,
                    );
                    vst1q_u8(dst.add(off + half * 2), vzip1q_u8(lo, hi));
                    vst1q_u8(dst.add(off + half * 2 + 16), vzip2q_u8(lo, hi));
                }
            }
        }
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn work_to_user(work: &[u16], user: &mut [u8], table: &NibbleTables16) {
        // SAFETY: caller validates equal lengths and complete 64-byte blocks;
        // each load and store is within its block, and u16 storage admits u8 views.
        unsafe {
            let l = [
                vld1q_u8(table.lo[0].as_ptr()),
                vld1q_u8(table.lo[1].as_ptr()),
                vld1q_u8(table.lo[2].as_ptr()),
                vld1q_u8(table.lo[3].as_ptr()),
            ];
            let h = [
                vld1q_u8(table.hi[0].as_ptr()),
                vld1q_u8(table.hi[1].as_ptr()),
                vld1q_u8(table.hi[2].as_ptr()),
                vld1q_u8(table.hi[3].as_ptr()),
            ];
            let src = work.as_ptr().cast::<u8>();
            for off in (0..user.len()).step_by(64) {
                for half in [0, 16] {
                    let a = vld1q_u8(src.add(off + half * 2));
                    let b = vld1q_u8(src.add(off + half * 2 + 16));
                    let (lo, hi) = multiply(vuzp1q_u8(a, b), vuzp2q_u8(a, b), &l, &h);
                    vst1q_u8(user.as_mut_ptr().add(off + half), lo);
                    vst1q_u8(user.as_mut_ptr().add(off + 32 + half), hi);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::super::init_leopard_gf16_tables;
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[cfg(not(feature = "std"))]
    #[test]
    fn no_std_fused_paths_accept_empty_scratch() {
        let tables = init_leopard_gf16_tables();
        let input = vec![7u8; 4096];
        let mut work = vec![0u16; 2048];
        let mut output = vec![0u8; 4096];
        let mut scratch = [];
        user_to_work_mul(&input, &mut work, &mut scratch, 12345, tables);
        work_mul_to_user(&work, &mut output, &mut scratch, 12345, tables);
        let mut expected = vec![0u16; 2048];
        scalar_user_to_work_mul(&input, &mut expected, 12345, tables);
        assert_eq!(work, expected);
        let mut expected_output = vec![0u8; 4096];
        scalar_work_mul_to_user(&expected, &mut expected_output, 12345, tables);
        assert_eq!(output, expected_output);
    }

    #[test]
    fn fused_layout_multiply_matches_separate_operations() {
        #[cfg(all(
            rse_x86_gfni,
            feature = "std",
            target_arch = "x86_64",
            target_endian = "little"
        ))]
        let _ = crate::galois_8::active_backend_id();
        let tables = init_leopard_gf16_tables();
        for len in [0, 64, 128, 192, 4032, 4096, 4160, 32_768, 65_536, 65_600] {
            let bytes: Vec<u8> = (0..len + 1)
                .map(|i| ((i * 29 + i / 7) % 256) as u8)
                .collect();
            let user = &bytes[1..]; // exercise an unaligned input
            let work: Vec<u16> = (0..len / 2)
                .map(|i| ((i * 1237 + i / 3) % 65536) as u16)
                .collect();
            for log_m in [0, 1, 2, 255, 1024, 32768, 65534, 65535] {
                let mut scratch = vec![0; len / 2];
                let mut converted = vec![0; len / 2];
                let mut expected_work = vec![0; len / 2];
                let mut actual_work = vec![0; len / 2];
                ops::user_bytes_to_work_u16_into(user, &mut converted);
                ops::mulgf16(&mut expected_work, &converted, log_m, tables);
                user_to_work_mul(user, &mut actual_work, &mut scratch, log_m, tables);
                assert_eq!(actual_work, expected_work, "step 1: len={len}, log={log_m}");

                let mut expected_bytes = vec![0; len];
                let mut actual_bytes = vec![0; len + 1];
                ops::mulgf16(&mut converted, &work, log_m, tables);
                ops::work_u16_to_user_bytes(&converted, &mut expected_bytes);
                work_mul_to_user(&work, &mut actual_bytes[1..], &mut scratch, log_m, tables);
                assert_eq!(
                    &actual_bytes[1..],
                    &expected_bytes,
                    "step 5: len={len}, log={log_m}"
                );
            }
        }
    }

    #[test]
    fn fused_kernels_cover_every_field_element() {
        let tables = init_leopard_gf16_tables();
        let values: Vec<u16> = (0..=u16::MAX).collect();
        let mut user = vec![0; values.len() * 2];
        ops::work_u16_to_user_bytes(&values, &mut user);
        for log_m in [0, 1, 31, 2048, 32768, 65534, 65535] {
            let mut expected = vec![0; values.len()];
            let mut actual = vec![0; values.len()];
            let mut scratch = vec![0; values.len()];
            ops::mulgf16(&mut expected, &values, log_m, tables);
            user_to_work_mul(&user, &mut actual, &mut scratch, log_m, tables);
            assert_eq!(actual, expected, "input conversion, log={log_m}");
            let mut expected_user = vec![0; user.len()];
            let mut actual_user = vec![0; user.len()];
            ops::work_u16_to_user_bytes(&expected, &mut expected_user);
            work_mul_to_user(&values, &mut actual_user, &mut scratch, log_m, tables);
            assert_eq!(actual_user, expected_user, "output conversion, log={log_m}");
        }
    }

    #[test]
    fn scalar_fused_kernels_match_reference_for_every_field_element() {
        let tables = init_leopard_gf16_tables();
        let work: Vec<u16> = (0..=u16::MAX).collect();
        let mut user = vec![0; work.len() * 2 + 1];
        ops::work_u16_to_user_bytes(&work, &mut user[1..]);
        for log_m in [0, 1, 31, 2048, 32768, 65534] {
            let mut expected = vec![0; work.len()];
            let mut actual = vec![0; work.len()];
            ops::mulgf16(&mut expected, &work, log_m, tables);
            scalar_user_to_work_mul(&user[1..], &mut actual, log_m, tables);
            assert_eq!(actual, expected, "scalar input, log={log_m}");
            let mut expected_user = vec![0; work.len() * 2];
            let mut actual_user = vec![0; work.len() * 2 + 1];
            ops::work_u16_to_user_bytes(&expected, &mut expected_user);
            scalar_work_mul_to_user(&work, &mut actual_user[1..], log_m, tables);
            assert_eq!(
                &actual_user[1..],
                &expected_user,
                "scalar output, log={log_m}"
            );
        }
    }

    #[test]
    fn fused_kernels_match_reference_for_every_multiplier() {
        let tables = init_leopard_gf16_tables();
        let mut work = [0u16; 32];
        for (i, value) in work.iter_mut().enumerate() {
            *value = (i as u16).wrapping_mul(2_113).rotate_left((i % 16) as u32);
        }
        let mut user = [0u8; 64];
        ops::work_u16_to_user_bytes(&work, &mut user);
        let mut expected = [0u16; 32];
        let mut actual = [0u16; 32];
        let mut scratch = [0u16; 32];
        let mut expected_user = [0u8; 64];
        let mut actual_user = [0u8; 64];
        for log_m in 0..=u16::MAX {
            ops::mulgf16(&mut expected, &work, log_m, tables);
            user_to_work_mul(&user, &mut actual, &mut scratch, log_m, tables);
            assert_eq!(actual, expected, "input conversion, log={log_m}");
            ops::work_u16_to_user_bytes(&expected, &mut expected_user);
            work_mul_to_user(&work, &mut actual_user, &mut scratch, log_m, tables);
            assert_eq!(actual_user, expected_user, "output conversion, log={log_m}");
        }
    }

    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    #[test]
    fn ssse3_kernels_match_reference_independently_of_dispatch() {
        let ssse3 = std::is_x86_feature_detected!("ssse3");
        let avx2 = std::is_x86_feature_detected!("avx2");
        std::println!("SSSE3={ssse3}, AVX2={avx2}");
        if !ssse3 {
            return;
        }
        let tables = init_leopard_gf16_tables();
        let work: Vec<u16> = (0..=u16::MAX).collect();
        let mut user = vec![0; work.len() * 2 + 1];
        ops::work_u16_to_user_bytes(&work, &mut user[1..]);
        for log_m in [0, 1, 31, 2048, 32768, 65534] {
            let nibble = super::super::mul_simd::NibbleTables16::build(
                log_m,
                &tables.log_lut,
                &tables.exp_lut,
            );
            let mut expected = vec![0; work.len()];
            let mut actual = vec![0; work.len()];
            ops::mulgf16(&mut expected, &work, log_m, tables);
            // SAFETY: SSSE3 was detected; the slices are disjoint and contain
            // the same whole number of 64-byte blocks.
            unsafe { super::x86::user_to_work(&user[1..], &mut actual, &nibble) };
            assert_eq!(actual, expected, "SSSE3 input, log={log_m}");
            let mut expected_user = vec![0; work.len() * 2];
            let mut actual_user = vec![0; work.len() * 2 + 1];
            ops::work_u16_to_user_bytes(&expected, &mut expected_user);
            // SAFETY: SSSE3 was detected; the slices are disjoint and contain
            // the same whole number of 64-byte blocks.
            unsafe { super::x86::work_to_user(&work, &mut actual_user[1..], &nibble) };
            assert_eq!(
                &actual_user[1..],
                &expected_user,
                "SSSE3 output, log={log_m}"
            );
        }
    }
}
