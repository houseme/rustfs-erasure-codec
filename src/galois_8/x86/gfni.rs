#[cfg(test)]
extern crate alloc;

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
const GFNI_ISOMORPHISM_ROWS: [u8; 8] = [0xff, 0xaa, 0xcc, 0x88, 0xf0, 0xa0, 0xc0, 0x80];

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
fn gfni_isomorphism_bytes() -> [u8; 16] {
    let word = [
        GFNI_ISOMORPHISM_ROWS[7],
        GFNI_ISOMORPHISM_ROWS[6],
        GFNI_ISOMORPHISM_ROWS[5],
        GFNI_ISOMORPHISM_ROWS[4],
        GFNI_ISOMORPHISM_ROWS[3],
        GFNI_ISOMORPHISM_ROWS[2],
        GFNI_ISOMORPHISM_ROWS[1],
        GFNI_ISOMORPHISM_ROWS[0],
    ];
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&word);
    bytes[8..].copy_from_slice(&word);
    bytes
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
fn coeff_table_avx2(c: u8) -> [u8; 32] {
    [c; 32]
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
fn coeff_table_avx512(c: u8) -> [u8; 64] {
    [c; 64]
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
fn gfni_avx2_constant_bytes(c: u8) -> ([u8; 16], [u8; 32]) {
    (gfni_isomorphism_bytes(), coeff_table_avx2(c))
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
fn gfni_avx512_constant_bytes(c: u8) -> ([u8; 16], [u8; 64]) {
    (gfni_isomorphism_bytes(), coeff_table_avx512(c))
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
#[target_feature(enable = "gfni,avx2")]
unsafe fn gfni_avx2_constants(c: u8) -> (core::arch::x86_64::__m256i, core::arch::x86_64::__m256i) {
    use core::arch::x86_64::{
        __m128i, __m256i, _mm_loadu_si128, _mm256_broadcastsi128_si256,
        _mm256_gf2p8affine_epi64_epi8, _mm256_loadu_si256,
    };

    let (iso_bytes, coeff_bytes) = gfni_avx2_constant_bytes(c);
    // SAFETY: `iso_bytes` is 16 bytes, `coeff_bytes` is 32 bytes — both stack-allocated.
    let iso128: __m128i = unsafe { _mm_loadu_si128(iso_bytes.as_ptr().cast()) };
    let iso256: __m256i = _mm256_broadcastsi128_si256(iso128);
    // SAFETY: `coeff_bytes` is a 32-byte stack array; read via an unaligned load. GFNI+AVX2 available here.
    let coeff_vec: __m256i = unsafe { _mm256_loadu_si256(coeff_bytes.as_ptr().cast()) };
    let coeff_mapped = _mm256_gf2p8affine_epi64_epi8(coeff_vec, iso256, 0);

    (iso256, coeff_mapped)
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
#[target_feature(enable = "gfni,avx512f,avx512bw")]
unsafe fn gfni_avx512_constants(
    c: u8,
) -> (core::arch::x86_64::__m512i, core::arch::x86_64::__m512i) {
    use core::arch::x86_64::{
        __m128i, __m512i, _mm_loadu_si128, _mm512_broadcast_i32x4, _mm512_gf2p8affine_epi64_epi8,
        _mm512_loadu_si512,
    };

    let (iso_bytes, coeff_bytes) = gfni_avx512_constant_bytes(c);
    // SAFETY: `iso_bytes` is 16 bytes, `coeff_bytes` is 64 bytes — both stack-allocated.
    let iso128: __m128i = unsafe { _mm_loadu_si128(iso_bytes.as_ptr().cast()) };
    let iso512: __m512i = _mm512_broadcast_i32x4(iso128);
    // SAFETY: `coeff_bytes` is a 64-byte stack array; read via an unaligned load. GFNI+AVX512 available here.
    let coeff_vec: __m512i = unsafe { _mm512_loadu_si512(coeff_bytes.as_ptr().cast()) };
    let coeff_mapped = _mm512_gf2p8affine_epi64_epi8::<0>(coeff_vec, iso512);

    (iso512, coeff_mapped)
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_gfni_avx2_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if input.is_empty() {
        return;
    }
    if c == 0 {
        out.fill(0);
        return;
    }
    if c == 1 {
        out.copy_from_slice(input);
        return;
    }
    // SAFETY: reached only after runtime `is_x86_feature_detected!` confirmed GFNI and AVX2 in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "gfni,avx2")]` requirement.
    unsafe { rust_gfni_avx2_mul_impl::<false>(c, input, out) }
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_gfni_avx2_mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if input.is_empty() {
        return;
    }
    if c == 0 {
        return;
    }
    if c == 1 {
        for (i, o) in input.iter().zip(out.iter_mut()) {
            *o ^= *i;
        }
        return;
    }
    // SAFETY: reached only after runtime `is_x86_feature_detected!` confirmed GFNI and AVX2 in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "gfni,avx2")]` requirement.
    unsafe { rust_gfni_avx2_mul_impl::<true>(c, input, out) }
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_gfni_avx512_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if input.is_empty() {
        return;
    }
    if c == 0 {
        out.fill(0);
        return;
    }
    if c == 1 {
        out.copy_from_slice(input);
        return;
    }
    // SAFETY: reached only after runtime `is_x86_feature_detected!` confirmed GFNI, AVX512F and AVX512BW
    // in the dispatcher, satisfying the callee's `#[target_feature(enable = "gfni,avx512f,avx512bw")]`.
    unsafe { rust_gfni_avx512_mul_impl::<false>(c, input, out) }
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_gfni_avx512_mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if input.is_empty() {
        return;
    }
    if c == 0 {
        return;
    }
    if c == 1 {
        for (i, o) in input.iter().zip(out.iter_mut()) {
            *o ^= *i;
        }
        return;
    }
    // SAFETY: reached only after runtime `is_x86_feature_detected!` confirmed GFNI, AVX512F and AVX512BW
    // in the dispatcher, satisfying the callee's `#[target_feature(enable = "gfni,avx512f,avx512bw")]`.
    unsafe { rust_gfni_avx512_mul_impl::<true>(c, input, out) }
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "gfni,avx2")]
unsafe fn rust_gfni_avx2_mul_impl<const XOR: bool>(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::x86_64::{
        __m256i, _mm256_gf2p8affine_epi64_epi8, _mm256_gf2p8mul_epi8, _mm256_loadu_si256,
        _mm256_storeu_si256, _mm256_xor_si256,
    };

    // SAFETY: GFNI+AVX2 are available in this `#[target_feature]` fn, satisfying the callee's requirement.
    let (iso256, coeff_mapped): (__m256i, __m256i) = unsafe { gfni_avx2_constants(c) };

    // Round down to 32-byte boundary so all SIMD loads/stores are in-bounds.
    let bytes_done = input.len() & !31usize;
    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);

    let (simd_input_chunks, []) = simd_input.as_chunks::<32>() else {
        unreachable!("GFNI AVX2 input is split on a 32-byte boundary");
    };
    let (simd_out_chunks, []) = simd_out.as_chunks_mut::<32>() else {
        unreachable!("GFNI AVX2 output is split on a 32-byte boundary");
    };
    for (input_chunk, out_chunk) in simd_input_chunks.iter().zip(simd_out_chunks.iter_mut()) {
        // SAFETY: `as_chunks::<32>()` guarantees 32 valid bytes for load/store.
        let input_vec = unsafe { _mm256_loadu_si256(input_chunk.as_ptr().cast()) };
        let mapped_input = _mm256_gf2p8affine_epi64_epi8(input_vec, iso256, 0);
        let product = _mm256_gf2p8mul_epi8(mapped_input, coeff_mapped);
        let restored = _mm256_gf2p8affine_epi64_epi8(product, iso256, 0);
        if XOR {
            // SAFETY: `as_chunks_mut::<32>()` guarantees 32 valid bytes for load/store.
            let out_vec = unsafe { _mm256_loadu_si256(out_chunk.as_ptr().cast()) };
            // SAFETY: `as_chunks_mut::<32>()` guarantees 32 valid bytes for this unaligned store.
            unsafe {
                _mm256_storeu_si256(
                    out_chunk.as_mut_ptr().cast(),
                    _mm256_xor_si256(out_vec, restored),
                )
            };
        } else {
            // SAFETY: `as_chunks_mut::<32>()` guarantees 32 valid bytes for the store.
            unsafe { _mm256_storeu_si256(out_chunk.as_mut_ptr().cast(), restored) };
        }
    }

    if XOR {
        super::super::scalar::mul_slice_xor_pure_rust(c, tail_input, tail_out);
    } else {
        super::super::scalar::mul_slice_pure_rust(c, tail_input, tail_out);
    }
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "gfni,avx512f,avx512bw")]
unsafe fn rust_gfni_avx512_mul_impl<const XOR: bool>(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::x86_64::{
        __m512i, _mm512_gf2p8affine_epi64_epi8, _mm512_gf2p8mul_epi8, _mm512_loadu_si512,
        _mm512_storeu_si512, _mm512_xor_si512,
    };

    // SAFETY: GFNI+AVX512 are available in this `#[target_feature]` fn, satisfying the callee's requirement.
    let (iso512, coeff_mapped): (__m512i, __m512i) = unsafe { gfni_avx512_constants(c) };

    // Round down to 64-byte boundary so all SIMD loads/stores are in-bounds.
    let bytes_done = input.len() & !63usize;
    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);

    let (simd_input_chunks, []) = simd_input.as_chunks::<64>() else {
        unreachable!("GFNI AVX-512 input is split on a 64-byte boundary");
    };
    let (simd_out_chunks, []) = simd_out.as_chunks_mut::<64>() else {
        unreachable!("GFNI AVX-512 output is split on a 64-byte boundary");
    };
    for (input_chunk, out_chunk) in simd_input_chunks.iter().zip(simd_out_chunks.iter_mut()) {
        // SAFETY: `as_chunks::<64>()` guarantees 64 valid bytes for load/store.
        let input_vec = unsafe { _mm512_loadu_si512(input_chunk.as_ptr().cast()) };
        let mapped_input = _mm512_gf2p8affine_epi64_epi8::<0>(input_vec, iso512);
        let product = _mm512_gf2p8mul_epi8(mapped_input, coeff_mapped);
        let restored = _mm512_gf2p8affine_epi64_epi8::<0>(product, iso512);
        if XOR {
            // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for load/store.
            let out_vec = unsafe { _mm512_loadu_si512(out_chunk.as_ptr().cast()) };
            // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for this unaligned store.
            unsafe {
                _mm512_storeu_si512(
                    out_chunk.as_mut_ptr().cast(),
                    _mm512_xor_si512(out_vec, restored),
                )
            };
        } else {
            // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for the store.
            unsafe { _mm512_storeu_si512(out_chunk.as_mut_ptr().cast(), restored) };
        }
    }

    if XOR {
        super::super::scalar::mul_slice_xor_pure_rust(c, tail_input, tail_out);
    } else {
        super::super::scalar::mul_slice_pure_rust(c, tail_input, tail_out);
    }
}

#[cfg(all(
    test,
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::galois_8::{mul_slice_scalar_for_test, mul_slice_xor_scalar_for_test, x86};
    use crate::tests::fill_random;
    use rand;

    const LENGTHS: [usize; 8] = [0usize, 1, 31, 32, 33, 255, 1024, 10_003];

    #[test]
    fn gfni_avx2_matches_scalar_mul_slice() {
        if !(std::is_x86_feature_detected!("gfni") && std::is_x86_feature_detected!("avx2")) {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut gfni = vec![0; len];

                mul_slice_scalar_for_test(c, &input, &mut scalar);
                rust_gfni_avx2_mul_slice(c, &input, &mut gfni);

                assert_eq!(scalar, gfni);
            }
        }
    }

    #[test]
    fn gfni_avx2_matches_scalar_mul_slice_xor() {
        if !(std::is_x86_feature_detected!("gfni") && std::is_x86_feature_detected!("avx2")) {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut gfni = vec![0; len];
                fill_random(&mut scalar);
                gfni.copy_from_slice(&scalar);

                mul_slice_xor_scalar_for_test(c, &input, &mut scalar);
                rust_gfni_avx2_mul_slice_xor(c, &input, &mut gfni);

                assert_eq!(scalar, gfni);
            }
        }
    }

    #[test]
    fn gfni_avx2_matches_avx2_mul_slice() {
        if !(std::is_x86_feature_detected!("gfni") && std::is_x86_feature_detected!("avx2")) {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut avx2 = vec![0; len];
                let mut gfni = vec![0; len];

                x86::avx2::rust_avx2_mul_slice(c, &input, &mut avx2);
                rust_gfni_avx2_mul_slice(c, &input, &mut gfni);

                assert_eq!(avx2, gfni);
            }
        }
    }

    #[test]
    fn gfni_avx512_matches_scalar_mul_slice() {
        if !(std::is_x86_feature_detected!("gfni")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        for &len in &[0usize, 1, 63, 64, 65, 255, 1024, 10_003] {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut gfni = vec![0; len];

                mul_slice_scalar_for_test(c, &input, &mut scalar);
                rust_gfni_avx512_mul_slice(c, &input, &mut gfni);

                assert_eq!(scalar, gfni);
            }
        }
    }

    #[test]
    fn gfni_avx512_matches_avx512_mul_slice() {
        if !(std::is_x86_feature_detected!("gfni")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        for &len in &[0usize, 1, 63, 64, 65, 255, 1024, 10_003] {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut avx512 = vec![0; len];
                let mut gfni = vec![0; len];

                x86::avx512::rust_avx512_mul_slice(c, &input, &mut avx512);
                rust_gfni_avx512_mul_slice(c, &input, &mut gfni);

                assert_eq!(avx512, gfni);
            }
        }
    }
}
