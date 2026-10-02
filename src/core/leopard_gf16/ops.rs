extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use super::{BITWIDTH16, LeopardGf16Tables, MODULUS16, ORDER16};

/// GF(2^16) multiply using log/exp tables.
///
/// `log_b` is the log of the second operand (used when one operand's log is precomputed).
#[inline]
pub(super) fn mul_log16(
    a: u16,
    log_b: u16,
    log_lut: &[u16; ORDER16],
    exp_lut: &[u16; ORDER16 * 2],
) -> u16 {
    if a == 0 {
        return 0;
    }
    exp_lut[add_mod16(log_lut[a as usize], log_b) as usize]
}

/// GF(2^16) multiply: `a * b` using log/exp tables.
#[inline]
pub(super) fn gf16_mul(
    a: u16,
    b: u16,
    log_lut: &[u16; ORDER16],
    exp_lut: &[u16; ORDER16 * 2],
) -> u16 {
    if a == 0 || b == 0 {
        return 0;
    }
    exp_lut[add_mod16(log_lut[a as usize], log_lut[b as usize]) as usize]
}

/// Modular addition in GF(2^16) log domain: `(a + b) % 65535`.
///
/// Returns 65535 when the sum is exactly 65535 (matching Go's addMod).
#[inline]
pub(super) fn add_mod16(a: u16, b: u16) -> u16 {
    let sum = a as u32 + b as u32;
    (sum + (sum >> BITWIDTH16)) as u16
}

/// Modular subtraction in GF(2^16) log domain: `(a - b) % 65535`.
#[inline]
pub(super) fn sub_mod16(a: u16, b: u16) -> u16 {
    let dif = (a as u32).wrapping_sub(b as u32);
    (dif.wrapping_add(dif >> BITWIDTH16)) as u16
}

/// Multiply each element of `input` by `g^log_m` in GF(2^16), writing to `out`.
///
/// Large slices use the SIMD 4-nibble shuffle kernel (see [`super::mul_simd`]);
/// short slices use the 4x-unrolled scalar log/exp path, which also serves as
/// the SIMD remainder tail and the portable fallback.
#[inline]
pub(super) fn mulgf16(out: &mut [u16], input: &[u16], log_m: u16, tables: &LeopardGf16Tables) {
    debug_assert_eq!(input.len(), out.len());
    if log_m == MODULUS16 as u16 {
        out.copy_from_slice(input);
        return;
    }
    if super::mul_simd::should_use_simd(input.len()) {
        super::mul_simd::mulgf16_simd::<false>(out, input, log_m, tables);
        return;
    }
    let log_lut = &tables.log_lut;
    let exp_lut = &tables.exp_lut;
    let (chunks_in, tail_in) = input.as_chunks::<4>();
    let (chunks_out, tail_out) = out.as_chunks_mut::<4>();
    for (dst, src) in chunks_out.iter_mut().zip(chunks_in.iter()) {
        dst[0] = mul_log16(src[0], log_m, log_lut, exp_lut);
        dst[1] = mul_log16(src[1], log_m, log_lut, exp_lut);
        dst[2] = mul_log16(src[2], log_m, log_lut, exp_lut);
        dst[3] = mul_log16(src[3], log_m, log_lut, exp_lut);
    }
    for (dst, &src) in tail_out.iter_mut().zip(tail_in.iter()) {
        *dst = mul_log16(src, log_m, log_lut, exp_lut);
    }
}

/// Multiply each element of `input` by `g^log_m` and XOR into `out` (first arg).
/// Matches GF8 `lut_xor(dst, src, lut)` convention: first arg is modified.
///
/// 4x unrolled to create independent dependency chains, hiding LUT latency.
#[inline]
pub(super) fn mulgf16_xor(out: &mut [u16], input: &[u16], log_m: u16, tables: &LeopardGf16Tables) {
    debug_assert_eq!(input.len(), out.len());
    if log_m == MODULUS16 as u16 {
        slice_xor_u16(out, input);
        return;
    }
    if super::mul_simd::should_use_simd(input.len()) {
        super::mul_simd::mulgf16_simd::<true>(out, input, log_m, tables);
        return;
    }
    let log_lut = &tables.log_lut;
    let exp_lut = &tables.exp_lut;
    let (chunks_in, tail_in) = input.as_chunks::<4>();
    let (chunks_out, tail_out) = out.as_chunks_mut::<4>();
    for (dst, src) in chunks_out.iter_mut().zip(chunks_in.iter()) {
        dst[0] ^= mul_log16(src[0], log_m, log_lut, exp_lut);
        dst[1] ^= mul_log16(src[1], log_m, log_lut, exp_lut);
        dst[2] ^= mul_log16(src[2], log_m, log_lut, exp_lut);
        dst[3] ^= mul_log16(src[3], log_m, log_lut, exp_lut);
    }
    for (dst, &src) in tail_out.iter_mut().zip(tail_in.iter()) {
        *dst ^= mul_log16(src, log_m, log_lut, exp_lut);
    }
}

/// XOR two u16 slices: `dst[i] ^= src[i]`.
/// Matches GF8 `slice_xor(dst, src)` convention: first arg is modified.
#[inline]
#[allow(clippy::needless_return)]
pub(super) fn slice_xor_u16(dst: &mut [u16], src: &[u16]) {
    debug_assert_eq!(dst.len(), src.len());
    // SAFETY: u16 XOR is identical to u8 XOR at the byte level (endian-independent).
    // Reinterpret as u8 slices to leverage SIMD byte-XOR implementations.
    let byte_len = dst.len() * 2;
    // SAFETY: dst is a valid &mut [u16]; the u8 view spans exactly byte_len = dst.len()*2 in-bounds bytes and u16 alignment (>=2) satisfies u8.
    let dst_bytes =
        unsafe { core::slice::from_raw_parts_mut(dst.as_mut_ptr().cast::<u8>(), byte_len) };
    // SAFETY: src is a valid &[u16] of the same length as dst; the u8 view spans exactly byte_len in-bounds bytes with satisfied alignment.
    let src_bytes = unsafe { core::slice::from_raw_parts(src.as_ptr().cast::<u8>(), byte_len) };

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: is_x86_feature_detected!("avx2") confirmed AVX2 at runtime, so calling the #[target_feature(enable="avx2")] fn is sound; both byte views are valid, equal-length slices.
            unsafe {
                slice_xor_u16_avx2(dst_bytes, src_bytes);
            }
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is a mandatory baseline feature on aarch64, so calling the #[target_feature(enable="neon")] fn is always sound here; both byte views are valid, equal-length slices.
        unsafe {
            slice_xor_u16_neon(dst_bytes, src_bytes);
        }
        return;
    }

    #[cfg(not(target_arch = "aarch64"))]
    slice_xor_u16_u64(dst_bytes, src_bytes);
}

/// AVX2 SIMD XOR for u16 slices (reinterpreted as bytes): 32 bytes per iteration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn slice_xor_u16_avx2(dst: &mut [u8], src: &[u8]) {
    use core::arch::x86_64::{_mm256_loadu_si256, _mm256_storeu_si256, _mm256_xor_si256};

    let (dst32, dst_tail) = dst.as_chunks_mut::<32>();
    let (src32, src_tail) = src.as_chunks::<32>();

    for (d, s) in dst32.iter_mut().zip(src32.iter()) {
        // SAFETY: as_chunks::<32>() yields 32-byte src/dst arrays, so each unaligned 256-bit load/store touches exactly 32 valid in-bounds bytes.
        unsafe {
            let sv = _mm256_loadu_si256(s.as_ptr().cast());
            let dv = _mm256_loadu_si256(d.as_ptr().cast());
            _mm256_storeu_si256(d.as_mut_ptr().cast(), _mm256_xor_si256(dv, sv));
        }
    }

    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d ^= *s;
    }
}

/// NEON SIMD XOR for u16 slices (reinterpreted as bytes): 64 bytes per iteration.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn slice_xor_u16_neon(dst: &mut [u8], src: &[u8]) {
    use core::arch::aarch64::{
        uint8x16x4_t, veorq_u8, vld1q_u8, vld1q_u8_x4, vst1q_u8, vst1q_u8_x4,
    };

    let (dst64, dst_tail) = dst.as_chunks_mut::<64>();
    let (src64, src_tail) = src.as_chunks::<64>();

    for (d, s) in dst64.iter_mut().zip(src64.iter()) {
        // SAFETY: as_chunks::<64>() yields 64-byte src/dst arrays, so the four 128-bit loads/stores cover exactly 64 valid in-bounds bytes each.
        unsafe {
            let sv = vld1q_u8_x4(s.as_ptr());
            let dv = vld1q_u8_x4(d.as_ptr());
            vst1q_u8_x4(
                d.as_mut_ptr(),
                uint8x16x4_t(
                    veorq_u8(dv.0, sv.0),
                    veorq_u8(dv.1, sv.1),
                    veorq_u8(dv.2, sv.2),
                    veorq_u8(dv.3, sv.3),
                ),
            );
        }
    }

    let (dst16, dst_scalar) = dst_tail.as_chunks_mut::<16>();
    let (src16, src_scalar) = src_tail.as_chunks::<16>();
    for (d, s) in dst16.iter_mut().zip(src16.iter()) {
        // SAFETY: as_chunks::<16>() yields 16-byte src/dst arrays, so each 128-bit load/store touches exactly 16 valid in-bounds bytes.
        unsafe {
            let sv = vld1q_u8(s.as_ptr());
            let dv = vld1q_u8(d.as_ptr());
            vst1q_u8(d.as_mut_ptr(), veorq_u8(dv, sv));
        }
    }

    for (d, s) in dst_scalar.iter_mut().zip(src_scalar.iter()) {
        *d ^= *s;
    }
}

/// u64-block XOR fallback for u16 slices (reinterpreted as bytes): 64 bytes per iteration.
fn slice_xor_u16_u64(dst: &mut [u8], src: &[u8]) {
    let (dst64, dst_tail64) = dst.as_chunks_mut::<64>();
    let (src64, src_tail64) = src.as_chunks::<64>();

    for (d, s) in dst64.iter_mut().zip(src64.iter()) {
        for i in 0..8 {
            let off = i * 8;
            // SAFETY: off = i*8 with i in 0..8 inside a 64-byte chunk, so s[off..] holds >=8 in-bounds bytes; read_unaligned needs no alignment.
            let sv = unsafe { core::ptr::read_unaligned(s[off..].as_ptr().cast::<u64>()) };
            // SAFETY: d[off..] holds >=8 in-bounds bytes of the 64-byte chunk; read_unaligned needs no alignment.
            let dv = unsafe { core::ptr::read_unaligned(d[off..].as_ptr().cast::<u64>()) };
            // SAFETY: d[off..] holds >=8 in-bounds bytes and is uniquely borrowed; write_unaligned needs no alignment.
            unsafe {
                core::ptr::write_unaligned(d[off..].as_mut_ptr().cast::<u64>(), dv ^ sv);
            }
        }
    }

    let (dst8, dst_tail) = dst_tail64.as_chunks_mut::<8>();
    let (src8, src_tail) = src_tail64.as_chunks::<8>();
    for (d, s) in dst8.iter_mut().zip(src8.iter()) {
        let sv = u64::from_ne_bytes(*s);
        let dv = u64::from_ne_bytes(*d);
        *d = (dv ^ sv).to_ne_bytes();
    }

    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d ^= *s;
    }
}

/// FWHT (Fast Walsh-Hadamard Transform) for GF(2^16) log-domain values.
///
/// Same structure as Go's `fwht`: sequential radix-2 butterflies within each block.
#[allow(clippy::explicit_counter_loop)]
pub(super) fn fwht16(data: &mut [u16; ORDER16]) {
    let mut dist = 1usize;
    let mut dist4 = 4usize;
    while dist4 <= ORDER16 {
        let mut r = 0usize;
        while r < ORDER16 {
            let mut off = r;
            for _ in 0..dist {
                let (t0, t1) = fwht2_alt16(data[off], data[off + dist]);
                data[off] = t0;
                data[off + dist] = t1;
                let (t2, t3) = fwht2_alt16(data[off + dist * 2], data[off + dist * 3]);
                data[off + dist * 2] = t2;
                data[off + dist * 3] = t3;
                let (t0, t2) = fwht2_alt16(data[off], data[off + dist * 2]);
                data[off] = t0;
                data[off + dist * 2] = t2;
                let (t1, t3) = fwht2_alt16(data[off + dist], data[off + dist * 3]);
                data[off + dist] = t1;
                data[off + dist * 3] = t3;
                off += 1;
            }
            r += dist4;
        }
        dist = dist4;
        dist4 <<= 2;
    }
}

/// FWHT with mtrunc: outer loop limited to `m`, inner loop limited to mtrunc.
///
/// Matches Go's `fwht(data, m, mtrunc)` — sequential radix-2 butterflies within each block.
#[allow(clippy::explicit_counter_loop)]
pub(super) fn fwht16_mtrunc(data: &mut [u16], m: usize, mtrunc: usize) {
    debug_assert_eq!(data.len(), ORDER16);
    let mut dist = 1usize;
    let mut dist4 = 4usize;
    while dist4 <= m {
        let mut r = 0usize;
        while r < mtrunc {
            let mut off = r;
            for _ in 0..dist {
                let (t0, t1) = fwht2_alt16(data[off], data[off + dist]);
                data[off] = t0;
                data[off + dist] = t1;
                let (t2, t3) = fwht2_alt16(data[off + dist * 2], data[off + dist * 3]);
                data[off + dist * 2] = t2;
                data[off + dist * 3] = t3;
                let (t0, t2) = fwht2_alt16(data[off], data[off + dist * 2]);
                data[off] = t0;
                data[off + dist * 2] = t2;
                let (t1, t3) = fwht2_alt16(data[off + dist], data[off + dist * 3]);
                data[off + dist] = t1;
                data[off + dist * 3] = t3;
                off += 1;
            }
            r += dist4;
        }
        dist = dist4;
        dist4 <<= 2;
    }
}

/// Flexible-size FWHT for slices whose length is a power of 2 and <= ORDER16.
///
/// Matches Go's `fwht(data, len)` — sequential radix-2 butterflies within each block.
#[allow(clippy::explicit_counter_loop)]
pub(super) fn fwht16_variable(data: &mut [u16]) {
    let n = data.len();
    debug_assert!(n.is_power_of_two());
    debug_assert!(n <= ORDER16);

    let mut dist = 1usize;
    while dist < n {
        let dist4 = dist * 4;
        if dist4 <= n {
            let mut r = 0usize;
            while r < n {
                super::fwht_simd::radix4_block(data, r, dist);
                r += dist4;
            }
            dist = dist4;
        } else {
            let dist2 = dist * 2;
            if dist2 <= n {
                let mut r = 0usize;
                while r < n {
                    super::fwht_simd::radix2_block(data, r, dist);
                    r += dist2;
                }
            }
            break;
        }
    }
}

#[inline]
pub(super) fn fwht2_alt16(a: u16, b: u16) -> (u16, u16) {
    (add_mod16(a, b), sub_mod16(a, b))
}

#[cfg(test)]
#[inline]
pub(super) fn fwht2_alt16_test(a: u16, b: u16) -> (u16, u16) {
    fwht2_alt16(a, b)
}

/// Forward butterfly step (FFT): `dst ^= mul(src, g^log_m); src ^= dst`.
/// Matches Go's `fftDIT2` behavior and `sliceXor` shortcut for modulus.
///
/// When `log_m == MODULUS16`, Go uses `sliceXor(work[a], work[c])` which modifies
/// the second argument: `src ^= dst`. The general path gives the same final result
/// via `dst ^= src*g^m; src ^= dst`, but the modulus shortcut must match exactly.
#[inline]
pub(super) fn dit2_step16(
    dst: &mut [u16],
    src: &mut [u16],
    log_m: u16,
    tables: &LeopardGf16Tables,
) {
    debug_assert_eq!(dst.len(), src.len());
    if log_m == MODULUS16 as u16 {
        slice_xor_u16(src, dst);
    } else {
        mulgf16_xor(dst, src, log_m, tables);
        slice_xor_u16(src, dst);
    }
}

/// Inverse butterfly step (IFFT): `src ^= dst; dst ^= mul(src, g^log_m)`.
/// Matches Go's `ifftDIT2` and `sliceXor` shortcut for modulus.
///
/// When `log_m == MODULUS16`, Go uses `sliceXor(work[a], work[b])` which modifies
/// the second argument only: `src ^= dst`. The general path gives a different result
/// because it also modifies `dst`, but Go's shortcut must be matched exactly.
#[inline]
pub(super) fn dit2_step_inv16(
    dst: &mut [u16],
    src: &mut [u16],
    log_m: u16,
    tables: &LeopardGf16Tables,
) {
    debug_assert_eq!(dst.len(), src.len());
    if log_m == MODULUS16 as u16 {
        slice_xor_u16(src, dst);
    } else {
        slice_xor_u16(src, dst);
        mulgf16_xor(dst, src, log_m, tables);
    }
}

/// Forward radix-2 FFT butterfly: `x ^= mul(y, m); y ^= x`.
/// Matches Go's `fftDIT2` and `sliceXor` shortcut for modulus.
///
/// When `log_m == MODULUS16`, Go uses `sliceXor(work[r], work[r+1])` which modifies
/// the second argument: `y ^= x`.
pub(super) fn fft_dit2_16(x: &mut [u16], y: &mut [u16], log_m: u16, tables: &LeopardGf16Tables) {
    debug_assert_eq!(x.len(), y.len());
    if log_m == MODULUS16 as u16 {
        slice_xor_u16(y, x);
    } else {
        mulgf16_xor(x, y, log_m, tables);
        slice_xor_u16(y, x);
    }
}

/// Inverse radix-2 IFFT butterfly: `y ^= x; x ^= mul(y, m)`.
/// Matches Go's `ifftDIT2` and `sliceXor` shortcut for modulus.
///
/// When `log_m == MODULUS16`, Go uses `sliceXor(work[a], work[b])` which modifies
/// the second argument only: `y ^= x`.
pub(super) fn ifft_dit2_16(x: &mut [u16], y: &mut [u16], log_m: u16, tables: &LeopardGf16Tables) {
    debug_assert_eq!(x.len(), y.len());
    if log_m == MODULUS16 as u16 {
        slice_xor_u16(y, x);
    } else {
        slice_xor_u16(y, x);
        mulgf16_xor(x, y, log_m, tables);
    }
}

/// Forward radix-4 butterfly.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(super) fn fft_dit4_16(
    a: &mut [u16],
    b: &mut [u16],
    c: &mut [u16],
    d: &mut [u16],
    log_m01: u16,
    log_m23: u16,
    log_m02: u16,
    tables: &LeopardGf16Tables,
) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), d.len());

    dit2_step16(a, c, log_m02, tables);
    dit2_step16(b, d, log_m02, tables);
    dit2_step16(a, b, log_m01, tables);
    dit2_step16(c, d, log_m23, tables);
}

/// Inverse radix-4 butterfly.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(super) fn ifft_dit4_16(
    a: &mut [u16],
    b: &mut [u16],
    c: &mut [u16],
    d: &mut [u16],
    log_m01: u16,
    log_m23: u16,
    log_m02: u16,
    tables: &LeopardGf16Tables,
) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), d.len());

    dit2_step_inv16(a, b, log_m01, tables);
    dit2_step_inv16(c, d, log_m23, tables);
    dit2_step_inv16(b, d, log_m02, tables);
    dit2_step_inv16(a, c, log_m02, tables);
}

/// Convert user byte layout to Go's GF16 split layout for u16 processing.
///
/// Go's mul16LUTs interprets each 64-byte chunk as:
///   element i = byte[i] | (byte[i+32] << 8)   for i in 0..32
///
/// Standard u16 LE interprets as:
///   element i = byte[2*i] | (byte[2*i+1] << 8)
///
/// This function rearranges bytes so that `as u16 LE` gives Go's elements:
///   dst[2*i] = src[i], dst[2*i+1] = src[i+32]   per 64-byte chunk
#[allow(clippy::needless_return)]
pub(super) fn user_bytes_to_work_bytes(src: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), dst.len());
    debug_assert!(src.len().is_multiple_of(64));

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        // The kernel is 128-bit SSE2/SSSE3, so it only needs SSSE3 — gating it on
        // AVX2 would needlessly drop SSSE3-only x86_64 CPUs to the scalar path.
        if is_x86_feature_detected!("ssse3") {
            // SAFETY: SSSE3 confirmed at runtime, matching the callee's `#[target_feature(enable="ssse3")]`; src/dst are equal-length, 64-byte-multiple slices.
            unsafe {
                user_bytes_to_work_bytes_ssse3(src, dst);
                return;
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is guaranteed on aarch64, so calling the #[target_feature(enable="neon")] fn is sound; src/dst are equal-length, 64-byte-multiple slices.
        unsafe {
            user_bytes_to_work_bytes_neon(src, dst);
            return;
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    user_bytes_to_work_bytes_scalar(src, dst);
}

fn user_bytes_to_work_bytes_scalar(src: &[u8], dst: &mut [u8]) {
    for (chunk_idx, chunk) in src.chunks(64).enumerate() {
        let base = chunk_idx * 64;
        let dst_chunk = &mut dst[base..base + 64];
        for i in 0..32 {
            dst_chunk[2 * i] = chunk[i];
            dst_chunk[2 * i + 1] = chunk[i + 32];
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn user_bytes_to_work_bytes_ssse3(src: &[u8], dst: &mut [u8]) {
    use core::arch::x86_64::{
        _mm_loadu_si128, _mm_storeu_si128, _mm_unpackhi_epi8, _mm_unpacklo_epi8,
    };

    for (s, d) in src.chunks(64).zip(dst.chunks_mut(64)) {
        // SAFETY: each chunk is 64 bytes, so the 128-bit loads at 0/16/32/48 and stores at 0/16/32/48 are all in-bounds; SSSE3 confirmed by caller.
        unsafe {
            let lo = _mm_loadu_si128(s.as_ptr().cast());
            let hi = _mm_loadu_si128(s[32..].as_ptr().cast());
            _mm_storeu_si128(d.as_mut_ptr().cast(), _mm_unpacklo_epi8(lo, hi));
            _mm_storeu_si128(d[16..].as_mut_ptr().cast(), _mm_unpackhi_epi8(lo, hi));
            let lo2 = _mm_loadu_si128(s[16..].as_ptr().cast());
            let hi2 = _mm_loadu_si128(s[48..].as_ptr().cast());
            _mm_storeu_si128(d[32..].as_mut_ptr().cast(), _mm_unpacklo_epi8(lo2, hi2));
            _mm_storeu_si128(d[48..].as_mut_ptr().cast(), _mm_unpackhi_epi8(lo2, hi2));
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn user_bytes_to_work_bytes_neon(src: &[u8], dst: &mut [u8]) {
    use core::arch::aarch64::{vld1q_u8, vst1q_u8, vzip1q_u8, vzip2q_u8};

    for (s, d) in src.chunks(64).zip(dst.chunks_mut(64)) {
        // Interleave: dst[2*i] = src[i], dst[2*i+1] = src[i+32] for i in 0..32.
        // Split the two 32-byte halves into 16-byte pieces and use vzip.
        // SAFETY: each chunk is 64 bytes, so the 128-bit loads/stores at offsets 0/16/32/48 are all in-bounds; NEON is baseline on aarch64.
        unsafe {
            // First 16 pairs: interleave s[0..16] with s[32..48]
            let lo = vld1q_u8(s.as_ptr());
            let hi = vld1q_u8(s[32..].as_ptr());
            vst1q_u8(d.as_mut_ptr(), vzip1q_u8(lo, hi));
            vst1q_u8(d[16..].as_mut_ptr(), vzip2q_u8(lo, hi));
            // Next 16 pairs: interleave s[16..32] with s[48..64]
            let lo2 = vld1q_u8(s[16..].as_ptr());
            let hi2 = vld1q_u8(s[48..].as_ptr());
            vst1q_u8(d[32..].as_mut_ptr(), vzip1q_u8(lo2, hi2));
            vst1q_u8(d[48..].as_mut_ptr(), vzip2q_u8(lo2, hi2));
        }
    }
}

/// Convert Go's GF16 split layout back to user byte layout.
///
/// Reverse of user_bytes_to_work_bytes:
///   dst[i] = src[2*i], dst[i+32] = src[2*i+1]   per 64-byte chunk
#[allow(clippy::needless_return)]
pub(super) fn work_bytes_to_user_bytes(src: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), dst.len());
    debug_assert!(src.len().is_multiple_of(64));

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        // The kernel is 128-bit SSSE3 (`_mm_shuffle_epi8`), so it only needs
        // SSSE3 — gating it on AVX2 would needlessly drop SSSE3-only x86_64 CPUs
        // to the scalar path.
        if is_x86_feature_detected!("ssse3") {
            // SAFETY: SSSE3 confirmed at runtime, matching the callee's `#[target_feature(enable="ssse3")]`; src/dst are equal-length, 64-byte-multiple slices.
            unsafe {
                work_bytes_to_user_bytes_ssse3(src, dst);
                return;
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is guaranteed on aarch64, so calling the #[target_feature(enable="neon")] fn is sound; src/dst are equal-length, 64-byte-multiple slices.
        unsafe {
            work_bytes_to_user_bytes_neon(src, dst);
            return;
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    work_bytes_to_user_bytes_scalar(src, dst);
}

fn work_bytes_to_user_bytes_scalar(src: &[u8], dst: &mut [u8]) {
    for (chunk_idx, chunk) in src.chunks(64).enumerate() {
        let base = chunk_idx * 64;
        let dst_chunk = &mut dst[base..base + 64];
        for i in 0..32 {
            dst_chunk[i] = chunk[2 * i];
            dst_chunk[i + 32] = chunk[2 * i + 1];
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn work_bytes_to_user_bytes_ssse3(src: &[u8], dst: &mut [u8]) {
    use core::arch::x86_64::{_mm_loadu_si128, _mm_shuffle_epi8, _mm_storel_epi64};

    // Mask to extract even-indexed bytes: [0,2,4,6,8,10,12,14], rest zeroed (0x80).
    #[rustfmt::skip]
    // SAFETY: the 16-byte array literal backs a valid 128-bit unaligned load of the shuffle mask.
    let even_mask = unsafe { _mm_loadu_si128([
        0u8, 2, 4, 6, 8, 10, 12, 14, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    ].as_ptr().cast()) };
    // Mask to extract odd-indexed bytes: [1,3,5,7,9,11,13,15], rest zeroed (0x80).
    #[rustfmt::skip]
    // SAFETY: the 16-byte array literal backs a valid 128-bit unaligned load of the shuffle mask.
    let odd_mask = unsafe { _mm_loadu_si128([
        1u8, 3, 5, 7, 9, 11, 13, 15, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    ].as_ptr().cast()) };

    for (s, d) in src.chunks(64).zip(dst.chunks_mut(64)) {
        // SAFETY: each chunk is 64 bytes, so the four 128-bit loads at 0/16/32/48
        // are in-bounds and SSSE3 is confirmed by the caller. Only the low 8 bytes
        // of each shuffle are meaningful (the mask zeroes the rest), so the 8-byte
        // `storel_epi64` stores at 0/8/…/56 tile the 64-byte chunk exactly, with
        // no write past its end.
        unsafe {
            let p0 = _mm_loadu_si128(s.as_ptr().cast());
            let p1 = _mm_loadu_si128(s[16..].as_ptr().cast());
            let p2 = _mm_loadu_si128(s[32..].as_ptr().cast());
            let p3 = _mm_loadu_si128(s[48..].as_ptr().cast());
            // Even bytes (a_i) → dst[0..8], dst[8..16], dst[16..24], dst[24..32]
            _mm_storel_epi64(d.as_mut_ptr().cast(), _mm_shuffle_epi8(p0, even_mask));
            _mm_storel_epi64(d[8..].as_mut_ptr().cast(), _mm_shuffle_epi8(p1, even_mask));
            _mm_storel_epi64(d[16..].as_mut_ptr().cast(), _mm_shuffle_epi8(p2, even_mask));
            _mm_storel_epi64(d[24..].as_mut_ptr().cast(), _mm_shuffle_epi8(p3, even_mask));
            // Odd bytes (b_i) → dst[32..40], dst[40..48], dst[48..56], dst[56..64]
            _mm_storel_epi64(d[32..].as_mut_ptr().cast(), _mm_shuffle_epi8(p0, odd_mask));
            _mm_storel_epi64(d[40..].as_mut_ptr().cast(), _mm_shuffle_epi8(p1, odd_mask));
            _mm_storel_epi64(d[48..].as_mut_ptr().cast(), _mm_shuffle_epi8(p2, odd_mask));
            _mm_storel_epi64(d[56..].as_mut_ptr().cast(), _mm_shuffle_epi8(p3, odd_mask));
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn work_bytes_to_user_bytes_neon(src: &[u8], dst: &mut [u8]) {
    use core::arch::aarch64::{vld1q_u8, vst1q_u8, vuzp1q_u8, vuzp2q_u8};

    for (s, d) in src.chunks(64).zip(dst.chunks_mut(64)) {
        // De-interleave: dst[i] = src[2*i], dst[i+32] = src[2*i+1] per 64-byte chunk.
        // vuzp1q extracts even bytes (a_i), vuzp2q extracts odd bytes (b_i).
        // p0 = s[0..16], p1 = s[16..32] → even/odd from first 32 interleaved bytes.
        // p2 = s[32..48], p3 = s[48..64] → even/odd from second 32 interleaved bytes.
        // SAFETY: each chunk is 64 bytes, so the 128-bit loads at 0/16/32/48 and stores at 0/16/32/48 are in-bounds; NEON is baseline on aarch64.
        unsafe {
            let p0 = vld1q_u8(s.as_ptr());
            let p1 = vld1q_u8(s[16..].as_ptr());
            // Even bytes from first 32 → d[0..16], odd bytes → d[32..48]
            vst1q_u8(d.as_mut_ptr(), vuzp1q_u8(p0, p1));
            vst1q_u8(d[32..].as_mut_ptr(), vuzp2q_u8(p0, p1));
            let p2 = vld1q_u8(s[32..].as_ptr());
            let p3 = vld1q_u8(s[48..].as_ptr());
            // Even bytes from second 32 → d[16..32], odd bytes → d[48..64]
            vst1q_u8(d[16..].as_mut_ptr(), vuzp1q_u8(p2, p3));
            vst1q_u8(d[48..].as_mut_ptr(), vuzp2q_u8(p2, p3));
        }
    }
}

/// Convert user-layout bytes into aligned split-layout `u16` elements.
///
/// Fuses the byte-level split-layout shuffle with the byte-to-`u16` decode, so
/// the codec never reinterprets an align-1 `&[u8]` as `&[u16]`. On little-endian
/// the shuffle writes straight into the returned buffer's storage (a single
/// allocation, no intermediate copy); on big-endian the split bytes are decoded
/// element-wise as little-endian to preserve the on-the-wire layout.
pub(super) fn user_bytes_to_work_u16(user: &[u8]) -> Vec<u16> {
    debug_assert!(user.len().is_multiple_of(2));
    let mut out = vec![0u16; user.len() / 2];
    user_bytes_to_work_u16_into(user, &mut out);
    out
}

/// Convert whole 64-byte user-layout blocks directly into existing work lanes.
/// Both slices must describe the same number of elements. Chunk boundaries must
/// preserve the 64-byte split layout used on the wire.
pub(super) fn user_bytes_to_work_u16_into(user: &[u8], out: &mut [u16]) {
    assert_eq!(user.len(), out.len() * 2);
    assert!(user.len().is_multiple_of(64));

    #[cfg(target_endian = "little")]
    {
        // SAFETY: `out` is `u16`-aligned (Vec guarantees alignment >= 2), so a
        // `u8` view is always valid; its length is exactly `out`'s byte size.
        // `u16` has no padding or invalid bit patterns, so writing arbitrary
        // bytes is sound, and read back as little-endian the split-layout bytes
        // are the field elements.
        let bytes =
            unsafe { core::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), user.len()) };
        user_bytes_to_work_bytes(user, bytes);
    }
    #[cfg(target_endian = "big")]
    {
        for (src, dst) in user.chunks_exact(64).zip(out.chunks_exact_mut(32)) {
            for (i, value) in dst.iter_mut().enumerate() {
                *value = u16::from_le_bytes([src[i], src[i + 32]]);
            }
        }
    }
}

/// Encode split-layout `u16` elements straight into user byte layout.
///
/// Fuses the element-bytes-to-split-layout encode with the split-layout-to-user
/// de-interleave ([`work_bytes_to_user_bytes`]) into a single pass, so encode and
/// decode write their `u16` results in final user layout without an intermediate
/// split-byte buffer or a second whole-shard de-interleave. `src` must cover a
/// whole number of 32-element (64-byte) blocks, which the encoder/decoder chunking
/// guarantees. On little-endian the `u16` bytes already *are* the split-layout
/// bytes, so this reuses the SIMD de-interleave directly.
pub(super) fn work_u16_to_user_bytes(src: &[u16], dst: &mut [u8]) {
    debug_assert_eq!(dst.len(), src.len() * 2);
    debug_assert!(dst.len().is_multiple_of(64));

    #[cfg(target_endian = "little")]
    {
        // SAFETY: `src` is `u16`-aligned, so a `u8` view is always valid; its
        // length is exactly `dst.len()`. Little-endian `u16` bytes match the
        // split-layout byte order, so de-interleaving them yields user layout.
        let src_bytes =
            unsafe { core::slice::from_raw_parts(src.as_ptr().cast::<u8>(), dst.len()) };
        work_bytes_to_user_bytes(src_bytes, dst);
    }
    #[cfg(target_endian = "big")]
    {
        let mut split = alloc::vec![0u8; dst.len()];
        for (c, &v) in split.chunks_exact_mut(2).zip(src.iter()) {
            c.copy_from_slice(&v.to_le_bytes());
        }
        work_bytes_to_user_bytes(&split, dst);
    }
}

/// Helper to get two mutable references from a slice at indices i and j.
pub(super) fn get_pair_mut_16<T>(slice: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
    if i == j || i >= slice.len() || j >= slice.len() {
        return None;
    }
    let (lo, hi, swapped) = if i < j { (i, j, false) } else { (j, i, true) };
    let (left, right) = slice.split_at_mut(hi);
    let first = &mut left[lo];
    let second = &mut right[0];
    if swapped {
        Some((second, first))
    } else {
        Some((first, second))
    }
}
