//! SIMD-accelerated GF(2^16) fixed-multiplier multiply for the Leopard FFT
//! butterfly hot path (`mulgf16` / `mulgf16_xor`).
//!
//! # Algorithm — 4-nibble shuffle tables
//!
//! Multiplying a 16-bit field element `v` by a fixed `g^log_m` is GF(2)-linear,
//! so it distributes over the 4-nibble decomposition of `v`:
//!
//! ```text
//! v = n0 | (n1<<4) | (n2<<8) | (n3<<12)
//! mul_log16(v) = mul_log16(n0<<0) ^ mul_log16(n1<<4) ^ mul_log16(n2<<8) ^ mul_log16(n3<<12)
//! ```
//!
//! For a fixed `log_m` we precompute, per nibble position `i` (0..4) and nibble
//! value `nib` (0..16), the 16-bit product `mul_log16(nib << 4i, log_m)`, split
//! into its low and high bytes. That is `4 * 16 = 64` bytes for the low plane and
//! 64 for the high plane — **128 bytes of stack tables**, versus Go's 8 KiB per
//! multiplier. A vector of elements is then evaluated with `pshufb`/`tbl` byte
//! shuffles:
//!
//! ```text
//! out_lo = lo[0][n0] ^ lo[1][n1] ^ lo[2][n2] ^ lo[3][n3]
//! out_hi = hi[0][n0] ^ hi[1][n1] ^ hi[2][n2] ^ hi[3][n3]
//! ```
//!
//! Both output planes need **all four** shuffles: `n2`/`n3` come from the input
//! high byte yet also feed the output low byte (and vice-versa), because the
//! carry-less product mixes the byte lanes.
//!
//! # Layout
//!
//! The work buffer is interleaved little-endian `u16` (`[lo0,hi0,lo1,hi1,…]` in
//! memory), not Go's planar layout, so each kernel de-interleaves the byte planes
//! on load and re-interleaves them on store. All kernels are gated on
//! `target_endian = "little"`; big-endian falls back to the scalar path (Leopard
//! GF16 is already rejected on big-endian at construction, rustfs/backlog#1238).
//!
//! Zero elements need no branch (`lo[i][0] = hi[i][0] = 0`). `log_m == 0`
//! (identity) is handled by the general kernel (its tables reproduce the input);
//! only `log_m == MODULUS16` is short-circuited by the callers in `ops.rs`.

use super::ops::mul_log16;
use super::{LeopardGf16Tables, ORDER16};

/// Minimum element count worth the per-call table build + SIMD setup. Shorter
/// slices use the scalar path directly.
pub(super) const SIMD_MIN_LEN: usize = 16;

/// Whether this target has a little-endian SIMD kernel (x86_64 with std for
/// runtime feature detection, or aarch64 with its baseline NEON). Single source
/// of truth for the dispatch gating in [`should_use_simd`].
const HAS_SIMD_KERNEL: bool = cfg!(all(
    any(
        all(feature = "std", target_arch = "x86_64"),
        target_arch = "aarch64"
    ),
    target_endian = "little"
));

/// 4-nibble shuffle tables for a fixed multiplier `g^log_m` (128 bytes).
///
/// `lo[i][nib]` / `hi[i][nib]` are the low / high byte of
/// `mul_log16(nib << 4i, log_m)`.
pub(super) struct NibbleTables16 {
    pub(super) lo: [[u8; 16]; 4],
    pub(super) hi: [[u8; 16]; 4],
}

impl NibbleTables16 {
    pub(super) fn build(
        log_m: u16,
        log_lut: &[u16; ORDER16],
        exp_lut: &[u16; ORDER16 * 2],
    ) -> Self {
        let mut lo = [[0u8; 16]; 4];
        let mut hi = [[0u8; 16]; 4];
        for i in 0..4 {
            let shift = 4 * i as u32;
            #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
            {
                let a = mul_log16(1 << shift, log_m, log_lut, exp_lut);
                let b = mul_log16(2 << shift, log_m, log_lut, exp_lut);
                let c = mul_log16(4 << shift, log_m, log_lut, exp_lut);
                let d = mul_log16(8 << shift, log_m, log_lut, exp_lut);
                fill_nibble_tables(&mut lo[i], &mut hi[i], a, b, c, d);
            }
            #[cfg(not(all(feature = "std", target_arch = "x86_64", target_endian = "little")))]
            for nib in 0u16..16 {
                let v = mul_log16(nib << shift, log_m, log_lut, exp_lut);
                lo[i][nib as usize] = v as u8;
                hi[i][nib as usize] = (v >> 8) as u8;
            }
        }
        NibbleTables16 { lo, hi }
    }
}

// Keep one shared expansion body: inlining it into every nibble position
// substantially increases x86 code size and slows large GFNI recoveries.
#[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
#[inline(never)]
fn fill_nibble_tables(lo: &mut [u8; 16], hi: &mut [u8; 16], a: u16, b: u16, c: u16, d: u16) {
    let ab = a ^ b;
    let cd = c ^ d;
    let values = [
        0,
        a,
        b,
        ab,
        c,
        a ^ c,
        b ^ c,
        ab ^ c,
        d,
        a ^ d,
        b ^ d,
        ab ^ d,
        cd,
        a ^ cd,
        b ^ cd,
        ab ^ cd,
    ];
    for (nib, value) in values.into_iter().enumerate() {
        lo[nib] = value as u8;
        hi[nib] = (value >> 8) as u8;
    }
}

/// Scalar table-based reference: `out[j] (^)= mul_log16(input[j], log_m)` using
/// the precomputed nibble tables. Byte-identical to a direct `mul_log16` loop
/// (proven by `mul_simd_tests::tabled_matches_mul_log16`); used for the SIMD
/// remainder tail and as the property-test oracle.
pub(super) fn mulgf16_tabled_scalar<const XOR: bool>(
    out: &mut [u16],
    input: &[u16],
    t: &NibbleTables16,
) {
    for (o, &v) in out.iter_mut().zip(input.iter()) {
        let n0 = (v & 0xF) as usize;
        let n1 = ((v >> 4) & 0xF) as usize;
        let n2 = ((v >> 8) & 0xF) as usize;
        let n3 = ((v >> 12) & 0xF) as usize;
        let lo = t.lo[0][n0] ^ t.lo[1][n1] ^ t.lo[2][n2] ^ t.lo[3][n3];
        let hi = t.hi[0][n0] ^ t.hi[1][n1] ^ t.hi[2][n2] ^ t.hi[3][n3];
        let res = (lo as u16) | ((hi as u16) << 8);
        if XOR {
            *o ^= res;
        } else {
            *o = res;
        }
    }
}

/// Dispatch entry: multiply (`XOR = false`) or multiply-accumulate
/// (`XOR = true`) `input` by `g^log_m` into `out`, choosing the best available
/// SIMD kernel. Callers (`ops.rs`) have already handled the `MODULUS16`
/// short-circuit and length/endian gating decisions via [`should_use_simd`].
pub(super) fn mulgf16_simd<const XOR: bool>(
    out: &mut [u16],
    input: &[u16],
    log_m: u16,
    tables: &LeopardGf16Tables,
) {
    let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);

    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 confirmed at runtime; slices are equal length.
            unsafe {
                x86::mulgf16_avx2::<XOR>(out, input, &t);
            }
            return;
        }
        if is_x86_feature_detected!("ssse3") {
            // SAFETY: SSSE3 confirmed at runtime; slices are equal length.
            unsafe {
                x86::mulgf16_ssse3::<XOR>(out, input, &t);
            }
            return;
        }
    }

    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    // SAFETY: NEON is a mandatory baseline on aarch64; slices are equal length.
    unsafe {
        aarch64::mulgf16_neon::<XOR>(out, input, &t);
    }

    // Universal fallback: reached on targets without a kernel, and on x86_64 that
    // lacks even SSSE3 (the x86 block above returns only when a feature matched).
    // Gated out on aarch64-le, where the NEON block is the whole body, to avoid a
    // needless-return / unreachable lint.
    #[cfg(not(all(target_arch = "aarch64", target_endian = "little")))]
    mulgf16_tabled_scalar::<XOR>(out, input, &t);
}

/// Whether the SIMD path applies for this call. `false` routes callers to the
/// existing scalar loop (short slices, or targets/endianness without a kernel).
#[inline]
pub(super) fn should_use_simd(len: usize) -> bool {
    len >= SIMD_MIN_LEN && HAS_SIMD_KERNEL
}

#[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
pub(super) fn try_butterfly_avx2<const INVERSE: bool>(
    out: &mut [u16],
    input: &mut [u16],
    log_m: u16,
    tables: &LeopardGf16Tables,
) -> bool {
    if out.len() != input.len() || input.len() < 32 || !is_x86_feature_detected!("avx2") {
        return false;
    }
    // Only observe an already selected GF8 backend: GF16 must not initialize it
    // or repeatedly read its override.
    #[cfg(rse_x86_gfni)]
    if is_x86_feature_detected!("gfni")
        && matches!(
            crate::galois_8::selected_backend_id(),
            Some(
                crate::galois_8::BackendId::RustGfniAvx2
                    | crate::galois_8::BackendId::RustGfniAvx512
            )
        )
    {
        // SAFETY: GFNI and AVX2 are detected and both lanes have equal lengths.
        unsafe {
            gfni_butterfly::run::<INVERSE>(out, input, log_m, tables);
        }
        return true;
    }
    let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
    // SAFETY: AVX2 was detected; equal-length mutable slices are disjoint.
    unsafe {
        x86::butterfly_avx2::<INVERSE>(out, input, &t);
    }
    true
}

#[cfg(all(
    rse_x86_gfni,
    feature = "std",
    target_arch = "x86_64",
    target_endian = "little"
))]
mod gfni_butterfly {
    use super::{LeopardGf16Tables, mul_log16};
    use core::arch::x86_64::*;
    const SPLIT: [u8; 16] = [0, 2, 4, 6, 8, 10, 12, 14, 1, 3, 5, 7, 9, 11, 13, 15];
    const PACK: [u8; 16] = [0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15];
    // Multiplication by a fixed GF16 element is a linear map over GF(2).
    // Build its four 8x8 byte-plane matrices in the bit order used by GFNI.
    fn matrices(log_m: u16, t: &LeopardGf16Tables) -> [i64; 4] {
        let mut columns = [0u64; 4];
        for input_bit in 0..16 {
            let value = mul_log16(1u16 << input_bit, log_m, &t.log_lut, &t.exp_lut);
            let byte_shift = (input_bit % 8) * 8;
            let group = input_bit / 8;
            columns[group] |= u64::from(value & 0xff) << byte_shift;
            columns[group + 2] |= u64::from(value >> 8) << byte_shift;
        }
        columns.map(|mut x| {
            // Transpose the 8x8 bits in three swaps, then reverse GFNI row order.
            let t = (x ^ (x >> 7)) & 0x00aa_00aa_00aa_00aa;
            x ^= t ^ (t << 7);
            let t = (x ^ (x >> 14)) & 0x0000_cccc_0000_cccc;
            x ^= t ^ (t << 14);
            let t = (x ^ (x >> 28)) & 0x0000_0000_f0f0_f0f0;
            x ^= t ^ (t << 28);
            x.swap_bytes() as i64
        })
    }
    #[target_feature(enable = "gfni,avx2")]
    pub(super) unsafe fn run<const INVERSE: bool>(
        out: &mut [u16],
        input: &mut [u16],
        log_m: u16,
        t: &LeopardGf16Tables,
    ) {
        let [a, b, c, d] = matrices(log_m, t);
        let diagonal = _mm256_set_epi64x(d, a, d, a);
        let crossed = _mm256_set_epi64x(c, b, c, b);
        // SAFETY: masks contain 16 bytes, and caller supplies disjoint equal lanes and CPU features.
        unsafe {
            let split = _mm256_broadcastsi128_si256(_mm_loadu_si128(SPLIT.as_ptr().cast()));
            let pack = _mm256_broadcastsi128_si256(_mm_loadu_si128(PACK.as_ptr().cast()));
            for chunk in 0..input.len() / 16 {
                let offset = chunk * 16;
                let x = _mm256_loadu_si256(out.as_ptr().add(offset).cast());
                let y = _mm256_loadu_si256(input.as_ptr().add(offset).cast());
                let value = if INVERSE { _mm256_xor_si256(x, y) } else { y };
                let planes = _mm256_shuffle_epi8(value, split);
                let other = _mm256_shuffle_epi32::<0x4e>(planes);
                let product = _mm256_shuffle_epi8(
                    _mm256_xor_si256(
                        _mm256_gf2p8affine_epi64_epi8::<0>(planes, diagonal),
                        _mm256_gf2p8affine_epi64_epi8::<0>(other, crossed),
                    ),
                    pack,
                );
                let x_new = _mm256_xor_si256(x, product);
                let y_new = if INVERSE {
                    value
                } else {
                    _mm256_xor_si256(y, x_new)
                };
                _mm256_storeu_si256(out.as_mut_ptr().add(offset).cast(), x_new);
                _mm256_storeu_si256(input.as_mut_ptr().add(offset).cast(), y_new);
            }
        }
        let done = input.len() / 16 * 16;
        for (x, y) in out[done..].iter_mut().zip(input[done..].iter_mut()) {
            if INVERSE {
                *y ^= *x;
            }
            *x ^= mul_log16(*y, log_m, &t.log_lut, &t.exp_lut);
            if !INVERSE {
                *y ^= *x;
            }
        }
    }
    #[cfg(test)]
    mod tests {
        use super::super::super::init_leopard_gf16_tables;
        use super::{matrices, mul_log16};

        #[test]
        fn transposed_matrices_match_bitwise_reference_for_every_multiplier() {
            let tables = init_leopard_gf16_tables();
            for log_m in 0..u16::MAX {
                let mut expected = [0u64; 4];
                for input_bit in 0..16 {
                    let value =
                        mul_log16(1u16 << input_bit, log_m, &tables.log_lut, &tables.exp_lut);
                    for output_bit in 0..16 {
                        let index = (output_bit / 8) * 2 + input_bit / 8;
                        expected[index] |= (((value >> output_bit) & 1) as u64)
                            << ((7 - output_bit % 8) * 8 + input_bit % 8);
                    }
                }
                assert_eq!(
                    matrices(log_m, tables),
                    expected.map(|value| value as i64),
                    "log={log_m}"
                );
            }
        }
    }
}

// ------------------------------------------------------------------ aarch64 --

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod aarch64 {
    use super::{NibbleTables16, mulgf16_tabled_scalar};
    use core::arch::aarch64::{
        vandq_u8, vdupq_n_u8, veorq_u8, vld1q_u8, vqtbl1q_u8, vshrq_n_u8, vst1q_u8, vuzp1q_u8,
        vuzp2q_u8, vzip1q_u8, vzip2q_u8,
    };

    /// NEON kernel: 16 `u16` elements (32 interleaved bytes) per iteration.
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn mulgf16_neon<const XOR: bool>(
        out: &mut [u16],
        input: &[u16],
        t: &NibbleTables16,
    ) {
        // SAFETY: whole body runs under `#[target_feature(enable = "neon")]`
        // (baseline on aarch64). Byte views of the `u16` slices are in-bounds and
        // aligned (u16 alignment >= u8); each 128-bit load/store covers exactly 16
        // in-bounds bytes of the current 32-byte chunk; table lookups index 16-byte
        // tables with 0..15 nibbles.
        unsafe {
            let lo0 = vld1q_u8(t.lo[0].as_ptr());
            let lo1 = vld1q_u8(t.lo[1].as_ptr());
            let lo2 = vld1q_u8(t.lo[2].as_ptr());
            let lo3 = vld1q_u8(t.lo[3].as_ptr());
            let hi0 = vld1q_u8(t.hi[0].as_ptr());
            let hi1 = vld1q_u8(t.hi[1].as_ptr());
            let hi2 = vld1q_u8(t.hi[2].as_ptr());
            let hi3 = vld1q_u8(t.hi[3].as_ptr());
            let mask = vdupq_n_u8(0x0f);

            let n = input.len();
            let chunks = n / 16;
            let in_ptr = input.as_ptr().cast::<u8>();
            let out_ptr = out.as_mut_ptr().cast::<u8>();

            for c in 0..chunks {
                let off = c * 32; // 16 u16 = 32 bytes
                let p0 = vld1q_u8(in_ptr.add(off));
                let p1 = vld1q_u8(in_ptr.add(off + 16));
                // De-interleave: even bytes = low plane, odd bytes = high plane.
                let plane_lo = vuzp1q_u8(p0, p1);
                let plane_hi = vuzp2q_u8(p0, p1);

                let n0 = vandq_u8(plane_lo, mask);
                let n1 = vshrq_n_u8::<4>(plane_lo);
                let n2 = vandq_u8(plane_hi, mask);
                let n3 = vshrq_n_u8::<4>(plane_hi);

                let out_lo = veorq_u8(
                    veorq_u8(vqtbl1q_u8(lo0, n0), vqtbl1q_u8(lo1, n1)),
                    veorq_u8(vqtbl1q_u8(lo2, n2), vqtbl1q_u8(lo3, n3)),
                );
                let out_hi = veorq_u8(
                    veorq_u8(vqtbl1q_u8(hi0, n0), vqtbl1q_u8(hi1, n1)),
                    veorq_u8(vqtbl1q_u8(hi2, n2), vqtbl1q_u8(hi3, n3)),
                );

                // Re-interleave low/high planes back to LE u16 bytes.
                let mut r0 = vzip1q_u8(out_lo, out_hi);
                let mut r1 = vzip2q_u8(out_lo, out_hi);
                if XOR {
                    r0 = veorq_u8(r0, vld1q_u8(out_ptr.add(off)));
                    r1 = veorq_u8(r1, vld1q_u8(out_ptr.add(off + 16)));
                }
                vst1q_u8(out_ptr.add(off), r0);
                vst1q_u8(out_ptr.add(off + 16), r1);
            }

            let done = chunks * 16;
            if done < n {
                mulgf16_tabled_scalar::<XOR>(&mut out[done..], &input[done..], t);
            }
        }
    }
}

// ---------------------------------------------------------------------- x86 --

#[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
mod x86 {
    use super::{NibbleTables16, mulgf16_tabled_scalar};
    use core::arch::x86_64::{
        __m128i, __m256i, _mm_and_si128, _mm_loadu_si128, _mm_packus_epi16, _mm_set1_epi8,
        _mm_shuffle_epi8, _mm_srli_epi16, _mm_storeu_si128, _mm_unpackhi_epi8, _mm_unpacklo_epi8,
        _mm_xor_si128, _mm256_and_si256, _mm256_broadcastsi128_si256, _mm256_loadu_si256,
        _mm256_packus_epi16, _mm256_set1_epi8, _mm256_shuffle_epi8, _mm256_srli_epi16,
        _mm256_storeu_si256, _mm256_unpackhi_epi8, _mm256_unpacklo_epi8, _mm256_xor_si256,
    };

    // ---- SSSE3: 16 u16 (32 bytes) per iteration, 128-bit lanes ----

    #[target_feature(enable = "ssse3")]
    pub(super) unsafe fn mulgf16_ssse3<const XOR: bool>(
        out: &mut [u16],
        input: &[u16],
        t: &NibbleTables16,
    ) {
        // SAFETY: runs under `#[target_feature(enable = "ssse3")]`. Byte views of
        // the u16 slices are in-bounds/aligned; each 128-bit load/store covers 16
        // in-bounds bytes of the current 32-byte chunk; shuffles index 16-byte
        // tables with masked 0..15 nibbles.
        unsafe {
            let lo0 = _mm_loadu_si128(t.lo[0].as_ptr().cast());
            let lo1 = _mm_loadu_si128(t.lo[1].as_ptr().cast());
            let lo2 = _mm_loadu_si128(t.lo[2].as_ptr().cast());
            let lo3 = _mm_loadu_si128(t.lo[3].as_ptr().cast());
            let hi0 = _mm_loadu_si128(t.hi[0].as_ptr().cast());
            let hi1 = _mm_loadu_si128(t.hi[1].as_ptr().cast());
            let hi2 = _mm_loadu_si128(t.hi[2].as_ptr().cast());
            let hi3 = _mm_loadu_si128(t.hi[3].as_ptr().cast());
            let mask: __m128i = _mm_set1_epi8(0x0f);
            // 0x00FF per u16 lane (0xFFFF >> 8): keep the low byte of each element.
            let mask_lo: __m128i = _mm_srli_epi16::<8>(_mm_set1_epi8(-1i8));

            let n = input.len();
            let chunks = n / 16;
            let in_ptr = input.as_ptr().cast::<u8>();
            let out_ptr = out.as_mut_ptr().cast::<u8>();

            for c in 0..chunks {
                let off = c * 32;
                let p0 = _mm_loadu_si128(in_ptr.add(off).cast());
                let p1 = _mm_loadu_si128(in_ptr.add(off + 16).cast());
                // De-interleave via packus of low/high bytes of each u16.
                let plane_lo =
                    _mm_packus_epi16(_mm_and_si128(p0, mask_lo), _mm_and_si128(p1, mask_lo));
                let plane_hi = _mm_packus_epi16(_mm_srli_epi16::<8>(p0), _mm_srli_epi16::<8>(p1));

                let n0 = _mm_and_si128(plane_lo, mask);
                let n1 = _mm_and_si128(_mm_srli_epi16::<4>(plane_lo), mask);
                let n2 = _mm_and_si128(plane_hi, mask);
                let n3 = _mm_and_si128(_mm_srli_epi16::<4>(plane_hi), mask);

                let out_lo = _mm_xor_si128(
                    _mm_xor_si128(_mm_shuffle_epi8(lo0, n0), _mm_shuffle_epi8(lo1, n1)),
                    _mm_xor_si128(_mm_shuffle_epi8(lo2, n2), _mm_shuffle_epi8(lo3, n3)),
                );
                let out_hi = _mm_xor_si128(
                    _mm_xor_si128(_mm_shuffle_epi8(hi0, n0), _mm_shuffle_epi8(hi1, n1)),
                    _mm_xor_si128(_mm_shuffle_epi8(hi2, n2), _mm_shuffle_epi8(hi3, n3)),
                );

                let mut r0 = _mm_unpacklo_epi8(out_lo, out_hi);
                let mut r1 = _mm_unpackhi_epi8(out_lo, out_hi);
                if XOR {
                    r0 = _mm_xor_si128(r0, _mm_loadu_si128(out_ptr.add(off).cast()));
                    r1 = _mm_xor_si128(r1, _mm_loadu_si128(out_ptr.add(off + 16).cast()));
                }
                _mm_storeu_si128(out_ptr.add(off).cast(), r0);
                _mm_storeu_si128(out_ptr.add(off + 16).cast(), r1);
            }

            let done = chunks * 16;
            if done < n {
                mulgf16_tabled_scalar::<XOR>(&mut out[done..], &input[done..], t);
            }
        }
    }

    // ---- AVX2: 32 u16 (64 bytes) per iteration, 256-bit lanes ----

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn mulgf16_avx2<const XOR: bool>(
        out: &mut [u16],
        input: &[u16],
        t: &NibbleTables16,
    ) {
        // SAFETY: input is read-only in this instantiation; slices are disjoint.
        unsafe {
            avx2_core::<XOR, false, false>(out, input.as_ptr().cast_mut(), input.len(), t);
        }
    }

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn butterfly_avx2<const INVERSE: bool>(
        out: &mut [u16],
        input: &mut [u16],
        t: &NibbleTables16,
    ) {
        // SAFETY: both equal-sized mutable slices are disjoint and AVX2 is enabled.
        unsafe {
            avx2_core::<true, true, INVERSE>(out, input.as_mut_ptr(), input.len(), t);
        }
    }

    // Both entry points share the same nibble multiplication; butterfly mode
    // additionally updates the input lane before or after the multiply.
    #[target_feature(enable = "avx2")]
    unsafe fn avx2_core<const XOR: bool, const BUTTERFLY: bool, const INVERSE: bool>(
        out: &mut [u16],
        input: *mut u16,
        n: usize,
        t: &NibbleTables16,
    ) {
        // SAFETY: callers provide disjoint buffers of n elements and enable AVX2.
        // The input pointer is writable only in butterfly mode. Each unaligned
        // vector load/store stays inside a whole 32-element chunk; the tail is
        // bounded by n, and nibble indices are masked to the table size.
        unsafe {
            let lo0 = bcast(t.lo[0].as_ptr());
            let lo1 = bcast(t.lo[1].as_ptr());
            let lo2 = bcast(t.lo[2].as_ptr());
            let lo3 = bcast(t.lo[3].as_ptr());
            let hi0 = bcast(t.hi[0].as_ptr());
            let hi1 = bcast(t.hi[1].as_ptr());
            let hi2 = bcast(t.hi[2].as_ptr());
            let hi3 = bcast(t.hi[3].as_ptr());
            let mask: __m256i = _mm256_set1_epi8(0x0f);
            let mask_lo: __m256i = _mm256_srli_epi16::<8>(_mm256_set1_epi8(-1i8)); // 0x00FF

            let chunks = n / 32;
            let in_ptr = input.cast::<u8>();
            let out_ptr = out.as_mut_ptr().cast::<u8>();

            for c in 0..chunks {
                let off = c * 64; // 32 u16 = 64 bytes
                let y0 = _mm256_loadu_si256(in_ptr.add(off).cast());
                let y1 = _mm256_loadu_si256(in_ptr.add(off + 32).cast());
                let x0 = if XOR || BUTTERFLY {
                    _mm256_loadu_si256(out_ptr.add(off).cast())
                } else {
                    y0
                };
                let x1 = if XOR || BUTTERFLY {
                    _mm256_loadu_si256(out_ptr.add(off + 32).cast())
                } else {
                    y1
                };
                let a = if INVERSE {
                    _mm256_xor_si256(y0, x0)
                } else {
                    y0
                };
                let b = if INVERSE {
                    _mm256_xor_si256(y1, x1)
                } else {
                    y1
                };

                // Keep packus's per-128-bit lane order through nibble lookup.
                // unpacklo/hi below then yield the original contiguous vectors.
                let plane_lo =
                    _mm256_packus_epi16(_mm256_and_si256(a, mask_lo), _mm256_and_si256(b, mask_lo));
                let plane_hi =
                    _mm256_packus_epi16(_mm256_srli_epi16::<8>(a), _mm256_srli_epi16::<8>(b));

                let n0 = _mm256_and_si256(plane_lo, mask);
                let n1 = _mm256_and_si256(_mm256_srli_epi16::<4>(plane_lo), mask);
                let n2 = _mm256_and_si256(plane_hi, mask);
                let n3 = _mm256_and_si256(_mm256_srli_epi16::<4>(plane_hi), mask);

                let out_lo = _mm256_xor_si256(
                    _mm256_xor_si256(_mm256_shuffle_epi8(lo0, n0), _mm256_shuffle_epi8(lo1, n1)),
                    _mm256_xor_si256(_mm256_shuffle_epi8(lo2, n2), _mm256_shuffle_epi8(lo3, n3)),
                );
                let out_hi = _mm256_xor_si256(
                    _mm256_xor_si256(_mm256_shuffle_epi8(hi0, n0), _mm256_shuffle_epi8(hi1, n1)),
                    _mm256_xor_si256(_mm256_shuffle_epi8(hi2, n2), _mm256_shuffle_epi8(hi3, n3)),
                );

                // unpacklo selects e0-7 and e8-15; unpackhi selects e16-31.
                let mut o0 = _mm256_unpacklo_epi8(out_lo, out_hi);
                let mut o1 = _mm256_unpackhi_epi8(out_lo, out_hi);
                if XOR || BUTTERFLY {
                    o0 = _mm256_xor_si256(o0, x0);
                    o1 = _mm256_xor_si256(o1, x1);
                }
                _mm256_storeu_si256(out_ptr.add(off).cast(), o0);
                _mm256_storeu_si256(out_ptr.add(off + 32).cast(), o1);
                if BUTTERFLY {
                    let v0 = if INVERSE { a } else { _mm256_xor_si256(y0, o0) };
                    let v1 = if INVERSE { b } else { _mm256_xor_si256(y1, o1) };
                    _mm256_storeu_si256(in_ptr.add(off).cast(), v0);
                    _mm256_storeu_si256(in_ptr.add(off + 32).cast(), v1);
                }
            }

            let done = chunks * 32;
            if done < n {
                if BUTTERFLY {
                    let tail = core::slice::from_raw_parts_mut(input.add(done), n - done);
                    if INVERSE {
                        for (x, y) in out[done..].iter().zip(tail.iter_mut()) {
                            *y ^= *x;
                        }
                    }
                    mulgf16_ssse3::<true>(&mut out[done..], tail, t);
                    if !INVERSE {
                        for (x, y) in out[done..].iter().zip(tail.iter_mut()) {
                            *y ^= *x;
                        }
                    }
                } else {
                    let tail = core::slice::from_raw_parts(input.add(done), n - done);
                    mulgf16_ssse3::<XOR>(&mut out[done..], tail, t);
                }
            }
        }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn bcast(ptr: *const u8) -> __m256i {
        // SAFETY: `ptr` points at a 16-byte table row; unaligned 128-bit load then
        // broadcast to both lanes.
        unsafe { _mm256_broadcastsi128_si256(_mm_loadu_si128(ptr.cast())) }
    }
}

#[cfg(test)]
mod mul_simd_tests {
    extern crate alloc;
    use super::super::ops::mul_log16;
    use super::super::tables::build_tables16;
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    // Lengths spanning the SIMD threshold, both AVX2 (32) and SSSE3/NEON (16)
    // chunk boundaries, their tails, and a full 64 KiB lane.
    const LENGTHS: [usize; 15] = [0, 1, 5, 16, 31, 32, 33, 48, 63, 64, 65, 80, 96, 1000, 32768];

    // Direct scalar oracle (independent of the tabled path), mirroring the
    // `MODULUS16` short-circuit the callers apply.
    fn ref_mul(out: &mut [u16], input: &[u16], log_m: u16, tables: &LeopardGf16Tables, xor: bool) {
        for (o, &v) in out.iter_mut().zip(input.iter()) {
            let p = if log_m == super::super::MODULUS16 as u16 {
                v
            } else {
                mul_log16(v, log_m, &tables.log_lut, &tables.exp_lut)
            };
            if xor {
                *o ^= p;
            } else {
                *o = p;
            }
        }
    }

    // Exercises the real dispatch (SIMD kernel for len >= threshold, scalar
    // otherwise), matching `ops::mulgf16{,_xor}`.
    fn run_mul<const XOR: bool>(
        out: &mut [u16],
        input: &[u16],
        log_m: u16,
        tables: &LeopardGf16Tables,
    ) {
        if log_m == super::super::MODULUS16 as u16 {
            if XOR {
                for (o, &v) in out.iter_mut().zip(input.iter()) {
                    *o ^= v;
                }
            } else {
                out.copy_from_slice(input);
            }
            return;
        }
        if should_use_simd(input.len()) {
            mulgf16_simd::<XOR>(out, input, log_m, tables);
        } else {
            let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
            mulgf16_tabled_scalar::<XOR>(out, input, &t);
        }
    }

    #[test]
    fn nibble_tables_match_direct_products_for_every_multiplier() {
        let tables = build_tables16();
        for log_m in 0..=u16::MAX {
            let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
            for i in 0..4 {
                for nib in 0u16..16 {
                    let expected =
                        mul_log16(nib << (4 * i), log_m, &tables.log_lut, &tables.exp_lut);
                    let actual =
                        u16::from(t.lo[i][nib as usize]) | (u16::from(t.hi[i][nib as usize]) << 8);
                    assert_eq!(actual, expected, "log={log_m} position={i} nibble={nib}");
                }
            }
        }
    }

    #[test]
    fn tabled_matches_mul_log16() {
        let tables = build_tables16();
        for _ in 0..64 {
            let log_m = rand::random::<u16>();
            let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);
            let input: Vec<u16> = (0..300).map(|_| rand::random::<u16>()).collect();
            let mut got = vec![0u16; input.len()];
            mulgf16_tabled_scalar::<false>(&mut got, &input, &t);
            let mut want = vec![0u16; input.len()];
            ref_mul(&mut want, &input, log_m, &tables, false);
            assert_eq!(got, want, "tabled != mul_log16 (log_m={log_m})");
        }
    }

    #[test]
    fn simd_matches_scalar_mul_and_xor() {
        let tables = build_tables16();
        // Special values: 0 (identity via general kernel), 1, small, MODULUS16
        // (short-circuit), plus random multipliers.
        let mut log_ms: Vec<u16> = vec![0, 1, 2, 255, 256, 0xFFFE, 0xFFFF];
        for _ in 0..8 {
            log_ms.push(rand::random::<u16>());
        }
        for &log_m in &log_ms {
            for &len in &LENGTHS {
                let input: Vec<u16> = (0..len).map(|_| rand::random::<u16>()).collect();

                let mut simd = vec![0u16; len];
                run_mul::<false>(&mut simd, &input, log_m, &tables);
                let mut scal = vec![0u16; len];
                ref_mul(&mut scal, &input, log_m, &tables, false);
                assert_eq!(simd, scal, "mul mismatch len={len} log_m={log_m}");

                // XOR into a non-zero seed (the most bug-prone case).
                let seed: Vec<u16> = (0..len).map(|i| (i as u16).wrapping_mul(40503)).collect();
                let mut simd_x = seed.clone();
                run_mul::<true>(&mut simd_x, &input, log_m, &tables);
                let mut scal_x = seed.clone();
                ref_mul(&mut scal_x, &input, log_m, &tables, true);
                assert_eq!(simd_x, scal_x, "xor mismatch len={len} log_m={log_m}");
            }
        }
    }

    // Decision-gate microbenchmark (rustfs/backlog#1233): SIMD vs scalar on a
    // 64 KiB lane. Run: `cargo test --release --lib -- --ignored --nocapture
    // decision_gate_speedup`. If the speedup is < ~1.5x, escalate to Phase 2
    // (planar work-buffer refactor to drop the de-/re-interleave).
    #[cfg(feature = "std")]
    #[test]
    #[ignore]
    fn decision_gate_speedup() {
        use std::hint::black_box;
        use std::time::Instant;
        let tables = build_tables16();
        let len = 32768usize;
        let log_m: u16 = 0x1234;
        let input: Vec<u16> = (0..len).map(|i| (i as u16).wrapping_mul(2027)).collect();
        let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);

        let mut out = vec![0u16; len];
        let iters = 4000u32;

        // Warmup + time SIMD (includes per-call table build, as in production).
        for _ in 0..64 {
            mulgf16_simd::<false>(&mut out, &input, log_m, &tables);
        }
        let ts = Instant::now();
        for _ in 0..iters {
            mulgf16_simd::<false>(black_box(&mut out), black_box(&input), log_m, &tables);
        }
        let simd_ns = ts.elapsed().as_nanos() as f64 / iters as f64;

        // Time scalar table path (the portable fallback / tail).
        for _ in 0..64 {
            mulgf16_tabled_scalar::<false>(&mut out, &input, &t);
        }
        let tc = Instant::now();
        for _ in 0..iters {
            mulgf16_tabled_scalar::<false>(black_box(&mut out), black_box(&input), &t);
        }
        let scalar_ns = tc.elapsed().as_nanos() as f64 / iters as f64;

        println!(
            "\n[#1233 decision gate] len={len} log_m={log_m:#06x}\n  SIMD:   {simd_ns:>9.1} ns/call\n  scalar: {scalar_ns:>9.1} ns/call\n  speedup: {:.2}x",
            scalar_ns / simd_ns
        );
    }

    #[test]
    fn identity_and_modulus_are_input() {
        let tables = build_tables16();
        let input: Vec<u16> = (0..1000).map(|_| rand::random::<u16>()).collect();
        // log_m == 0 must go through the general kernel and reproduce the input.
        let mut out = vec![0u16; input.len()];
        run_mul::<false>(&mut out, &input, 0, &tables);
        assert_eq!(out, input, "mul by g^0 must be identity");
    }

    // Explicitly exercise the SSSE3 kernel even when AVX2 is the dispatch choice.
    #[cfg(all(feature = "std", target_arch = "x86_64", target_endian = "little"))]
    #[test]
    fn ssse3_kernel_matches_scalar() {
        if !std::is_x86_feature_detected!("ssse3") {
            return;
        }
        let tables = build_tables16();
        for &log_m in &[0u16, 1, 2, 0xABCD, 0xFFFE] {
            for &len in LENGTHS.iter().filter(|&&l| l >= 16) {
                let input: Vec<u16> = (0..len).map(|_| rand::random::<u16>()).collect();
                let t = NibbleTables16::build(log_m, &tables.log_lut, &tables.exp_lut);

                let mut got = vec![0u16; len];
                // SAFETY: guarded by the runtime SSSE3 detection above.
                unsafe {
                    super::x86::mulgf16_ssse3::<false>(&mut got, &input, &t);
                }
                let mut want = vec![0u16; len];
                ref_mul(&mut want, &input, log_m, &tables, false);
                assert_eq!(got, want, "ssse3 mul len={len} log_m={log_m}");

                let seed: Vec<u16> = (0..len).map(|i| (i as u16).wrapping_mul(7)).collect();
                let mut got_x = seed.clone();
                // SAFETY: guarded by the runtime SSSE3 detection above.
                unsafe {
                    super::x86::mulgf16_ssse3::<true>(&mut got_x, &input, &t);
                }
                let mut want_x = seed.clone();
                for (w, (&v, &s)) in want_x.iter_mut().zip(input.iter().zip(seed.iter())) {
                    let _ = s;
                    *w ^= mul_log16(v, log_m, &tables.log_lut, &tables.exp_lut);
                }
                assert_eq!(got_x, want_x, "ssse3 xor len={len} log_m={log_m}");
            }
        }
    }
}

#[cfg(all(
    test,
    feature = "std",
    target_arch = "x86_64",
    target_endian = "little"
))]
mod butterfly_tests {
    use super::super::{MODULUS16, init_leopard_gf16_tables, ops};
    #[test]
    fn fused_butterfly_matches_scalar_for_every_multiplier() {
        if !std::is_x86_feature_detected!("avx2") {
            std::println!("AVX2 unavailable: exhaustive butterfly test not executed");
            return;
        }
        std::println!("AVX2=true: exhaustive butterfly test executed");
        let tables = init_leopard_gf16_tables();
        let x0: [u16; 32] = core::array::from_fn(|i| (i as u16).wrapping_mul(9127));
        let y0: [u16; 32] = core::array::from_fn(|i| (i as u16).wrapping_mul(3571));
        for log_m in 0..u16::MAX {
            for inverse in [false, true] {
                let mut x = x0;
                let mut y = y0;
                let mut expected_x = x0;
                let mut expected_y = y0;
                for i in 0..x.len() {
                    if inverse {
                        expected_y[i] ^= expected_x[i];
                    }
                    expected_x[i] ^=
                        ops::mul_log16(expected_y[i], log_m, &tables.log_lut, &tables.exp_lut);
                    if !inverse {
                        expected_y[i] ^= expected_x[i];
                    }
                }
                let executed = if inverse {
                    super::try_butterfly_avx2::<true>(&mut x, &mut y, log_m, tables)
                } else {
                    super::try_butterfly_avx2::<false>(&mut x, &mut y, log_m, tables)
                };
                assert!(executed);
                assert_eq!(x, expected_x, "x log={log_m} inverse={inverse}");
                assert_eq!(y, expected_y, "y log={log_m} inverse={inverse}");
            }
        }
    }

    #[test]
    fn fused_butterfly_matches_two_pass_reference() {
        if !std::is_x86_feature_detected!("avx2") {
            std::println!("AVX2 unavailable: butterfly kernel not executed");
            return;
        }
        std::println!("AVX2=true: butterfly kernel executed");
        let tables = init_leopard_gf16_tables();
        let mut short = [7u16; 32];
        let mut long = [9u16; 64];
        assert!(!super::try_butterfly_avx2::<false>(
            &mut short, &mut long, 1, tables
        ));
        assert_eq!(short, [7u16; 32]);
        assert_eq!(long, [9u16; 64]);
        for len in [
            0, 1, 15, 16, 31, 32, 33, 47, 63, 64, 65, 1024, 4095, 4096, 4097, 32768,
        ] {
            for log_m in [0, 1, 31, 255, 1024, 32768, 65534, MODULUS16 as u16] {
                let x0: Vec<u16> = (0..len + 2)
                    .map(|i| ((i * 9127 + 113) & 65535) as u16)
                    .collect();
                let y0: Vec<u16> = (0..len + 2)
                    .map(|i| ((i * 3571 + 987) & 65535) as u16)
                    .collect();
                for inverse in [false, true] {
                    let mut x = x0.clone();
                    let mut y = y0.clone();
                    let mut ex = x0.clone();
                    let mut ey = y0.clone();
                    if log_m == MODULUS16 as u16 {
                        ops::slice_xor_u16(&mut ey[1..len + 1], &ex[1..len + 1]);
                    } else if inverse {
                        ops::slice_xor_u16(&mut ey[1..len + 1], &ex[1..len + 1]);
                        ops::mulgf16_xor(&mut ex[1..len + 1], &ey[1..len + 1], log_m, tables);
                    } else {
                        ops::mulgf16_xor(&mut ex[1..len + 1], &ey[1..len + 1], log_m, tables);
                        ops::slice_xor_u16(&mut ey[1..len + 1], &ex[1..len + 1]);
                    }
                    if inverse {
                        ops::dit2_step_inv16(&mut x[1..len + 1], &mut y[1..len + 1], log_m, tables);
                    } else {
                        ops::dit2_step16(&mut x[1..len + 1], &mut y[1..len + 1], log_m, tables);
                    }
                    assert_eq!(x, ex, "x len={len} log={log_m} inverse={inverse}");
                    assert_eq!(y, ey, "y len={len} log={log_m} inverse={inverse}");
                }
            }
        }
    }
}

#[cfg(all(
    test,
    feature = "std",
    target_arch = "x86_64",
    target_endian = "little"
))]
mod lane_order_tests {
    use super::super::{init_leopard_gf16_tables, ops};
    #[test]
    fn avx2_mul_and_xor_match_every_field_value() {
        if !std::is_x86_feature_detected!("avx2") {
            std::println!("AVX2 unavailable");
            return;
        }
        std::println!("AVX2=true: lane-order kernel executed");
        let tables = init_leopard_gf16_tables();
        let input: Vec<u16> = (0..=u16::MAX).collect();
        for log_m in [0, 1, 31, 1024, 32768, 65534] {
            for xor in [false, true] {
                let seed: Vec<u16> = (0..input.len())
                    .map(|i| (i as u16).wrapping_mul(40503))
                    .collect();
                let mut actual = seed.clone();
                let mut expected = seed;
                for (out, &value) in expected.iter_mut().zip(&input) {
                    let product = ops::mul_log16(value, log_m, &tables.log_lut, &tables.exp_lut);
                    if xor {
                        *out ^= product;
                    } else {
                        *out = product;
                    }
                }
                if xor {
                    super::mulgf16_simd::<true>(&mut actual, &input, log_m, tables);
                } else {
                    super::mulgf16_simd::<false>(&mut actual, &input, log_m, tables);
                }
                assert_eq!(actual, expected, "multiplier={log_m} xor={xor}");
            }
        }
    }
}

#[cfg(all(
    test,
    rse_x86_gfni,
    feature = "std",
    target_arch = "x86_64",
    target_endian = "little"
))]
mod gfni_butterfly_tests {
    use super::super::{init_leopard_gf16_tables, ops};
    #[test]
    fn gfni_butterfly_handles_unaligned_tails_without_overwriting_guards() {
        if !std::is_x86_feature_detected!("gfni") || !std::is_x86_feature_detected!("avx2") {
            std::println!("GFNI unavailable");
            return;
        }
        let tables = init_leopard_gf16_tables();
        for len in [0, 1, 15, 16, 17, 31, 32, 33, 4095, 4096, 4097] {
            for inverse in [false, true] {
                let mut x: Vec<u16> = (0..len + 2)
                    .map(|i| (i as u16).wrapping_mul(9127))
                    .collect();
                let mut y: Vec<u16> = (0..len + 2)
                    .map(|i| (i as u16).wrapping_mul(3571))
                    .collect();
                let mut expected_x = x.clone();
                let mut expected_y = y.clone();
                for (a, b) in expected_x[1..len + 1]
                    .iter_mut()
                    .zip(expected_y[1..len + 1].iter_mut())
                {
                    if inverse {
                        *b ^= *a;
                    }
                    *a ^= ops::mul_log16(*b, 1024, &tables.log_lut, &tables.exp_lut);
                    if !inverse {
                        *b ^= *a;
                    }
                }
                // SAFETY: features were detected and the equal-sized lanes are disjoint.
                unsafe {
                    if inverse {
                        super::gfni_butterfly::run::<true>(
                            &mut x[1..len + 1],
                            &mut y[1..len + 1],
                            1024,
                            tables,
                        );
                    } else {
                        super::gfni_butterfly::run::<false>(
                            &mut x[1..len + 1],
                            &mut y[1..len + 1],
                            1024,
                            tables,
                        );
                    }
                }
                assert_eq!(x, expected_x, "x len={len} inverse={inverse}");
                assert_eq!(y, expected_y, "y len={len} inverse={inverse}");
            }
        }
    }

    #[test]
    fn gfni_butterfly_matches_scalar_for_every_multiplier_and_basis_vector() {
        if !std::is_x86_feature_detected!("gfni") || !std::is_x86_feature_detected!("avx2") {
            std::println!("GFNI unavailable");
            return;
        }
        let tables = init_leopard_gf16_tables();
        for log_m in 0..u16::MAX {
            for inverse in [false, true] {
                let mut x = [0x5a5au16; 16];
                let mut y = core::array::from_fn::<_, 16, _>(|i| {
                    (1u16 << i) ^ if inverse { 0x5a5a } else { 0 }
                });
                let mut expected_x = x;
                let mut expected_y = y;
                for (a, b) in expected_x.iter_mut().zip(expected_y.iter_mut()) {
                    if inverse {
                        *b ^= *a;
                    }
                    *a ^= ops::mul_log16(*b, log_m, &tables.log_lut, &tables.exp_lut);
                    if !inverse {
                        *b ^= *a;
                    }
                }
                // SAFETY: features were detected and the equal-sized lanes are disjoint.
                unsafe {
                    if inverse {
                        super::gfni_butterfly::run::<true>(&mut x, &mut y, log_m, tables);
                    } else {
                        super::gfni_butterfly::run::<false>(&mut x, &mut y, log_m, tables);
                    }
                }
                assert_eq!(x, expected_x, "x log={log_m} inverse={inverse}");
                assert_eq!(y, expected_y, "y log={log_m} inverse={inverse}");
            }
        }
    }

    #[test]
    fn gfni_butterfly_matches_scalar_for_every_field_value() {
        if !std::is_x86_feature_detected!("gfni") || !std::is_x86_feature_detected!("avx2") {
            std::println!("GFNI unavailable");
            return;
        }
        std::println!("GFNI=true AVX2=true: GFNI butterfly executed");
        let tables = init_leopard_gf16_tables();
        let initial: Vec<u16> = (0..=u16::MAX).collect();
        for log_m in [0, 1, 31, 1024, 32768, 65534] {
            for inverse in [false, true] {
                let mut x: Vec<u16> = initial.iter().map(|v| v.wrapping_mul(40503)).collect();
                let mut y = initial.clone();
                let mut ex = x.clone();
                let mut ey = y.clone();
                for (a, b) in ex.iter_mut().zip(ey.iter_mut()) {
                    if inverse {
                        *b ^= *a;
                    }
                    *a ^= ops::mul_log16(*b, log_m, &tables.log_lut, &tables.exp_lut);
                    if !inverse {
                        *b ^= *a;
                    }
                }
                // SAFETY: feature detection succeeded and mutable lanes are disjoint and equal sized.
                unsafe {
                    if inverse {
                        super::gfni_butterfly::run::<true>(&mut x, &mut y, log_m, tables)
                    } else {
                        super::gfni_butterfly::run::<false>(&mut x, &mut y, log_m, tables)
                    }
                }
                assert_eq!(x, ex, "x log={log_m} inverse={inverse}");
                assert_eq!(y, ey, "y log={log_m} inverse={inverse}");
            }
        }
    }
}
