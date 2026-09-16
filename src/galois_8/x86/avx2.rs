#[cfg(test)]
extern crate alloc;

#[cfg(all(
    feature = "simd-avx2",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load_tables_avx2(
    low: &[u8; 16],
    high: &[u8; 16],
) -> (core::arch::x86_64::__m256i, core::arch::x86_64::__m256i) {
    use core::arch::x86_64::{__m128i, _mm_loadu_si128, _mm256_broadcastsi128_si256};

    // SAFETY: reads a 16-byte table half via an unaligned load; AVX2 is available in this `#[target_feature]` fn.
    let low128: __m128i = unsafe { _mm_loadu_si128(low.as_ptr().cast()) };
    // SAFETY: reads a 16-byte table half via an unaligned load; AVX2 is available in this `#[target_feature]` fn.
    let high128: __m128i = unsafe { _mm_loadu_si128(high.as_ptr().cast()) };

    (
        _mm256_broadcastsi128_si256(low128),
        _mm256_broadcastsi128_si256(high128),
    )
}

#[cfg(all(
    feature = "simd-avx2",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_avx2_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
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
    // SAFETY: reached only after a runtime `is_x86_feature_detected!("avx2")` check in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "avx2")]` requirement.
    unsafe { rust_avx2_mul_impl::<false>(c, input, out) }
}

#[cfg(all(
    feature = "simd-avx2",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_avx2_mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
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
    // SAFETY: reached only after a runtime `is_x86_feature_detected!("avx2")` check in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "avx2")]` requirement.
    unsafe { rust_avx2_mul_impl::<true>(c, input, out) }
}

#[cfg(all(
    feature = "simd-avx2",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "avx2")]
unsafe fn rust_avx2_mul_impl<const XOR: bool>(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::x86_64::{
        __m256i, _mm256_and_si256, _mm256_loadu_si256, _mm256_set1_epi8, _mm256_shuffle_epi8,
        _mm256_srli_epi64, _mm256_storeu_si256, _mm256_xor_si256,
    };

    let (low_half, high_half) = super::load_table_halves(c);
    // SAFETY: `low_half`/`high_half` are 16-byte table halves; broadcast to 256-bit.
    let (low_tbl, high_tbl): (__m256i, __m256i) = unsafe { load_tables_avx2(low_half, high_half) };
    let nibble_mask: __m256i = _mm256_set1_epi8(0x0f);

    // Round down to 32-byte boundary so all SIMD loads/stores are in-bounds.
    let bytes_done = input.len() & !31usize;
    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);

    let (simd_input_chunks, []) = simd_input.as_chunks::<32>() else {
        unreachable!("AVX2 input is split on a 32-byte boundary");
    };
    let (simd_out_chunks, []) = simd_out.as_chunks_mut::<32>() else {
        unreachable!("AVX2 output is split on a 32-byte boundary");
    };
    for (input_chunk, out_chunk) in simd_input_chunks.iter().zip(simd_out_chunks.iter_mut()) {
        // SAFETY: `as_chunks::<32>()` yields exactly 32 valid bytes for this unaligned load.
        let input_vec = unsafe { _mm256_loadu_si256(input_chunk.as_ptr().cast()) };
        let low = _mm256_and_si256(input_vec, nibble_mask);
        let high = _mm256_and_si256(_mm256_srli_epi64::<4>(input_vec), nibble_mask);
        let product = _mm256_xor_si256(
            _mm256_shuffle_epi8(low_tbl, low),
            _mm256_shuffle_epi8(high_tbl, high),
        );
        if XOR {
            // SAFETY: `as_chunks_mut::<32>()` yields exactly 32 valid bytes for this unaligned load of the current output.
            let out_vec = unsafe { _mm256_loadu_si256(out_chunk.as_ptr().cast()) };
            // SAFETY: `as_chunks_mut::<32>()` yields exactly 32 valid bytes for this unaligned store.
            unsafe {
                _mm256_storeu_si256(
                    out_chunk.as_mut_ptr().cast(),
                    _mm256_xor_si256(out_vec, product),
                )
            };
        } else {
            // SAFETY: `as_chunks_mut::<32>()` yields exactly 32 valid bytes for this unaligned store.
            unsafe { _mm256_storeu_si256(out_chunk.as_mut_ptr().cast(), product) };
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
    feature = "simd-avx2",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios")),
    feature = "std"
))]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::galois_8::{mul_slice_scalar_for_test, mul_slice_xor_scalar_for_test};
    use crate::tests::fill_random;
    use rand;

    const LENGTHS: [usize; 8] = [0usize, 1, 31, 32, 33, 255, 1024, 10_003];

    #[test]
    fn avx2_matches_scalar_mul_slice() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut avx2 = vec![0; len];

                mul_slice_scalar_for_test(c, &input, &mut scalar);
                rust_avx2_mul_slice(c, &input, &mut avx2);

                assert_eq!(scalar, avx2);
            }
        }
    }

    #[test]
    fn avx2_matches_scalar_mul_slice_xor() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut avx2 = vec![0; len];
                fill_random(&mut scalar);
                avx2.copy_from_slice(&scalar);

                mul_slice_xor_scalar_for_test(c, &input, &mut scalar);
                rust_avx2_mul_slice_xor(c, &input, &mut avx2);

                assert_eq!(scalar, avx2);
            }
        }
    }
}
