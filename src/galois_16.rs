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

    fn stable_format() -> Option<(&'static str, usize)> {
        Some(("rustfs.gf16.tower-11d-x2-02x-80.ax-ac.v1", 2))
    }

    fn write_format_element(element: [u8; 2], output: &mut [u8]) -> Option<usize> {
        if output.len() != 2 {
            return None;
        }
        output.copy_from_slice(&element);
        Some(2)
    }

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

    #[inline]
    fn mul_slice(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]]) {
        gf16_mul_slice::<false>(elem, input, out);
    }

    #[inline]
    fn mul_slice_add(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]]) {
        gf16_mul_slice::<true>(elem, input, out);
    }
}

#[cfg(rse_x86_gfni)]
type TowerMulFn = fn([u8; 2], &[[u8; 2]], &mut [[u8; 2]], bool);

#[cfg(rse_x86_gfni)]
static GFNI_TOWER_MULTIPLY: spin::Once<Option<TowerMulFn>> = spin::Once::new();

#[cfg(rse_x86_gfni)]
fn select_native_tower_multiply(backend: galois_8::BackendId) -> Option<TowerMulFn> {
    match backend {
        galois_8::BackendId::RustGfniAvx2 => Some(multiply_tower_gfni_avx2),
        galois_8::BackendId::RustGfniAvx512 => Some(multiply_tower_gfni_avx512),
        _ => None,
    }
}

#[cfg(rse_x86_gfni)]
fn multiply_tower_gfni_avx2(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]], xor: bool) {
    galois_8::x86::gfni::rust_gfni_avx2_mul_tower(tower_mul_coefficients(elem), input, out, xor);
}

#[cfg(rse_x86_gfni)]
fn multiply_tower_gfni_avx512(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]], xor: bool) {
    galois_8::x86::gfni::rust_gfni_avx512_mul_tower(tower_mul_coefficients(elem), input, out, xor);
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
/// (SIMD avx2/ssse3/neon, scalar fallback) to stay `no_std` and allocation free; the
/// whole path is byte-wise GF(2^8), so it is endian-agnostic.
///
/// On x86 with GFNI the GF(2^8) multiplies are so fast that a *scalar* byte
/// de-/re-interleave dominates the runtime (Amdahl); SIMD-ing the layout
/// conversion is what restores the tower decomposition's speed-up there.
#[inline]
fn gf16_mul_slice<const ACCUMULATE: bool>(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]]) {
    assert_eq!(input.len(), out.len());

    // A constant tower element scales both byte coordinates independently.
    // Keep their interleaved representation and reuse one GF8 slice operation.
    if elem[0] == 0 {
        if ACCUMULATE {
            galois_8::mul_slice_xor(elem[1], input.as_flattened(), out.as_flattened_mut());
        } else {
            galois_8::mul_slice(elem[1], input.as_flattened(), out.as_flattened_mut());
        }
        return;
    }

    // An empty general operation must not select a GF8 backend.
    if input.is_empty() {
        return;
    }
    #[cfg(rse_x86_gfni)]
    {
        // Resolve only optional native callables from the immutable GF8 choice.
        // Other backends keep the statically specialized portable executor.
        let native = GFNI_TOWER_MULTIPLY
            .call_once(|| select_native_tower_multiply(galois_8::active_backend_id()));
        if let Some(multiply) = native {
            multiply(elem, input, out, ACCUMULATE);
            return;
        }
    }
    gf16_mul_general::<ACCUMULATE>(elem, input, out);
}

const GF16_CHUNK: usize = 1024;

// Every plane starts on a 64-byte boundary, including when the stack's page
// offset varies between workers. The element format and caller slices stay
// unchanged; only this private scratch has a stronger alignment.
#[repr(C, align(64))]
struct BytePlanes {
    ax: [u8; GF16_CHUNK],
    ac: [u8; GF16_CHUNK],
    ox: [u8; GF16_CHUNK],
    oc: [u8; GF16_CHUNK],
}

const _: () = assert!(core::mem::size_of::<BytePlanes>() == 4 * GF16_CHUNK);

// Keep the 4 KiB plane frame out of the common subfield entry. Inlining this
// body back into its dispatcher would reintroduce unconditional stack probing.
#[inline(never)]
fn gf16_mul_general<const ACCUMULATE: bool>(elem: [u8; 2], input: &[[u8; 2]], out: &mut [[u8; 2]]) {
    debug_assert_ne!(elem[0], 0);
    debug_assert_eq!(input.len(), out.len());
    let cx = elem[0];
    let cc = elem[1];
    let coef_a = cc ^ galois_8::mul(EXT_POLY[1], cx);
    let coef_b = cx;
    let coef_c = galois_8::mul(EXT_POLY[2], cx);
    let coef_d = cc;

    let mut planes = BytePlanes {
        ax: [0; GF16_CHUNK],
        ac: [0; GF16_CHUNK],
        ox: [0; GF16_CHUNK],
        oc: [0; GF16_CHUNK],
    };
    let BytePlanes { ax, ac, ox, oc } = &mut planes;

    let mut offset = 0;
    while offset < input.len() {
        let n = core::cmp::min(GF16_CHUNK, input.len() - offset);

        // Split the interleaved `[[u8; 2]]` input into the two GF(2^8) byte
        // planes `ax`/`ac`. `as_flattened` is the safe `&[[u8; 2]] -> &[u8]`
        // byte view; the split is a pure byte permutation (endian-agnostic).
        deinterleave(
            input[offset..offset + n].as_flattened(),
            &mut ax[..n],
            &mut ac[..n],
        );

        // Compute the product once in the same planes for both modes. The
        // accumulator is read only while interleaving back into the caller's
        // destination; it needs no separate deinterleave or plane copy.
        galois_8::mul_slice(coef_a, &ax[..n], &mut ox[..n]);
        galois_8::mul_slice_xor(coef_b, &ac[..n], &mut ox[..n]);
        galois_8::mul_slice(coef_c, &ax[..n], &mut oc[..n]);
        galois_8::mul_slice_xor(coef_d, &ac[..n], &mut oc[..n]);

        interleave::<ACCUMULATE>(
            &ox[..n],
            &oc[..n],
            out[offset..offset + n].as_flattened_mut(),
        );
        offset += n;
    }
}

// Keep the fused path tied to the same extension polynomial as the original executor.
#[cfg(any(rse_x86_gfni, test))]
pub(crate) fn tower_mul_coefficients([cx, cc]: [u8; 2]) -> [u8; 4] {
    [
        cc ^ galois_8::mul(EXT_POLY[1], cx),
        cx,
        galois_8::mul(EXT_POLY[2], cx),
        cc,
    ]
}

// Keep the layout width consistent with the immutable arithmetic selection.
// A wide CPU alone does not make widening a narrow arithmetic path profitable.
#[cfg(any(
    test,
    all(feature = "std", feature = "simd-avx2", target_arch = "x86_64")
))]
fn avx2_layout_allowed(backend: galois_8::BackendId, cpu_has_avx2: bool) -> bool {
    cpu_has_avx2
        && matches!(
            backend,
            galois_8::BackendId::RustAvx2
                | galois_8::BackendId::RustAvx512
                | galois_8::BackendId::RustGfniAvx2
                | galois_8::BackendId::RustGfniAvx512
        )
}

#[cfg(all(feature = "std", feature = "simd-avx2", target_arch = "x86_64"))]
#[inline]
fn avx2_layout_enabled() -> bool {
    static ENABLED: spin::Once<bool> = spin::Once::new();
    *ENABLED.call_once(|| {
        avx2_layout_allowed(
            galois_8::active_backend_id(),
            is_x86_feature_detected!("avx2"),
        )
    })
}

/// De-interleave interleaved GF(2^16) element bytes into two contiguous GF(2^8)
/// byte planes: `even[i] = src[2*i]`, `odd[i] = src[2*i + 1]`.
///
/// Requires `src.len() == 2 * even.len()` and `even.len() == odd.len()`. This is
/// a pure byte permutation, so it is endian-agnostic. SIMD-accelerated on
/// avx2/ssse3 (x86_64) and neon (aarch64), with a scalar fallback that also handles
/// the sub-32-element tail of the SIMD paths.
#[allow(clippy::needless_return)]
#[inline]
fn deinterleave(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    debug_assert_eq!(src.len(), even.len() * 2);
    debug_assert_eq!(even.len(), odd.len());

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        #[cfg(feature = "simd-avx2")]
        if even.len() >= 32 && avx2_layout_enabled() {
            // SAFETY: AVX2 is detected and the checked slice lengths match the
            // 64-byte source / 32-byte plane spans used by this kernel.
            unsafe { deinterleave_avx2(src, even, odd) };
            return;
        }
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
/// `dst[2*i] = even[i]`, `dst[2*i + 1] = odd[i]`, or XOR those bytes into
/// initialized output when `ACCUMULATE` is true. Overwrite inverts [`deinterleave`].
///
/// Requires `dst.len() == 2 * even.len()` and `even.len() == odd.len()`.
#[allow(clippy::needless_return)]
#[inline]
fn interleave<const ACCUMULATE: bool>(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(dst.len(), even.len() * 2);
    debug_assert_eq!(even.len(), odd.len());

    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    {
        #[cfg(feature = "simd-avx2")]
        if even.len() >= 32 && avx2_layout_enabled() {
            // SAFETY: AVX2 is detected; both 32-byte planes and the initialized
            // 64-byte destination satisfy the kernel's slice contract.
            unsafe { interleave_avx2::<ACCUMULATE>(even, odd, dst) };
            return;
        }
        if is_x86_feature_detected!("ssse3") {
            // SAFETY: ssse3 confirmed at runtime, matching the callee; the
            // length contract above matches what the kernel indexes.
            unsafe {
                interleave_ssse3::<ACCUMULATE>(even, odd, dst);
                return;
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is baseline on aarch64; the length contract above matches
        // what the kernel indexes.
        unsafe {
            interleave_neon::<ACCUMULATE>(even, odd, dst);
            return;
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    interleave_scalar::<ACCUMULATE>(even, odd, dst);
}

fn deinterleave_scalar(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    for (i, (e, o)) in even.iter_mut().zip(odd.iter_mut()).enumerate() {
        *e = src[2 * i];
        *o = src[2 * i + 1];
    }
}

fn interleave_scalar<const ACCUMULATE: bool>(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    for (i, (e, o)) in even.iter().zip(odd.iter()).enumerate() {
        if ACCUMULATE {
            dst[2 * i] ^= *e;
            dst[2 * i + 1] ^= *o;
        } else {
            dst[2 * i] = *e;
            dst[2 * i + 1] = *o;
        }
    }
}

#[cfg(all(feature = "std", feature = "simd-avx2", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn deinterleave_avx2(src: &[u8], even: &mut [u8], odd: &mut [u8]) {
    use core::arch::x86_64::{
        _mm256_and_si256, _mm256_loadu_si256, _mm256_packus_epi16, _mm256_permute4x64_epi64,
        _mm256_set1_epi16, _mm256_srli_epi16, _mm256_storeu_si256,
    };

    let mask = _mm256_set1_epi16(0xff);
    let batches = even.len() / 32;
    for b in 0..batches {
        let s = &src[b * 64..b * 64 + 64];
        let e = &mut even[b * 32..b * 32 + 32];
        let o = &mut odd[b * 32..b * 32 + 32];
        // SAFETY: the two unaligned loads cover exactly s's 64 bytes, and each
        // store covers one 32-byte plane. Masking / logical shifts yield words
        // in 0..=255, so unsigned saturation preserves every extracted byte.
        // Packing works within 128-bit lanes; qword order 0,2,1,3 restores the
        // original element order. The caller has checked AVX2 support.
        unsafe {
            let lo = _mm256_loadu_si256(s.as_ptr().cast());
            let hi = _mm256_loadu_si256(s[32..].as_ptr().cast());
            let e_bytes =
                _mm256_packus_epi16(_mm256_and_si256(lo, mask), _mm256_and_si256(hi, mask));
            let o_bytes =
                _mm256_packus_epi16(_mm256_srli_epi16::<8>(lo), _mm256_srli_epi16::<8>(hi));
            _mm256_storeu_si256(
                e.as_mut_ptr().cast(),
                _mm256_permute4x64_epi64::<0xd8>(e_bytes),
            );
            _mm256_storeu_si256(
                o.as_mut_ptr().cast(),
                _mm256_permute4x64_epi64::<0xd8>(o_bytes),
            );
        }
    }
    let done = batches * 32;
    deinterleave_scalar(&src[done * 2..], &mut even[done..], &mut odd[done..]);
}

#[cfg(all(feature = "std", feature = "simd-avx2", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn interleave_avx2<const ACCUMULATE: bool>(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    use core::arch::x86_64::{
        _mm256_loadu_si256, _mm256_permute2x128_si256, _mm256_storeu_si256, _mm256_unpackhi_epi8,
        _mm256_unpacklo_epi8, _mm256_xor_si256,
    };

    let batches = even.len() / 32;
    for b in 0..batches {
        let e = &even[b * 32..b * 32 + 32];
        let o = &odd[b * 32..b * 32 + 32];
        let d = &mut dst[b * 64..b * 64 + 64];
        // SAFETY: each plane load covers exactly 32 bytes. The two destination
        // stores (and accumulation loads) cover exactly the initialized 64-byte
        // span at offsets 0 and 32. Lane joins produce elements 0..15 / 16..31;
        // the caller has checked AVX2 support.
        unsafe {
            let e_bytes = _mm256_loadu_si256(e.as_ptr().cast());
            let o_bytes = _mm256_loadu_si256(o.as_ptr().cast());
            let lo = _mm256_unpacklo_epi8(e_bytes, o_bytes);
            let hi = _mm256_unpackhi_epi8(e_bytes, o_bytes);
            let mut first = _mm256_permute2x128_si256::<0x20>(lo, hi);
            let mut second = _mm256_permute2x128_si256::<0x31>(lo, hi);
            if ACCUMULATE {
                first = _mm256_xor_si256(first, _mm256_loadu_si256(d.as_ptr().cast()));
                second = _mm256_xor_si256(second, _mm256_loadu_si256(d[32..].as_ptr().cast()));
            }
            _mm256_storeu_si256(d.as_mut_ptr().cast(), first);
            _mm256_storeu_si256(d[32..].as_mut_ptr().cast(), second);
        }
    }
    let done = batches * 32;
    interleave_scalar::<ACCUMULATE>(&even[done..], &odd[done..], &mut dst[done * 2..]);
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
unsafe fn interleave_ssse3<const ACCUMULATE: bool>(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    use core::arch::x86_64::{
        _mm_loadu_si128, _mm_storeu_si128, _mm_unpackhi_epi8, _mm_unpacklo_epi8, _mm_xor_si128,
    };

    let batches = even.len() / 32;
    for b in 0..batches {
        let e = &even[b * 32..b * 32 + 32];
        let o = &odd[b * 32..b * 32 + 32];
        let d = &mut dst[b * 64..b * 64 + 64];
        // SAFETY: the slices above are exactly 32/32/64 bytes, so the four
        // 128-bit loads at 0/16 of `e`/`o` and the four 128-bit stores at
        // 0/16/32/48 of initialized `d` are in-bounds. Accumulation reads
        // exactly those same destination ranges; ssse3 confirmed by caller.
        unsafe {
            let lo = _mm_loadu_si128(e.as_ptr().cast());
            let hi = _mm_loadu_si128(o.as_ptr().cast());
            let mut low = _mm_unpacklo_epi8(lo, hi);
            let mut high = _mm_unpackhi_epi8(lo, hi);
            if ACCUMULATE {
                low = _mm_xor_si128(low, _mm_loadu_si128(d.as_ptr().cast()));
                high = _mm_xor_si128(high, _mm_loadu_si128(d[16..].as_ptr().cast()));
            }
            _mm_storeu_si128(d.as_mut_ptr().cast(), low);
            _mm_storeu_si128(d[16..].as_mut_ptr().cast(), high);
            let lo2 = _mm_loadu_si128(e[16..].as_ptr().cast());
            let hi2 = _mm_loadu_si128(o[16..].as_ptr().cast());
            let mut low = _mm_unpacklo_epi8(lo2, hi2);
            let mut high = _mm_unpackhi_epi8(lo2, hi2);
            if ACCUMULATE {
                low = _mm_xor_si128(low, _mm_loadu_si128(d[32..].as_ptr().cast()));
                high = _mm_xor_si128(high, _mm_loadu_si128(d[48..].as_ptr().cast()));
            }
            _mm_storeu_si128(d[32..].as_mut_ptr().cast(), low);
            _mm_storeu_si128(d[48..].as_mut_ptr().cast(), high);
        }
    }
    let done = batches * 32;
    interleave_scalar::<ACCUMULATE>(&even[done..], &odd[done..], &mut dst[done * 2..]);
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
unsafe fn interleave_neon<const ACCUMULATE: bool>(even: &[u8], odd: &[u8], dst: &mut [u8]) {
    use core::arch::aarch64::{veorq_u8, vld1q_u8, vst1q_u8, vzip1q_u8, vzip2q_u8};

    let batches = even.len() / 32;
    for b in 0..batches {
        let e = &even[b * 32..b * 32 + 32];
        let o = &odd[b * 32..b * 32 + 32];
        let d = &mut dst[b * 64..b * 64 + 64];
        // vzip1q/vzip2q interleave the low/high 16 bytes of the two planes.
        // SAFETY: the slices above are exactly 32/32/64 bytes, so the 128-bit
        // loads at 0/16 of `e`/`o` and stores at 0/16/32/48 of initialized `d`
        // are in-bounds. Accumulation reads those same destination ranges;
        // NEON is baseline on aarch64.
        unsafe {
            let lo = vld1q_u8(e.as_ptr());
            let hi = vld1q_u8(o.as_ptr());
            let mut low = vzip1q_u8(lo, hi);
            let mut high = vzip2q_u8(lo, hi);
            if ACCUMULATE {
                low = veorq_u8(low, vld1q_u8(d.as_ptr()));
                high = veorq_u8(high, vld1q_u8(d[16..].as_ptr()));
            }
            vst1q_u8(d.as_mut_ptr(), low);
            vst1q_u8(d[16..].as_mut_ptr(), high);
            let lo2 = vld1q_u8(e[16..].as_ptr());
            let hi2 = vld1q_u8(o[16..].as_ptr());
            let mut low = vzip1q_u8(lo2, hi2);
            let mut high = vzip2q_u8(lo2, hi2);
            if ACCUMULATE {
                low = veorq_u8(low, vld1q_u8(d[32..].as_ptr()));
                high = veorq_u8(high, vld1q_u8(d[48..].as_ptr()));
            }
            vst1q_u8(d[32..].as_mut_ptr(), low);
            vst1q_u8(d[48..].as_mut_ptr(), high);
        }
    }
    let done = batches * 32;
    interleave_scalar::<ACCUMULATE>(&even[done..], &odd[done..], &mut dst[done * 2..]);
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

    fn exp(self, n: usize) -> Element {
        if n == 0 {
            Element::constant(1)
        } else if self == Element::zero() {
            Element::zero()
        } else {
            // Nonzero elements form a group of order 65535. Reduce only after
            // the zero check, so zero to a positive multiple of the period is zero.
            let n = n % (<Field as crate::Field>::ORDER - 1);
            if n == 0 {
                return Element::constant(1);
            }
            // Start with the implicit highest set bit: at most 15 lower bits,
            // one square and optionally one multiply per bit, with no multiply by one.
            let mut power = self;
            let mut bit = (1usize << n.ilog2()) >> 1;
            while bit != 0 {
                power = power * power;
                if n & bit != 0 {
                    power = power * self;
                }
                bit >>= 1;
            }
            power
        }
    }

    /// Invert through the GF8 norm of this quadratic extension.
    /// Preserve the historical zero-inverse (and division-by-zero) behavior.
    fn inverse(self) -> Element {
        let [ax, ac] = self.0;
        if ax == 0 {
            return if ac == 0 {
                Element::zero()
            } else {
                Element::constant(galois_8::div(1, ac))
            };
        }

        // The other root of X² + 2X + 128 is X + 2. Thus the conjugate
        // of ax*X + ac is ax*X + (ac + 2*ax), and their product is the
        // nonzero GF8 constant ac*(ac + 2*ax) + 128*ax².
        let conjugate_c = ac ^ galois_8::mul(EXT_POLY[1], ax);
        let norm =
            galois_8::mul(ac, conjugate_c) ^ galois_8::mul(EXT_POLY[2], galois_8::mul(ax, ax));
        let norm_inverse = galois_8::div(1, norm);
        Element([
            galois_8::mul(ax, norm_inverse),
            galois_8::mul(conjugate_c, norm_inverse),
        ])
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

    #[cfg(test)]
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

// Retain the former Euclidean implementation as an independent test oracle.

#[cfg(test)]
#[derive(Debug)]
enum EgcdRhs {
    Element(Element),
    ExtPoly,
}

#[cfg(test)]
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
    fn inverse_euclid(self) -> Element {
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
    fn norm_inverse_matches_euclid_for_entire_field() {
        for bits in 0..=u16::MAX {
            let element = Element(bits.to_be_bytes());
            let inverse = element.inverse();
            assert_eq!(inverse, element.inverse_euclid(), "element {bits:#06x}");
            let expected = if bits == 0 {
                Element::zero()
            } else {
                Element::constant(1)
            };
            assert_eq!(element * inverse, expected, "element {bits:#06x}");
        }
    }

    #[test]
    fn norm_division_preserves_all_denominators() {
        for bits in 0..=u16::MAX {
            let denominator = Element(bits.to_be_bytes());
            for numerator in [[0, 0], [0, 1], [1, 0], [0x9a, 0x3f], [0xff, 0xff]] {
                let numerator = Element(numerator);
                assert_eq!(
                    numerator / denominator,
                    numerator * denominator.inverse_euclid(),
                    "numerator {numerator:?}, denominator {bits:#06x}"
                );
            }
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

    #[cfg(rse_x86_gfni)]
    #[test]
    fn tower_dispatch_preserves_scalar_and_other_backend_choices() {
        use galois_8::BackendId;
        for backend in [
            BackendId::ScalarRust,
            BackendId::RustNeon,
            BackendId::RustSsse3,
            BackendId::RustAvx2,
            BackendId::RustAvx512,
            BackendId::RustVsx,
        ] {
            assert!(select_native_tower_multiply(backend).is_none());
        }
        assert!(core::ptr::fn_addr_eq(
            select_native_tower_multiply(BackendId::RustGfniAvx2).unwrap(),
            multiply_tower_gfni_avx2 as TowerMulFn
        ));
        assert!(core::ptr::fn_addr_eq(
            select_native_tower_multiply(BackendId::RustGfniAvx512).unwrap(),
            multiply_tower_gfni_avx512 as TowerMulFn
        ));
    }

    #[test]
    fn avx2_layout_requires_wide_selected_arithmetic() {
        use galois_8::BackendId;
        for backend in [
            BackendId::ScalarRust,
            BackendId::RustSsse3,
            BackendId::RustNeon,
            BackendId::RustVsx,
        ] {
            assert!(!avx2_layout_allowed(backend, true), "{backend:?}");
            assert!(!avx2_layout_allowed(backend, false), "{backend:?}");
        }
        for backend in [
            BackendId::RustAvx2,
            BackendId::RustAvx512,
            BackendId::RustGfniAvx2,
            BackendId::RustGfniAvx512,
        ] {
            assert!(avx2_layout_allowed(backend, true), "{backend:?}");
            assert!(!avx2_layout_allowed(backend, false), "{backend:?}");
        }
    }

    #[cfg(all(feature = "std", feature = "simd-avx2", target_arch = "x86_64"))]
    #[test]
    fn cached_avx2_layout_matches_selected_arithmetic() {
        let backend = galois_8::active_backend_id();
        let selected = avx2_layout_enabled();
        assert_eq!(
            selected,
            avx2_layout_allowed(backend, is_x86_feature_detected!("avx2"))
        );
        use std::io::Write as _;
        // Expected-panic hooks can write concurrently without the stderr lock.
        // Format first so the short marker reaches the capture pipe in one write.
        let marker = format!(
            "GF16_LAYOUT_SELECTED backend={} avx2={selected}\n",
            galois_8::active_backend_name()
        );
        std::io::stderr()
            .lock()
            .write_all(marker.as_bytes())
            .unwrap();
    }

    #[test]
    fn tower_coefficients_match_scalar_for_every_coefficient_and_basis_bit() {
        for coefficient in 0..65536 {
            let scalar = Field::nth_internal(coefficient);
            let [a, b, c, d] = tower_mul_coefficients(scalar);
            for bit in 0..16 {
                let input = if bit < 8 {
                    [0, 1 << bit]
                } else {
                    [1 << (bit - 8), 0]
                };
                let product = [
                    galois_8::mul(a, input[0]) ^ galois_8::mul(b, input[1]),
                    galois_8::mul(c, input[0]) ^ galois_8::mul(d, input[1]),
                ];
                assert_eq!(product, Field::mul(scalar, input));
            }
        }
    }

    #[test]
    fn constant_tower_slices_match_scalar_for_every_subfield_coefficient() {
        let input: Vec<_> = (0..2104usize)
            .map(|i| [(i * 197 + 31) as u8, (i * 73 + 11) as u8])
            .collect();
        for coefficient in 0..=u8::MAX {
            let scalar = [0, coefficient];
            for start in [0, 1, 3] {
                for length in [
                    0, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 1023, 1024, 1025, 2100,
                ] {
                    let mut output = vec![[0xa5, 0x5a]; length + 2];
                    for accumulate in [false, true] {
                        output.fill([0xa5, 0x5a]);
                        if accumulate {
                            Field::mul_slice_add(
                                scalar,
                                &input[start..start + length],
                                &mut output[1..length + 1],
                            );
                        } else {
                            Field::mul_slice(
                                scalar,
                                &input[start..start + length],
                                &mut output[1..length + 1],
                            );
                        }
                        assert_eq!(output[0], [0xa5, 0x5a]);
                        assert_eq!(output[length + 1], [0xa5, 0x5a]);
                        for i in 0..length {
                            let product = Field::mul(scalar, input[start + i]);
                            let expected = if accumulate {
                                Field::add([0xa5, 0x5a], product)
                            } else {
                                product
                            };
                            assert_eq!(
                                output[i + 1],
                                expected,
                                "coefficient {coefficient}, start {start}, length {length}, index {i}, accumulate {accumulate}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic]
    fn constant_tower_multiply_rejects_mismatched_lengths() {
        Field::mul_slice([0, 0], &[], &mut [[0xa5, 0x5a]]);
    }

    #[test]
    #[should_panic]
    fn constant_tower_accumulate_rejects_mismatched_lengths() {
        Field::mul_slice_add([0, 0], &[[0xa5, 0x5a]], &mut []);
    }

    #[test]
    #[should_panic]
    fn empty_general_tower_multiply_rejects_mismatched_lengths() {
        Field::mul_slice([1, 0], &[], &mut [[0xa5, 0x5a]]);
    }

    #[test]
    #[should_panic]
    fn empty_general_tower_accumulate_rejects_mismatched_lengths() {
        Field::mul_slice_add([1, 0], &[[0xa5, 0x5a]], &mut []);
    }

    #[test]
    fn general_tower_slices_match_scalar_for_every_coefficient() {
        const N: usize = 33; // one SIMD batch plus its scalar tail
        let input: [[u8; 2]; N] = core::array::from_fn(|i| match i {
            0..8 => [1 << i, 0],
            8..16 => [0, 1 << (i - 8)],
            _ => [(i * 31 + 7) as u8, (i * 17 + 3) as u8],
        });
        for coefficient in 0..65536 {
            let coefficient = [(coefficient >> 8) as u8, coefficient as u8];
            for accumulate in [false, true] {
                let mut output = [[0xa5, 0x5a]; N + 2];
                let mut expected = output;
                for (i, &value) in input.iter().enumerate() {
                    let product = Field::mul(coefficient, value);
                    expected[i + 1] = if accumulate {
                        Field::add(expected[i + 1], product)
                    } else {
                        product
                    };
                }
                if accumulate {
                    Field::mul_slice_add(coefficient, &input, &mut output[1..N + 1]);
                } else {
                    Field::mul_slice(coefficient, &input, &mut output[1..N + 1]);
                }
                assert_eq!(
                    output, expected,
                    "coefficient {coefficient:?}, accumulate {accumulate}"
                );
            }
        }
    }

    #[cfg(all(feature = "std", feature = "simd-avx2", target_arch = "x86_64"))]
    #[test]
    fn avx2_layout_matches_scalar_at_offsets_and_tails() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        for n in (0..=97).chain([1023, 1024, 1025, 2049]) {
            let source: Vec<_> = (0..2 * n + 9).map(|i| (i * 197 + 31) as u8).collect();
            for (src_start, dst_start) in [(0, 1), (1, 3), (3, 1)] {
                let input = &source[src_start..src_start + 2 * n];
                let mut even = vec![0xa5; dst_start + n + 3];
                let mut odd = even.clone();
                let mut expected_even = even.clone();
                let mut expected_odd = odd.clone();
                deinterleave_scalar(
                    input,
                    &mut expected_even[dst_start..dst_start + n],
                    &mut expected_odd[dst_start..dst_start + n],
                );
                // SAFETY: AVX2 checked above; initialized plane spans each have
                // n bytes, and input has exactly 2*n bytes, with valid offsets.
                unsafe {
                    deinterleave_avx2(
                        input,
                        &mut even[dst_start..dst_start + n],
                        &mut odd[dst_start..dst_start + n],
                    )
                };
                assert_eq!(even, expected_even, "even length {n}, offset {src_start}");
                assert_eq!(odd, expected_odd, "odd length {n}, offset {src_start}");
                let e = &even[dst_start..dst_start + n];
                let o = &odd[dst_start..dst_start + n];
                for accumulate in [false, true] {
                    let mut output: Vec<_> = (0..src_start + 2 * n + 3)
                        .map(|i| (i * 41 + 13) as u8)
                        .collect();
                    let mut expected = output.clone();
                    if accumulate {
                        interleave_scalar::<true>(
                            e,
                            o,
                            &mut expected[src_start..src_start + 2 * n],
                        );
                        // SAFETY: AVX2 checked; the two n-byte planes and
                        // initialized 2*n-byte destination satisfy the contract.
                        unsafe {
                            interleave_avx2::<true>(e, o, &mut output[src_start..src_start + 2 * n])
                        };
                    } else {
                        interleave_scalar::<false>(
                            e,
                            o,
                            &mut expected[src_start..src_start + 2 * n],
                        );
                        // SAFETY: AVX2 checked; the same valid spans as above.
                        unsafe {
                            interleave_avx2::<false>(
                                e,
                                o,
                                &mut output[src_start..src_start + 2 * n],
                            )
                        };
                    }
                    assert_eq!(output, expected, "length {n}, accumulate {accumulate}");
                }
            }
        }
        eprintln!("AVX2 GF16 byte layout scalar oracle executed");
    }

    #[test]
    fn interleave_accumulation_preserves_offsets_tails_and_sentinels() {
        for n in (0..=97).chain([1023, 1024, 1025, 2049]) {
            let even: Vec<_> = (0..n + 3).map(|i| (i * 29 + 13) as u8).collect();
            let odd: Vec<_> = (0..n + 3).map(|i| (i * 37 + 41) as u8).collect();
            for (source_start, destination_start) in [(0, 1), (1, 3), (3, 1)] {
                let mut output: Vec<_> = (0..destination_start + 2 * n + 3)
                    .map(|i| (i * 17 + 7) as u8)
                    .collect();
                let mut expected = output.clone();
                for i in 0..n {
                    expected[destination_start + 2 * i] ^= even[source_start + i];
                    expected[destination_start + 2 * i + 1] ^= odd[source_start + i];
                }
                interleave::<true>(
                    &even[source_start..source_start + n],
                    &odd[source_start..source_start + n],
                    &mut output[destination_start..destination_start + 2 * n],
                );
                assert_eq!(
                    output, expected,
                    "length {n}, offsets {source_start}/{destination_start}"
                );
            }
        }
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
            interleave::<false>(&even, &odd, &mut dst);

            let mut dst_ref = vec![0u8; 2 * n];
            interleave_scalar::<false>(&even, &odd, &mut dst_ref);
            assert_eq!(dst, dst_ref, "interleave, n={n}");

            // Full round-trip must reconstruct the original interleaved bytes.
            assert_eq!(
                dst, src,
                "interleave::<false>(deinterleave(src)) != src, n={n}"
            );
        }
    }
}
