#[cfg(test)]
extern crate alloc;

#[cfg(all(
    feature = "simd-avx512",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[inline]
#[target_feature(enable = "avx512f,avx512bw")]
unsafe fn load_tables_avx512(
    low: &[u8; 16],
    high: &[u8; 16],
) -> (core::arch::x86_64::__m512i, core::arch::x86_64::__m512i) {
    use core::arch::x86_64::{__m128i, _mm_loadu_si128, _mm512_broadcast_i32x4};

    // SAFETY: reads a 16-byte table half via an unaligned load; AVX512 is available in this `#[target_feature]` fn.
    let low128: __m128i = unsafe { _mm_loadu_si128(low.as_ptr().cast()) };
    // SAFETY: reads a 16-byte table half via an unaligned load; AVX512 is available in this `#[target_feature]` fn.
    let high128: __m128i = unsafe { _mm_loadu_si128(high.as_ptr().cast()) };

    (
        _mm512_broadcast_i32x4(low128),
        _mm512_broadcast_i32x4(high128),
    )
}

#[cfg(all(
    feature = "simd-avx512",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_avx512_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
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
    // SAFETY: reached only after runtime `is_x86_feature_detected!` confirmed AVX512F and AVX512BW in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "avx512f,avx512bw")]` requirement.
    unsafe { rust_avx512_mul_impl::<false>(c, input, out) }
}

#[cfg(all(
    feature = "simd-avx512",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_avx512_mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
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
    // SAFETY: reached only after runtime `is_x86_feature_detected!` confirmed AVX512F and AVX512BW in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "avx512f,avx512bw")]` requirement.
    unsafe { rust_avx512_mul_impl::<true>(c, input, out) }
}

#[cfg(all(
    feature = "simd-avx512",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "avx512f,avx512bw")]
unsafe fn rust_avx512_mul_impl<const XOR: bool>(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::x86_64::{
        __m512i, _mm512_and_si512, _mm512_loadu_si512, _mm512_set1_epi8, _mm512_shuffle_epi8,
        _mm512_srli_epi64, _mm512_storeu_si512, _mm512_xor_si512,
    };

    let (low_half, high_half) = super::load_table_halves(c);
    // SAFETY: `low_half`/`high_half` are 16-byte table halves; broadcast to 512-bit.
    let (low_tbl, high_tbl): (__m512i, __m512i) =
        unsafe { load_tables_avx512(low_half, high_half) };
    let nibble_mask: __m512i = _mm512_set1_epi8(0x0f);

    // Round down to 64-byte boundary so all SIMD loads/stores are in-bounds.
    let bytes_done = input.len() & !63usize;
    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);

    let (simd_input_chunks, []) = simd_input.as_chunks::<64>() else {
        unreachable!("AVX-512 input is split on a 64-byte boundary");
    };
    let (simd_out_chunks, []) = simd_out.as_chunks_mut::<64>() else {
        unreachable!("AVX-512 output is split on a 64-byte boundary");
    };
    for (input_chunk, out_chunk) in simd_input_chunks.iter().zip(simd_out_chunks.iter_mut()) {
        // SAFETY: `as_chunks::<64>()` guarantees 64 valid bytes for load/store.
        let input_vec = unsafe { _mm512_loadu_si512(input_chunk.as_ptr().cast()) };
        let low = _mm512_and_si512(input_vec, nibble_mask);
        let high = _mm512_and_si512(_mm512_srli_epi64::<4>(input_vec), nibble_mask);
        let product = _mm512_xor_si512(
            _mm512_shuffle_epi8(low_tbl, low),
            _mm512_shuffle_epi8(high_tbl, high),
        );
        if XOR {
            // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for load/store.
            let out_vec = unsafe { _mm512_loadu_si512(out_chunk.as_ptr().cast()) };
            // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for this unaligned store.
            unsafe {
                _mm512_storeu_si512(
                    out_chunk.as_mut_ptr().cast(),
                    _mm512_xor_si512(out_vec, product),
                )
            };
        } else {
            // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for the store.
            unsafe { _mm512_storeu_si512(out_chunk.as_mut_ptr().cast(), product) };
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
    feature = "simd-avx512",
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

    const LENGTHS: [usize; 8] = [0usize, 1, 63, 64, 65, 255, 1024, 10_003];

    #[test]
    fn avx512_matches_scalar_mul_slice() {
        if !(std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut avx512 = vec![0; len];

                mul_slice_scalar_for_test(c, &input, &mut scalar);
                rust_avx512_mul_slice(c, &input, &mut avx512);

                assert_eq!(scalar, avx512);
            }
        }
    }

    #[test]
    fn avx512_matches_scalar_mul_slice_xor() {
        if !(std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut avx512 = vec![0; len];
                fill_random(&mut scalar);
                avx512.copy_from_slice(&scalar);

                mul_slice_xor_scalar_for_test(c, &input, &mut scalar);
                rust_avx512_mul_slice_xor(c, &input, &mut avx512);

                assert_eq!(scalar, avx512);
            }
        }
    }

    #[test]
    fn avx512_matches_avx2_mul_slice() {
        if !(std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut avx2 = vec![0; len];
                let mut avx512 = vec![0; len];

                x86::avx2::rust_avx2_mul_slice(c, &input, &mut avx2);
                rust_avx512_mul_slice(c, &input, &mut avx512);

                assert_eq!(avx2, avx512);
            }
        }
    }
}
