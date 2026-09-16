use super::{BITWIDTH8, LeopardGf8Tables, MODULUS8, ORDER8};

pub(super) fn mul_log8(a: u8, log_b: u8, log_lut: &[u8; ORDER8], exp_lut: &[u8; ORDER8]) -> u8 {
    if a == 0 {
        return 0;
    }

    exp_lut[add_mod8(log_lut[a as usize], log_b) as usize]
}

pub(super) fn add_mod8(a: u8, b: u8) -> u8 {
    let sum = a as usize + b as usize;
    (sum + (sum >> BITWIDTH8)) as u8
}

pub(super) fn sub_mod8(a: u8, b: u8) -> u8 {
    // Match Go's `uint(a) - uint(b)` which uses unsigned wrapping on usize.
    // The subtraction on usize creates a borrow that propagates into the high
    // bits; `dif + dif>>8` folds the carry back, then truncation to u8 gives
    // the correct mod-255 result.
    let dif = (a as usize).wrapping_sub(b as usize);
    (dif.wrapping_add(dif >> BITWIDTH8)) as u8
}

#[allow(clippy::explicit_counter_loop)]
pub(super) fn fwht8(data: &mut [u8; ORDER8]) {
    let mut dist = 1usize;
    let mut dist4 = 4usize;
    while dist4 <= ORDER8 {
        let mut r = 0usize;
        while r < ORDER8 {
            let mut off = r;
            for _ in 0..dist {
                let (t0, t1) = fwht2_alt8(data[off], data[off + dist]);
                data[off] = t0;
                data[off + dist] = t1;
                let (t2, t3) = fwht2_alt8(data[off + dist * 2], data[off + dist * 3]);
                data[off + dist * 2] = t2;
                data[off + dist * 3] = t3;
                let (t0, t2) = fwht2_alt8(data[off], data[off + dist * 2]);
                data[off] = t0;
                data[off + dist * 2] = t2;
                let (t1, t3) = fwht2_alt8(data[off + dist], data[off + dist * 3]);
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

/// FWHT with mtrunc: outer loop runs to ORDER8, inner loop limited to mtrunc.
///
/// Matches Go's `fwht(data, mtrunc)` exactly — sequential radix-2 butterflies
/// within each block, positions beyond mtrunc untouched.
#[allow(clippy::explicit_counter_loop)]
pub(super) fn fwht8_mtrunc(data: &mut [u8], mtrunc: usize) {
    debug_assert_eq!(data.len(), ORDER8);
    let mut dist = 1usize;
    let mut dist4 = 4usize;
    while dist4 <= ORDER8 {
        let mut r = 0usize;
        while r < mtrunc {
            let mut off = r;
            for _ in 0..dist {
                let (t0, t1) = fwht2_alt8(data[off], data[off + dist]);
                data[off] = t0;
                data[off + dist] = t1;
                let (t2, t3) = fwht2_alt8(data[off + dist * 2], data[off + dist * 3]);
                data[off + dist * 2] = t2;
                data[off + dist * 3] = t3;
                let (t0, t2) = fwht2_alt8(data[off], data[off + dist * 2]);
                data[off] = t0;
                data[off + dist * 2] = t2;
                let (t1, t3) = fwht2_alt8(data[off + dist], data[off + dist * 3]);
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

/// Flexible-size FWHT for slices whose length is a power of 2 and <= ORDER8.
///
/// Used by the decode path where the transform size is `m + data_shards`
/// (not necessarily ORDER8). Matches Go's `fwht(data, len)`.
#[allow(clippy::explicit_counter_loop)]
pub(super) fn fwht_variable(data: &mut [u8]) {
    let n = data.len();
    debug_assert!(n.is_power_of_two());
    debug_assert!(n <= ORDER8);

    let mut dist = 1usize;
    while dist < n {
        let dist4 = dist * 4;
        if dist4 <= n {
            let mut r = 0usize;
            while r < n {
                let mut off = r;
                for _ in 0..dist {
                    let (t0, t1) = fwht2_alt8(data[off], data[off + dist]);
                    data[off] = t0;
                    data[off + dist] = t1;
                    let (t2, t3) = fwht2_alt8(data[off + dist * 2], data[off + dist * 3]);
                    data[off + dist * 2] = t2;
                    data[off + dist * 3] = t3;
                    let (t0, t2) = fwht2_alt8(data[off], data[off + dist * 2]);
                    data[off] = t0;
                    data[off + dist * 2] = t2;
                    let (t1, t3) = fwht2_alt8(data[off + dist], data[off + dist * 3]);
                    data[off + dist] = t1;
                    data[off + dist * 3] = t3;
                    off += 1;
                }
                r += dist4;
            }
            dist = dist4;
        } else {
            // Remaining pairwise pass (dist * 2 <= n < dist * 4)
            let dist2 = dist * 2;
            if dist2 <= n {
                let mut r = 0usize;
                while r < n {
                    let mut off = r;
                    for _ in 0..dist {
                        let (t0, t1) = fwht2_alt8(data[off], data[off + dist]);
                        data[off] = t0;
                        data[off + dist] = t1;
                        off += 1;
                    }
                    r += dist2;
                }
            }
            break;
        }
    }
}

fn fwht2_alt8(a: u8, b: u8) -> (u8, u8) {
    (add_mod8(a, b), sub_mod8(a, b))
}

pub(super) fn slice_xor(input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: runtime feature detection confirmed AVX2 is available.
            unsafe {
                slice_xor_avx2(input, out);
            }
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is mandatory on aarch64.
        unsafe {
            slice_xor_neon(input, out);
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    slice_xor_u64(input, out);
}

/// Portable u64-block XOR fallback used when no wider SIMD path was selected.
fn slice_xor_u64(input: &[u8], out: &mut [u8]) {
    // Process 64 bytes per iteration using u64 blocks.
    // Uses unaligned reads to avoid UB on sub-slice pointers.
    let (input64, input_tail64) = input.as_chunks::<64>();
    let (out64, out_tail64) = out.as_chunks_mut::<64>();

    for (src, dst) in input64.iter().zip(out64.iter_mut()) {
        for i in 0..8 {
            let off = i * 8;
            // SAFETY: 8 bytes guaranteed by as_chunks::<64>().
            let s = unsafe { core::ptr::read_unaligned(src[off..].as_ptr().cast::<u64>()) };
            // SAFETY: 8 bytes guaranteed by as_chunks_mut::<64>().
            let d = unsafe { core::ptr::read_unaligned(dst[off..].as_ptr().cast::<u64>()) };
            // SAFETY: off+8 <= 64 within the as_chunks_mut::<64>() chunk; write in bounds.
            unsafe {
                core::ptr::write_unaligned(dst[off..].as_mut_ptr().cast::<u64>(), d ^ s);
            }
        }
    }

    // Process remaining bytes in 8-byte chunks.
    let (input8, input_tail) = input_tail64.as_chunks::<8>();
    let (out8, out_tail) = out_tail64.as_chunks_mut::<8>();

    for (src, dst) in input8.iter().zip(out8.iter_mut()) {
        let s = u64::from_ne_bytes(*src);
        let d = u64::from_ne_bytes(*dst);
        *dst = (d ^ s).to_ne_bytes();
    }

    // Scalar tail.
    for (src, dst) in input_tail.iter().zip(out_tail.iter_mut()) {
        *dst ^= *src;
    }
}

/// AVX2 SIMD XOR: 32 bytes per iteration using `_mm256_xor_si256`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn slice_xor_avx2(input: &[u8], out: &mut [u8]) {
    use core::arch::x86_64::{_mm256_loadu_si256, _mm256_storeu_si256, _mm256_xor_si256};

    let (in32, in_tail) = input.as_chunks::<32>();
    let (out32, out_tail) = out.as_chunks_mut::<32>();

    for (src, dst) in in32.iter().zip(out32.iter_mut()) {
        // SAFETY: as_chunks::<32>() guarantees 32 valid bytes for load/store.
        let s = unsafe { _mm256_loadu_si256(src.as_ptr().cast()) };
        // SAFETY: as_chunks_mut::<32>() guarantees 32 valid bytes for load.
        let d = unsafe { _mm256_loadu_si256(dst.as_ptr().cast()) };
        // SAFETY: as_chunks_mut::<32>() guarantees 32 valid bytes for store.
        unsafe { _mm256_storeu_si256(dst.as_mut_ptr().cast(), _mm256_xor_si256(d, s)) };
    }

    // Scalar tail (0-31 bytes).
    for (src, dst) in in_tail.iter().zip(out_tail.iter_mut()) {
        *dst ^= *src;
    }
}

/// NEON SIMD XOR: 64 bytes per iteration using `vld1q_u8_x4` / `veorq_u8`.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn slice_xor_neon(input: &[u8], out: &mut [u8]) {
    use core::arch::aarch64::{
        uint8x16x4_t, veorq_u8, vld1q_u8, vld1q_u8_x4, vst1q_u8, vst1q_u8_x4,
    };

    let (in64, in_tail) = input.as_chunks::<64>();
    let (out64, out_tail) = out.as_chunks_mut::<64>();

    for (src, dst) in in64.iter().zip(out64.iter_mut()) {
        // SAFETY: as_chunks::<64>() guarantees 64 valid bytes for load/store.
        let s = unsafe { vld1q_u8_x4(src.as_ptr()) };
        // SAFETY: as_chunks_mut::<64>() guarantees 64 valid bytes for load.
        let d = unsafe { vld1q_u8_x4(dst.as_ptr()) };
        // SAFETY: as_chunks_mut::<64>() guarantees 64 valid bytes for store.
        unsafe {
            vst1q_u8_x4(
                dst.as_mut_ptr(),
                uint8x16x4_t(
                    veorq_u8(d.0, s.0),
                    veorq_u8(d.1, s.1),
                    veorq_u8(d.2, s.2),
                    veorq_u8(d.3, s.3),
                ),
            )
        };
    }

    // 16-byte tail.
    let (in16, in_scalar) = in_tail.as_chunks::<16>();
    let (out16, out_scalar) = out_tail.as_chunks_mut::<16>();
    for (src, dst) in in16.iter().zip(out16.iter_mut()) {
        // SAFETY: as_chunks::<16>() guarantees 16 valid bytes.
        let s = unsafe { vld1q_u8(src.as_ptr()) };
        // SAFETY: as_chunks_mut::<16>() guarantees 16 valid bytes for load.
        let d = unsafe { vld1q_u8(dst.as_ptr()) };
        // SAFETY: as_chunks_mut::<16>() guarantees 16 valid bytes for store.
        unsafe { vst1q_u8(dst.as_mut_ptr(), veorq_u8(d, s)) };
    }

    // Scalar tail (0-15 bytes).
    for (src, dst) in in_scalar.iter().zip(out_scalar.iter_mut()) {
        *dst ^= *src;
    }
}

/// SIMD-accelerated LUT-XOR with pre-split nibble tables.
///
/// Same as `lut_xor` but accepts pre-computed nibble halves to avoid
/// rebuilding them on every call. `low[i] = lut[i]` for `i in 0..16`,
/// `high[i] = lut[i * 16]` for `i in 0..16`.
#[inline]
fn lut_xor_prebuilt(dst: &mut [u8], src: &[u8], low: &[u8; 16], high: &[u8; 16], lut: &[u8; 256]) {
    lut_xor_impl(dst, src, low, high, lut)
}

#[inline]
fn lut_xor_impl(dst: &mut [u8], src: &[u8], low: &[u8; 16], high: &[u8; 16], lut: &[u8; 256]) {
    debug_assert_eq!(dst.len(), src.len());

    #[cfg(not(any(all(feature = "std", target_arch = "x86_64"), target_arch = "aarch64")))]
    let _ = (low, high);

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        if dst.len() >= 32 && is_x86_feature_detected!("avx2") {
            // SAFETY: runtime feature detection confirmed AVX2 is available, len >= 32.
            unsafe {
                lut_xor_avx2_prebuilt(dst, src, low, high, lut);
            }
            return;
        }
        if dst.len() >= 16 && is_x86_feature_detected!("ssse3") {
            // SAFETY: runtime feature detection confirmed SSSE3 is available, len >= 16.
            unsafe {
                lut_xor_ssse3_prebuilt(dst, src, low, high, lut);
            }
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if dst.len() >= 16 {
            // SAFETY: aarch64 always has NEON, len >= 16.
            unsafe {
                lut_xor_neon_prebuilt(dst, src, low, high, lut);
            }
            return;
        }
    }

    // Scalar fallback.
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d ^= lut[*s as usize];
    }
}

/// AVX2 nibble-lookup with pre-split tables: `dst[i] ^= lut[src[i]]`, 32 bytes/iter.
///
/// Accepts pre-computed 16-byte nibble halves to avoid per-call table construction.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn lut_xor_avx2_prebuilt(
    dst: &mut [u8],
    src: &[u8],
    low: &[u8; 16],
    high: &[u8; 16],
    lut: &[u8; 256],
) {
    use core::arch::x86_64::{
        __m256i, _mm256_and_si256, _mm256_loadu_si256, _mm256_set1_epi8, _mm256_shuffle_epi8,
        _mm256_srli_epi64, _mm256_storeu_si256, _mm256_xor_si256,
    };

    // Broadcast 16-byte tables to both 128-bit lanes of 256-bit registers.
    let low_tbl: __m256i = {
        use core::arch::x86_64::{__m128i, _mm_loadu_si128, _mm256_broadcastsi128_si256};
        // SAFETY: `low` is a valid 16-byte array, enough for a 128-bit load.
        let lo128: __m128i = unsafe { _mm_loadu_si128(low.as_ptr().cast()) };
        _mm256_broadcastsi128_si256(lo128)
    };
    let high_tbl: __m256i = {
        use core::arch::x86_64::{__m128i, _mm_loadu_si128, _mm256_broadcastsi128_si256};
        // SAFETY: `high` is a valid 16-byte array, enough for a 128-bit load.
        let hi128: __m128i = unsafe { _mm_loadu_si128(high.as_ptr().cast()) };
        _mm256_broadcastsi128_si256(hi128)
    };
    let nibble_mask: __m256i = _mm256_set1_epi8(0x0f);

    let (src32, src_tail) = src.as_chunks::<32>();
    let (dst32, dst_tail) = dst.as_chunks_mut::<32>();

    for (s_chunk, d_chunk) in src32.iter().zip(dst32.iter_mut()) {
        // SAFETY: as_chunks::<32>() guarantees 32 valid bytes for load/store.
        let sv = unsafe { _mm256_loadu_si256(s_chunk.as_ptr().cast()) };
        // SAFETY: as_chunks_mut::<32>() guarantees 32 valid bytes for load.
        let dv = unsafe { _mm256_loadu_si256(d_chunk.as_ptr().cast()) };
        let lo = _mm256_and_si256(sv, nibble_mask);
        let hi = _mm256_and_si256(_mm256_srli_epi64::<4>(sv), nibble_mask);
        let product = _mm256_xor_si256(
            _mm256_shuffle_epi8(low_tbl, lo),
            _mm256_shuffle_epi8(high_tbl, hi),
        );
        // SAFETY: as_chunks_mut::<32>() guarantees 32 valid bytes for store.
        unsafe {
            _mm256_storeu_si256(d_chunk.as_mut_ptr().cast(), _mm256_xor_si256(dv, product));
        }
    }

    // Scalar tail (0-31 bytes).
    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d ^= lut[*s as usize];
    }
}

/// SSSE3 nibble-lookup with pre-split tables: `dst[i] ^= lut[src[i]]`, 16 bytes/iter.
///
/// Accepts pre-computed 16-byte nibble halves to avoid per-call table construction.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn lut_xor_ssse3_prebuilt(
    dst: &mut [u8],
    src: &[u8],
    low: &[u8; 16],
    high: &[u8; 16],
    lut: &[u8; 256],
) {
    use core::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_loadu_si128, _mm_set1_epi8, _mm_shuffle_epi8, _mm_srli_epi64,
        _mm_storeu_si128, _mm_xor_si128,
    };

    // SAFETY: `low` is a valid 16-byte array, enough for a 128-bit load.
    let low_tbl: __m128i = unsafe { _mm_loadu_si128(low.as_ptr().cast()) };
    // SAFETY: `high` is a valid 16-byte array, enough for a 128-bit load.
    let high_tbl: __m128i = unsafe { _mm_loadu_si128(high.as_ptr().cast()) };
    let nibble_mask: __m128i = _mm_set1_epi8(0x0f);

    let (src16, src_tail) = src.as_chunks::<16>();
    let (dst16, dst_tail) = dst.as_chunks_mut::<16>();

    for (s_chunk, d_chunk) in src16.iter().zip(dst16.iter_mut()) {
        // SAFETY: as_chunks::<16>() guarantees 16 valid bytes for load.
        let sv = unsafe { _mm_loadu_si128(s_chunk.as_ptr().cast()) };
        // SAFETY: as_chunks_mut::<16>() guarantees 16 valid bytes for load.
        let dv = unsafe { _mm_loadu_si128(d_chunk.as_ptr().cast()) };
        let lo = _mm_and_si128(sv, nibble_mask);
        let hi = _mm_and_si128(_mm_srli_epi64::<4>(sv), nibble_mask);
        let product = _mm_xor_si128(
            _mm_shuffle_epi8(low_tbl, lo),
            _mm_shuffle_epi8(high_tbl, hi),
        );
        // SAFETY: as_chunks_mut::<16>() guarantees 16 valid bytes for store.
        unsafe {
            _mm_storeu_si128(d_chunk.as_mut_ptr().cast(), _mm_xor_si128(dv, product));
        }
    }

    // Scalar tail (0-15 bytes).
    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d ^= lut[*s as usize];
    }
}

pub(super) fn mulgf8(out: &mut [u8], input: &[u8], log_m: u8, tables: &LeopardGf8Tables) {
    let lut = &tables.mul_luts[log_m as usize];
    debug_assert_eq!(input.len(), out.len());
    for (src, dst) in input.iter().zip(out.iter_mut()) {
        *dst = lut.value[*src as usize];
    }
}

pub(super) fn fft_dit2(x: &mut [u8], y: &mut [u8], log_m: u8, tables: &LeopardGf8Tables) {
    let lut = &tables.mul_luts[log_m as usize];
    dit2_step_prebuilt(x, y, log_m, &lut.value, &lut.low, &lut.high);
}

/// Forward butterfly with pre-split nibble tables for SIMD acceleration.
#[inline(always)]
fn dit2_step_prebuilt(
    dst: &mut [u8],
    src: &mut [u8],
    log_m: u8,
    lut: &[u8; 256],
    low: &[u8; 16],
    high: &[u8; 16],
) {
    if log_m == MODULUS8 as u8 {
        slice_xor(dst, src);
    } else {
        lut_xor_prebuilt(dst, src, low, high, lut);
        slice_xor(dst, src);
    }
}

/// Inverse butterfly with pre-split nibble tables for SIMD acceleration.
#[inline(always)]
fn dit2_step_inv_prebuilt(
    dst: &mut [u8],
    src: &mut [u8],
    log_m: u8,
    lut: &[u8; 256],
    low: &[u8; 16],
    high: &[u8; 16],
) {
    if log_m == MODULUS8 as u8 {
        slice_xor(dst, src);
    } else {
        slice_xor(dst, src);
        lut_xor_prebuilt(dst, src, low, high, lut);
    }
}

pub(super) fn ifft_dit2(x: &mut [u8], y: &mut [u8], log_m: u8, tables: &LeopardGf8Tables) {
    let lut = &tables.mul_luts[log_m as usize];
    dit2_step_inv_prebuilt(x, y, log_m, &lut.value, &lut.low, &lut.high);
}

/// Zero-copy forward radix-4 butterfly using pre-allocated scratch buffer
/// and pre-split nibble tables.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn fft_dit4_full_lut_scratch(
    a: &mut [u8],
    b: &mut [u8],
    c: &mut [u8],
    d: &mut [u8],
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    lut01: &[u8; 256],
    lut01_low: &[u8; 16],
    lut01_high: &[u8; 16],
    lut23: &[u8; 256],
    lut23_low: &[u8; 16],
    lut23_high: &[u8; 16],
    lut02: &[u8; 256],
    lut02_low: &[u8; 16],
    lut02_high: &[u8; 16],
    scratch: &mut [u8],
) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), d.len());
    debug_assert!(scratch.len() >= a.len());
    let _ = scratch; // not needed for forward butterfly

    // Go ARM64 fftDIT28(x, y): x ^= mul(y); y ^= x
    // First layer: pairs (a,c) and (b,d) with m02
    dit2_step_prebuilt(a, c, log_m02, lut02, lut02_low, lut02_high);
    dit2_step_prebuilt(b, d, log_m02, lut02, lut02_low, lut02_high);
    // Second layer: pair (a,b) with m01, pair (c,d) with m23
    dit2_step_prebuilt(a, b, log_m01, lut01, lut01_low, lut01_high);
    dit2_step_prebuilt(c, d, log_m23, lut23, lut23_low, lut23_high);
}

/// Zero-copy inverse radix-4 butterfly using pre-allocated scratch buffer
/// and pre-split nibble tables.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn ifft_dit4_full_lut_scratch(
    a: &mut [u8],
    b: &mut [u8],
    c: &mut [u8],
    d: &mut [u8],
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    lut01: &[u8; 256],
    lut01_low: &[u8; 16],
    lut01_high: &[u8; 16],
    lut23: &[u8; 256],
    lut23_low: &[u8; 16],
    lut23_high: &[u8; 16],
    lut02: &[u8; 256],
    lut02_low: &[u8; 16],
    lut02_high: &[u8; 16],
    scratch: &mut [u8],
) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), d.len());
    debug_assert!(scratch.len() >= a.len());

    // Go ARM64 ifftDIT28(x, y): y ^= x; x ^= mul(y)
    // Step 1: (a,b) with m01.
    dit2_step_inv_prebuilt(a, b, log_m01, lut01, lut01_low, lut01_high);

    // Step 2: (c,d) with m23, then (b,d) with m02.
    dit2_step_inv_prebuilt(c, d, log_m23, lut23, lut23_low, lut23_high);
    dit2_step_inv_prebuilt(b, d, log_m02, lut02, lut02_low, lut02_high);

    // Step 3: (a,c) with m02.
    dit2_step_inv_prebuilt(a, c, log_m02, lut02, lut02_low, lut02_high);
}

/// NEON nibble-lookup with pre-split tables: `dst[i] ^= lut[src[i]]`, 16 bytes/iter.
///
/// Accepts pre-computed 16-byte nibble halves to avoid per-call table construction.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn lut_xor_neon_prebuilt(
    dst: &mut [u8],
    src: &[u8],
    low: &[u8; 16],
    high: &[u8; 16],
    lut: &[u8; 256],
) {
    use core::arch::aarch64::{
        uint8x16_t, vandq_u8, vdupq_n_u8, veorq_u8, vld1q_u8, vqtbl1q_u8, vshrq_n_u8, vst1q_u8,
    };

    // SAFETY: low and high are valid 16-byte arrays.
    let low_tbl: uint8x16_t = unsafe { vld1q_u8(low.as_ptr()) };
    // SAFETY: `high` is a valid 16-byte array for a 128-bit load.
    let high_tbl: uint8x16_t = unsafe { vld1q_u8(high.as_ptr()) };
    let nibble_mask: uint8x16_t = vdupq_n_u8(0x0f);

    let (src16, src_tail) = src.as_chunks::<16>();
    let (dst16, dst_tail) = dst.as_chunks_mut::<16>();

    for (s_chunk, d_chunk) in src16.iter().zip(dst16.iter_mut()) {
        // SAFETY: as_chunks::<16>() guarantees 16 valid bytes.
        let sv = unsafe { vld1q_u8(s_chunk.as_ptr()) };
        // SAFETY: as_chunks_mut::<16>() guarantees 16 valid bytes for load.
        let dv = unsafe { vld1q_u8(d_chunk.as_ptr()) };
        let lo = vandq_u8(sv, nibble_mask);
        let hi = vandq_u8(vshrq_n_u8::<4>(sv), nibble_mask);
        let product = veorq_u8(vqtbl1q_u8(low_tbl, lo), vqtbl1q_u8(high_tbl, hi));
        // SAFETY: as_chunks_mut::<16>() guarantees 16 valid bytes for store.
        unsafe { vst1q_u8(d_chunk.as_mut_ptr(), veorq_u8(dv, product)) };
    }

    // Scalar tail (0-15 bytes).
    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d ^= lut[*s as usize];
    }
}

pub(super) fn get_pair_mut<T>(slice: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
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

/// Butterfly transform direction, shared between encode and decode paths.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TransformDir {
    Forward,
    Inverse,
}
