//! GF(2^16) implementation.
//!
//! More accurately, this is a `GF((2^8)^2)` implementation which builds an extension
//! field of `GF(2^8)`, as defined in the `galois_8` module.

use crate::galois_8;
use core::ops::{Add, Div, Mul, Sub};

// Irreducible extension polynomial over GF(2^8):
// X^2 + a*X + a^7, where `a` is the GF(2^8) generator.
const EXT_POLY: [u8; 3] = [1, 2, 128];

/// The field GF(2^16).
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Field;

impl crate::Field for Field {
    const ORDER: usize = 65536;

    type Elem = [u8; 2];

    fn add(a: [u8; 2], b: [u8; 2]) -> [u8; 2] {
        (Element(a) + Element(b)).0
    }

    fn mul(a: [u8; 2], b: [u8; 2]) -> [u8; 2] {
        (Element(a) * Element(b)).0
    }

    fn div(a: [u8; 2], b: [u8; 2]) -> [u8; 2] {
        (Element(a) / Element(b)).0
    }

    fn exp(elem: [u8; 2], n: usize) -> [u8; 2] {
        Element(elem).exp(n).0
    }

    fn zero() -> [u8; 2] {
        [0; 2]
    }

    fn one() -> [u8; 2] {
        [0, 1]
    }

    fn nth_internal(n: usize) -> [u8; 2] {
        [(n >> 8) as u8, n as u8]
    }

    fn mul_slice(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]]) {
        gf16_mul_slice(elem, input, out, false);
    }

    fn mul_slice_add(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]]) {
        gf16_mul_slice(elem, input, out, true);
    }
}

/// SIMD-accelerated `out[i] = elem * input[i]` (or `out[i] ^= elem * input[i]`
/// when `accumulate`) over the `GF((2^8)^2)` tower field.
///
/// A GF(2^16) element `ax·X + ac` multiplied by the fixed `elem = cx·X + cc`
/// reduces (via `X² = 2·X + 128`, from [`EXT_POLY`]) to two GF(2^8) output
/// planes, each a linear combination of the `ax`/`ac` input byte planes:
///
/// * `out_x = A·ax ^ B·ac`, `out_c = C·ax ^ D·ac`
/// * `A = cc ^ 2·cx`, `B = cx`, `C = 128·cx`, `D = cc` (all in GF(2^8)).
///
/// Each `·` is a fixed-multiplier GF(2^8) slice multiply, so this reuses the
/// SIMD-accelerated [`galois_8::mul_slice`]/[`galois_8::mul_slice_xor`] backends
/// (ssse3/avx2/gfni/avx512/neon/vsx with runtime dispatch). Byte planes are
/// de-/re-interleaved in fixed stack chunks via [`deinterleave`]/[`interleave`]
/// (SIMD ssse3/neon, scalar fallback) to stay `no_std` and allocation free; the
/// whole path is byte-wise GF(2^8), so it is endian-agnostic.
///
/// On x86 with GFNI the GF(2^8) multiplies are so fast that a *scalar* byte
/// de-/re-interleave dominates the runtime (Amdahl); SIMD-ing the layout
/// conversion is what restores the tower decomposition's speed-up there.
fn gf16_mul_slice(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]], accumulate: bool) {
    assert_eq!(input.len(), out.len());

    let cx = elem[0];
    let cc = elem[1];
    let coef_a = cc ^ galois_8::mul(2, cx);
    let coef_b = cx;
    let coef_c = galois_8::mul(128, cx);
    let coef_d = cc;

    const CHUNK: usize = 1024;
    let mut ax = [0u8; CHUNK];
    let mut ac = [0u8; CHUNK];
    let mut ox = [0u8; CHUNK];
    let mut oc = [0u8; CHUNK];

    let mut offset = 0;
    while offset < input.len() {
        let n = core::cmp::min(CHUNK, input.len() - offset);

        // Split the interleaved `[[u8; 2]]` input into the two GF(2^8) byte
        // planes `ax`/`ac`. `as_flattened` is the safe `&[[u8; 2]] -> &[u8]`
        // byte view; the split is a pure byte permutation (endian-agnostic).
        deinterleave(
            input[offset..offset + n].as_flattened(),
            &mut ax[..n],
            &mut ac[..n],
        );

        if accumulate {
            deinterleave(
                out[offset..offset + n].as_flattened(),
                &mut ox[..n],
                &mut oc[..n],
            );
            galois_8::mul_slice_xor(coef_a, &ax[..n], &mut ox[..n]);
            galois_8::mul_slice_xor(coef_b, &ac[..n], &mut ox[..n]);
            galois_8::mul_slice_xor(coef_c, &ax[..n], &mut oc[..n]);
            galois_8::mul_slice_xor(coef_d, &ac[..n], &mut oc[..n]);
        } else {
            galois_8::mul_slice(coef_a, &ax[..n], &mut ox[..n]);
            galois_8::mul_slice_xor(coef_b, &ac[..n], &mut ox[..n]);
            galois_8::mul_slice(coef_c, &ax[..n], &mut oc[..n]);
            galois_8::mul_slice_xor(coef_d, &ac[..n], &mut oc[..n]);
        }

        // Re-interleave the `ox`/`oc` planes back into the output elements.
        interleave(
            &ox[..n],
            &oc[..n],
            out[offset..offset + n].as_flattened_mut(),
        );
        offset += n;
    }
}

/// De-interleave interleaved GF(2^16) element bytes into two contiguous GF(2^8)
/// byte planes: `even[i] = src[2*i]`, `odd[i] = src[2*i + 1]`.
///
/// Requires `src.len() == 2 * even.len()` and `even.len() == odd.len()`. This is
/// a pure byte permutation, so it is endian-agnostic. SIMD-accelerated on
/// ssse3 (x86_64) and neon (aarch64), with a scalar fallback that also handles
/// the sub-32-element tail of the SIMD paths.
#[allow(clippy::needless_return)]
#[inline]
fn deinterleave(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    debug_assert_eq!(src.len(), even.len() * 2);
    debug_assert_eq!(even.len(), odd.len());

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        // 128-bit ssse3 kernel — covers every AVX2/GFNI CPU too, so the fast
        // GF(2^8) backends no longer stall on a scalar layout conversion.
        if is_x86_feature_detected!("ssse3") {
            // SAFETY: ssse3 confirmed at runtime, matching the callee's
            // `#[target_feature(enable = "ssse3")]`; the length contract above
            // matches what the kernel indexes.
            unsafe {
                deinterleave_ssse3(src, even, odd);
                return;
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is baseline on aarch64, so calling the
        // `#[target_feature(enable = "neon")]` fn is sound; the length contract
        // above matches what the kernel indexes.
        unsafe {
            deinterleave_neon(src, even, odd);
            return;
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    deinterleave_scalar(src, even, odd);
}

/// Re-interleave two GF(2^8) byte planes into GF(2^16) element bytes:
/// `dst[2*i] = even[i]`, `dst[2*i + 1] = odd[i]`. Inverse of [`deinterleave`].
///
/// Requires `dst.len() == 2 * even.len()` and `even.len() == odd.len()`.
#[allow(clippy::needless_return)]
#[inline]
fn interleave(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(dst.len(), even.len() * 2);
    debug_assert_eq!(even.len(), odd.len());

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("ssse3") {
            // SAFETY: ssse3 confirmed at runtime, matching the callee; the
            // length contract above matches what the kernel indexes.
            unsafe {
                interleave_ssse3(even, odd, dst);
                return;
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is baseline on aarch64; the length contract above matches
        // what the kernel indexes.
        unsafe {
            interleave_neon(even, odd, dst);
            return;
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    interleave_scalar(even, odd, dst);
}

fn deinterleave_scalar(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    for (i, (e, o)) in even.iter_mut().zip(odd.iter_mut()).enumerate() {
        *e = src[2 * i];
        *o = src[2 * i + 1];
    }
}

fn interleave_scalar(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    for (i, (e, o)) in even.iter().zip(odd.iter()).enumerate() {
        dst[2 * i] = *e;
        dst[2 * i + 1] = *o;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn deinterleave_ssse3(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    use core::arch::x86_64::{_mm_loadu_si128, _mm_shuffle_epi8, _mm_storel_epi64};

    // Extract even/odd bytes into the low 8 lanes; the high 8 are zeroed (0x80).
    #[rustfmt::skip]
    // SAFETY: the 16-byte array literal backs a valid 128-bit unaligned load of the shuffle mask.
    let even_mask = unsafe { _mm_loadu_si128([
        0u8, 2, 4, 6, 8, 10, 12, 14, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    ].as_ptr().cast()) };
    #[rustfmt::skip]
    // SAFETY: the 16-byte array literal backs a valid 128-bit unaligned load of the shuffle mask.
    let odd_mask = unsafe { _mm_loadu_si128([
        1u8, 3, 5, 7, 9, 11, 13, 15, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    ].as_ptr().cast()) };

    let batches = even.len() / 32;
    for b in 0..batches {
        let s = &src[b * 64..b * 64 + 64];
        let e = &mut even[b * 32..b * 32 + 32];
        let o = &mut odd[b * 32..b * 32 + 32];
        // SAFETY: the slices above are exactly 64/32/32 bytes, so the four
        // 128-bit loads at 0/16/32/48 and the eight 8-byte `storel_epi64` stores
        // at 0/8/16/24 of `e` and `o` are all in-bounds; ssse3 confirmed by caller.
        unsafe {
            let p0 = _mm_loadu_si128(s.as_ptr().cast());
            let p1 = _mm_loadu_si128(s[16..].as_ptr().cast());
            let p2 = _mm_loadu_si128(s[32..].as_ptr().cast());
            let p3 = _mm_loadu_si128(s[48..].as_ptr().cast());
            _mm_storel_epi64(e.as_mut_ptr().cast(), _mm_shuffle_epi8(p0, even_mask));
            _mm_storel_epi64(e[8..].as_mut_ptr().cast(), _mm_shuffle_epi8(p1, even_mask));
            _mm_storel_epi64(e[16..].as_mut_ptr().cast(), _mm_shuffle_epi8(p2, even_mask));
            _mm_storel_epi64(e[24..].as_mut_ptr().cast(), _mm_shuffle_epi8(p3, even_mask));
            _mm_storel_epi64(o.as_mut_ptr().cast(), _mm_shuffle_epi8(p0, odd_mask));
            _mm_storel_epi64(o[8..].as_mut_ptr().cast(), _mm_shuffle_epi8(p1, odd_mask));
            _mm_storel_epi64(o[16..].as_mut_ptr().cast(), _mm_shuffle_epi8(p2, odd_mask));
            _mm_storel_epi64(o[24..].as_mut_ptr().cast(), _mm_shuffle_epi8(p3, odd_mask));
        }
    }
    let done = batches * 32;
    deinterleave_scalar(&src[done * 2..], &mut even[done..], &mut odd[done..]);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn interleave_ssse3(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    use core::arch::x86_64::{
        _mm_loadu_si128, _mm_storeu_si128, _mm_unpackhi_epi8, _mm_unpacklo_epi8,
    };

    let batches = even.len() / 32;
    for b in 0..batches {
        let e = &even[b * 32..b * 32 + 32];
        let o = &odd[b * 32..b * 32 + 32];
        let d = &mut dst[b * 64..b * 64 + 64];
        // SAFETY: the slices above are exactly 32/32/64 bytes, so the four
        // 128-bit loads at 0/16 of `e`/`o` and the four 128-bit stores at
        // 0/16/32/48 of `d` are all in-bounds; ssse3 confirmed by caller.
        unsafe {
            let lo = _mm_loadu_si128(e.as_ptr().cast());
            let hi = _mm_loadu_si128(o.as_ptr().cast());
            _mm_storeu_si128(d.as_mut_ptr().cast(), _mm_unpacklo_epi8(lo, hi));
            _mm_storeu_si128(d[16..].as_mut_ptr().cast(), _mm_unpackhi_epi8(lo, hi));
            let lo2 = _mm_loadu_si128(e[16..].as_ptr().cast());
            let hi2 = _mm_loadu_si128(o[16..].as_ptr().cast());
            _mm_storeu_si128(d[32..].as_mut_ptr().cast(), _mm_unpacklo_epi8(lo2, hi2));
            _mm_storeu_si128(d[48..].as_mut_ptr().cast(), _mm_unpackhi_epi8(lo2, hi2));
        }
    }
    let done = batches * 32;
    interleave_scalar(&even[done..], &odd[done..], &mut dst[done * 2..]);
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn deinterleave_neon(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    use core::arch::aarch64::{vld1q_u8, vst1q_u8, vuzp1q_u8, vuzp2q_u8};

    let batches = even.len() / 32;
    for b in 0..batches {
        let s = &src[b * 64..b * 64 + 64];
        let e = &mut even[b * 32..b * 32 + 32];
        let o = &mut odd[b * 32..b * 32 + 32];
        // vuzp1q extracts even-indexed bytes, vuzp2q the odd-indexed ones.
        // SAFETY: the slices above are exactly 64/32/32 bytes, so the 128-bit
        // loads at 0/16/32/48 of `s` and the 128-bit stores at 0/16 of `e`/`o`
        // are in-bounds; NEON is baseline on aarch64.
        unsafe {
            let p0 = vld1q_u8(s.as_ptr());
            let p1 = vld1q_u8(s[16..].as_ptr());
            vst1q_u8(e.as_mut_ptr(), vuzp1q_u8(p0, p1));
            vst1q_u8(o.as_mut_ptr(), vuzp2q_u8(p0, p1));
            let p2 = vld1q_u8(s[32..].as_ptr());
            let p3 = vld1q_u8(s[48..].as_ptr());
            vst1q_u8(e[16..].as_mut_ptr(), vuzp1q_u8(p2, p3));
            vst1q_u8(o[16..].as_mut_ptr(), vuzp2q_u8(p2, p3));
        }
    }
    let done = batches * 32;
    deinterleave_scalar(&src[done * 2..], &mut even[done..], &mut odd[done..]);
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn interleave_neon(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    use core::arch::aarch64::{vld1q_u8, vst1q_u8, vzip1q_u8, vzip2q_u8};

    let batches = even.len() / 32;
    for b in 0..batches {
        let e = &even[b * 32..b * 32 + 32];
        let o = &odd[b * 32..b * 32 + 32];
        let d = &mut dst[b * 64..b * 64 + 64];
        // vzip1q/vzip2q interleave the low/high 16 bytes of the two planes.
        // SAFETY: the slices above are exactly 32/32/64 bytes, so the 128-bit
        // loads at 0/16 of `e`/`o` and the stores at 0/16/32/48 of `d` are
        // in-bounds; NEON is baseline on aarch64.
        unsafe {
            let lo = vld1q_u8(e.as_ptr());
            let hi = vld1q_u8(o.as_ptr());
            vst1q_u8(d.as_mut_ptr(), vzip1q_u8(lo, hi));
            vst1q_u8(d[16..].as_mut_ptr(), vzip2q_u8(lo, hi));
            let lo2 = vld1q_u8(e[16..].as_ptr());
            let hi2 = vld1q_u8(o[16..].as_ptr());
            vst1q_u8(d[32..].as_mut_ptr(), vzip1q_u8(lo2, hi2));
            vst1q_u8(d[48..].as_mut_ptr(), vzip2q_u8(lo2, hi2));
        }
    }
    let done = batches * 32;
    interleave_scalar(&even[done..], &odd[done..], &mut dst[done * 2..]);
}

/// Type alias of ReedSolomon over GF(2^8).
pub type ReedSolomon = crate::ReedSolomon<Field>;

/// Type alias of ShardByShard over GF(2^8).
pub type ShardByShard<'a> = crate::ShardByShard<'a, Field>;

/// An element of `GF(2^16)`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct Element(pub [u8; 2]);

impl Element {
    // Create the zero element.
    fn zero() -> Self {
        Element([0, 0])
    }

    // A constant element evaluating to `n`.
    fn constant(n: u8) -> Element {
        Element([0, n])
    }

    // Whether this is the zero element.
    fn is_zero(&self) -> bool {
        self.0 == [0; 2]
    }

    fn exp(mut self, n: usize) -> Element {
        if n == 0 {
            Element::constant(1)
        } else if self == Element::zero() {
            Element::zero()
        } else {
            let x = self;
            for _ in 1..n {
                self = self * x;
            }

            self
        }
    }

    // reduces from some polynomial with degree <= 2.
    #[inline]
    fn reduce_from(mut x: [u8; 3]) -> Self {
        if x[0] != 0 {
            // divide x by EXT_POLY and use remainder.
            // i = 0 here.
            // c*x^(i+j)  = a*x^i*b*x^j
            x[1] ^= galois_8::mul(EXT_POLY[1], x[0]);
            x[2] ^= galois_8::mul(EXT_POLY[2], x[0]);
        }

        Element([x[1], x[2]])
    }

    fn degree(&self) -> usize {
        if self.0[0] != 0 { 1 } else { 0 }
    }
}

impl From<[u8; 2]> for Element {
    fn from(c: [u8; 2]) -> Self {
        Element(c)
    }
}

impl Default for Element {
    fn default() -> Self {
        Element::zero()
    }
}

impl Add for Element {
    type Output = Element;

    fn add(self, other: Self) -> Element {
        Element([self.0[0] ^ other.0[0], self.0[1] ^ other.0[1]])
    }
}

impl Sub for Element {
    type Output = Element;

    fn sub(self, other: Self) -> Element {
        self.add(other)
    }
}

impl Mul for Element {
    type Output = Element;

    fn mul(self, rhs: Self) -> Element {
        // FOIL; our elements are linear at most, with two coefficients
        let out: [u8; 3] = [
            galois_8::mul(self.0[0], rhs.0[0]),
            galois_8::add(
                galois_8::mul(self.0[1], rhs.0[0]),
                galois_8::mul(self.0[0], rhs.0[1]),
            ),
            galois_8::mul(self.0[1], rhs.0[1]),
        ];

        Element::reduce_from(out)
    }
}

impl Mul<u8> for Element {
    type Output = Element;

    fn mul(self, rhs: u8) -> Element {
        Element([galois_8::mul(rhs, self.0[0]), galois_8::mul(rhs, self.0[1])])
    }
}

impl Div for Element {
    type Output = Element;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, rhs: Self) -> Element {
        self * rhs.inverse()
    }
}

// helpers for division.

#[derive(Debug)]
enum EgcdRhs {
    Element(Element),
    ExtPoly,
}

impl Element {
    // compute extended euclidean algorithm against an element of self,
    // where the GCD is known to be constant.
    fn const_egcd(self, rhs: EgcdRhs) -> (u8, Element, Element) {
        if self.is_zero() {
            let rhs = match rhs {
                EgcdRhs::Element(elem) => elem,
                EgcdRhs::ExtPoly => {
                    debug_assert!(false, "const_egcd invoked with divisible");
                    Element::constant(1)
                }
            };
            (rhs.0[1], Element::constant(0), Element::constant(1))
        } else {
            let (cur_quotient, cur_remainder) = match rhs {
                EgcdRhs::Element(rhs) => rhs.polynom_div(self),
                EgcdRhs::ExtPoly => Element::div_ext_by(self),
            };

            // GCD is constant because EXT_POLY is irreducible
            let (g, x, y) = cur_remainder.const_egcd(EgcdRhs::Element(self));
            (g, y + (cur_quotient * x), x)
        }
    }

    // divide EXT_POLY by self.
    fn div_ext_by(rhs: Self) -> (Element, Element) {
        if rhs.degree() == 0 {
            // dividing by constant is the same as multiplying by another constant.
            // and all constant multiples of EXT_POLY are in the equivalence class
            // of 0.
            return (Element::zero(), Element::zero());
        }

        // divisor is ensured linear here.
        // now ensure divisor is monic.
        let leading_mul_inv = galois_8::div(1, rhs.0[0]);

        let monictized = rhs * leading_mul_inv;
        let mut poly = EXT_POLY;

        for i in 0..2 {
            let coef = poly[i];
            for j in 1..2 {
                if rhs.0[j] != 0 {
                    poly[i + j] ^= galois_8::mul(monictized.0[j], coef);
                }
            }
        }

        let remainder = Element::constant(poly[2]);
        let quotient = Element([poly[0], poly[1]]) * leading_mul_inv;

        (quotient, remainder)
    }

    fn polynom_div(self, rhs: Self) -> (Element, Element) {
        let divisor_degree = rhs.degree();
        if rhs.is_zero() {
            (Element::zero(), self)
        } else if self.degree() < divisor_degree {
            // If divisor's degree (len-1) is bigger, all dividend is a remainder
            (Element::zero(), self)
        } else if divisor_degree == 0 {
            // divide by constant.
            let invert = galois_8::div(1, rhs.0[1]);
            let quotient = Element([
                galois_8::mul(invert, self.0[0]),
                galois_8::mul(invert, self.0[1]),
            ]);

            (quotient, Element::zero())
        } else {
            // self degree is at least divisor degree, divisor degree not 0.
            // therefore both are 1.
            debug_assert_eq!(self.degree(), divisor_degree);
            debug_assert_eq!(self.degree(), 1);

            // ensure rhs is constant.
            let leading_mul_inv = galois_8::div(1, rhs.0[0]);
            let monic = Element([
                galois_8::mul(leading_mul_inv, rhs.0[0]),
                galois_8::mul(leading_mul_inv, rhs.0[1]),
            ]);

            let leading_coeff = self.0[0];
            let mut remainder = self.0[1];

            if monic.0[1] != 0 {
                remainder ^= galois_8::mul(monic.0[1], self.0[0]);
            }

            (
                Element::constant(galois_8::mul(leading_mul_inv, leading_coeff)),
                Element::constant(remainder),
            )
        }
    }

    /// Convert the inverse of this field element. Returns zero for zero input.
    fn inverse(self) -> Element {
        if self.is_zero() {
            return Element::zero();
        }

        // first step of extended euclidean algorithm.
        // done here because EXT_POLY is outside the scope of `Element`.
        let (gcd, y) = {
            // self / EXT_POLY = (0, self)
            let remainder = self;

            // GCD is constant because EXT_POLY is irreducible
            let (g, x, _) = remainder.const_egcd(EgcdRhs::ExtPoly);

            (g, x)
        };

        // we still need to normalize it by dividing by the gcd
        if gcd != 0 {
            // EXT_POLY is irreducible so the GCD will always be constant.
            // EXT_POLY*x + self*y = gcd
            // self*y = gcd - EXT_POLY*x
            //
            // EXT_POLY*x is representative of the equivalence class of 0.
            let normalizer = galois_8::div(1, gcd);
            y * normalizer
        } else {
            // self is equivalent to zero.
            Element::zero()
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::{vec, vec::Vec};

    use super::*;
    use crate::Field as _;
    use quickcheck::Arbitrary;

    impl Arbitrary for Element {
        fn arbitrary(gens: &mut quickcheck::Gen) -> Self {
            let a = u8::arbitrary(gens);
            let b = u8::arbitrary(gens);

            Element([a, b])
        }
    }

    quickcheck! {
        fn qc_add_associativity(a: Element, b: Element, c: Element) -> bool {
            a + (b + c) == (a + b) + c
        }

        fn qc_mul_associativity(a: Element, b: Element, c: Element) -> bool {
            a * (b * c) == (a * b) * c
        }

        fn qc_additive_identity(a: Element) -> bool {
            let zero = Element::zero();
            a - (zero - a) == zero
        }

        fn qc_multiplicative_identity(a: Element) -> bool {
            a.is_zero() || {
                let one = Element([0, 1]);
                (one / a) * a == one
            }
        }

        fn qc_add_commutativity(a: Element, b: Element) -> bool {
            a + b == b + a
        }

        fn qc_mul_commutativity(a: Element, b: Element) -> bool {
            a * b == b * a
        }

        fn qc_add_distributivity(a: Element, b: Element, c: Element) -> bool {
            a * (b + c) == (a * b) + (a * c)
        }

        fn qc_inverse(a: Element) -> bool {
            a.is_zero() || {
                let inv = a.inverse();
                a * inv == Element::constant(1)
            }
        }

        fn qc_exponent_1(a: Element, n: u8) -> bool {
            a.is_zero() || n == 0 || {
                let mut b = a.exp(n as usize);
                for _ in 1..n {
                    b = b / a;
                }

                a == b
            }
        }

        fn qc_exponent_2(a: Element, n: u8) -> bool {
            a.is_zero() || {
                let mut res = true;
                let mut b = Element::constant(1);

                for i in 0..n {
                    res = res && b == a.exp(i as usize);
                    b = b * a;
                }

                res
            }
        }

        fn qc_exp_zero_is_one(a: Element) -> bool {
            a.exp(0) == Element::constant(1)
        }
    }

    #[test]
    fn test_div_b_is_0() {
        assert_eq!(Element::zero(), Element([1, 0]) / Element::zero());
    }

    #[test]
    fn zero_to_zero_is_one() {
        assert_eq!(Element::zero().exp(0), Element::constant(1))
    }

    // Verifies the SIMD-decomposed `mul_slice`/`mul_slice_add` overrides against
    // the element-wise scalar `Field::mul` reference across a range of lengths
    // (including CHUNK boundary and tail) and multiplier values. If the A/B/C/D
    // tower-field decomposition were wrong, this would catch it.
    #[test]
    fn mul_slice_matches_scalar_reference() {
        const N: usize = 2100; // spans past the 1024 internal CHUNK, with a tail
        let mut input = [[0u8; 2]; N];
        for (i, e) in input.iter_mut().enumerate() {
            *e = [
                (i.wrapping_mul(31).wrapping_add(7)) as u8,
                (i.wrapping_mul(17).wrapping_add(3)) as u8,
            ];
        }

        let coeffs = [
            [0u8, 0],  // zero
            [0, 1],    // multiplicative identity
            [0, 0x8e], // pure constant coefficient
            [0x9a, 0], // pure X coefficient
            [0x9a, 0x3f],
            [0xff, 0xff],
            [0x01, 0x80],
        ];
        let lens = [0usize, 1, 2, 7, 16, 17, 63, 1023, 1024, 1025, N];

        for &c in &coeffs {
            for &len in &lens {
                let inp = &input[..len];

                // mul_slice: out = c * inp
                let mut out = [[0u8; 2]; N];
                Field::mul_slice(c, inp, &mut out[..len]);
                for (i, &e) in inp.iter().enumerate() {
                    assert_eq!(
                        out[i],
                        Field::mul(c, e),
                        "mul_slice c={c:?} len={len} i={i}"
                    );
                }

                // mul_slice_add: out ^= c * inp, starting from a nonzero seed
                let mut acc = [[0u8; 2]; N];
                for (i, e) in acc.iter_mut().enumerate() {
                    *e = [
                        (i.wrapping_mul(13).wrapping_add(5)) as u8,
                        (i.wrapping_mul(19).wrapping_add(11)) as u8,
                    ];
                }
                let seed = acc;
                Field::mul_slice_add(c, inp, &mut acc[..len]);
                for (i, &e) in inp.iter().enumerate() {
                    let expected = Field::add(seed[i], Field::mul(c, e));
                    assert_eq!(acc[i], expected, "mul_slice_add c={c:?} len={len} i={i}");
                }
            }
        }
    }

    // Verifies the SIMD `deinterleave`/`interleave` (whatever path the host CPU
    // dispatches to) is byte-exact against the scalar reference and round-trips
    // to the identity, across lengths that exercise the 32-element SIMD batch
    // boundary and every sub-32 tail. The runtime-dispatch `deinterleave`
    // compared against `deinterleave_scalar` also cross-checks the active SIMD
    // kernel on the build target (neon on aarch64, ssse3 on x86_64).
    #[test]
    fn deinterleave_interleave_match_scalar_and_round_trip() {
        // Deterministic interleaved input; distinct even/odd byte streams so a
        // swapped/misaligned plane would show up immediately.
        fn src_byte(i: usize) -> u8 {
            (i.wrapping_mul(37).wrapping_add(i / 2).wrapping_add(1)) as u8
        }

        let lens = [
            0usize, 1, 2, 3, 7, 15, 16, 17, 31, 32, 33, 47, 63, 64, 65, 96, 127, 128, 1000, 1024,
        ];

        for &n in &lens {
            let src: Vec<u8> = (0..2 * n).map(src_byte).collect();

            let mut even = vec![0u8; n];
            let mut odd = vec![0u8; n];
            deinterleave(&src, &mut even, &mut odd);

            let mut even_ref = vec![0u8; n];
            let mut odd_ref = vec![0u8; n];
            deinterleave_scalar(&src, &mut even_ref, &mut odd_ref);
            assert_eq!(even, even_ref, "deinterleave even plane, n={n}");
            assert_eq!(odd, odd_ref, "deinterleave odd plane, n={n}");
            for i in 0..n {
                assert_eq!(even[i], src[2 * i], "even[{i}] != src[2i], n={n}");
                assert_eq!(odd[i], src[2 * i + 1], "odd[{i}] != src[2i+1], n={n}");
            }

            let mut dst = vec![0u8; 2 * n];
            interleave(&even, &odd, &mut dst);

            let mut dst_ref = vec![0u8; 2 * n];
            interleave_scalar(&even, &odd, &mut dst_ref);
            assert_eq!(dst, dst_ref, "interleave, n={n}");

            // Full round-trip must reconstruct the original interleaved bytes.
            assert_eq!(dst, src, "interleave(deinterleave(src)) != src, n={n}");
        }
    }
}
