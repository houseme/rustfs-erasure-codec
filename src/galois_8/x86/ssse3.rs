#[cfg(test)]
extern crate alloc;

#[cfg(all(
    feature = "simd-ssse3",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_ssse3_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
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
    // SAFETY: reached only after a runtime `is_x86_feature_detected!("ssse3")` check in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "ssse3")]` requirement.
    unsafe { rust_ssse3_mul_impl::<false>(c, input, out) }
}

#[cfg(all(
    feature = "simd-ssse3",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_ssse3_mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
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
    // SAFETY: reached only after a runtime `is_x86_feature_detected!("ssse3")` check in the
    // dispatcher, satisfying the callee's `#[target_feature(enable = "ssse3")]` requirement.
    unsafe { rust_ssse3_mul_impl::<true>(c, input, out) }
}

#[cfg(all(
    feature = "simd-ssse3",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "ssse3")]
unsafe fn rust_ssse3_mul_impl<const XOR: bool>(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_loadu_si128, _mm_set1_epi8, _mm_shuffle_epi8, _mm_srli_epi64,
        _mm_storeu_si128, _mm_xor_si128,
    };

    let (low_half, high_half) = super::load_table_halves(c);
    // SAFETY: `low_half`/`high_half` are 16-byte aligned table halves (from MUL_TABLE_LOW/HIGH).
    let low_tbl: __m128i = unsafe { _mm_loadu_si128(low_half.as_ptr().cast()) };
    // SAFETY: `high_half` is a 16-byte aligned table half (from MUL_TABLE_HIGH); read via unaligned load.
    let high_tbl: __m128i = unsafe { _mm_loadu_si128(high_half.as_ptr().cast()) };
    let nibble_mask: __m128i = _mm_set1_epi8(0x0f);

    // Round down to 16-byte boundary so all SIMD loads/stores are in-bounds.
    let bytes_done = input.len() & !15usize;
    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);

    let (simd_input_chunks, []) = simd_input.as_chunks::<16>() else {
        unreachable!("SSSE3 input is split on a 16-byte boundary");
    };
    let (simd_out_chunks, []) = simd_out.as_chunks_mut::<16>() else {
        unreachable!("SSSE3 output is split on a 16-byte boundary");
    };
    for (input_chunk, out_chunk) in simd_input_chunks.iter().zip(simd_out_chunks.iter_mut()) {
        // SAFETY: `as_chunks::<16>()` yields exactly 16 valid bytes for this unaligned load.
        let input_vec = unsafe { _mm_loadu_si128(input_chunk.as_ptr().cast()) };
        let low = _mm_and_si128(input_vec, nibble_mask);
        let high = _mm_and_si128(_mm_srli_epi64::<4>(input_vec), nibble_mask);
        let product = _mm_xor_si128(
            _mm_shuffle_epi8(low_tbl, low),
            _mm_shuffle_epi8(high_tbl, high),
        );
        if XOR {
            // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned load of the current output.
            let out_vec = unsafe { _mm_loadu_si128(out_chunk.as_ptr().cast()) };
            // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned store.
            unsafe {
                _mm_storeu_si128(
                    out_chunk.as_mut_ptr().cast(),
                    _mm_xor_si128(out_vec, product),
                )
            };
        } else {
            // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned store.
            unsafe { _mm_storeu_si128(out_chunk.as_mut_ptr().cast(), product) };
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
    feature = "simd-ssse3",
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

    const LENGTHS: [usize; 10] = [0usize, 1, 15, 16, 17, 31, 32, 33, 255, 10_003];

    #[test]
    fn ssse3_matches_scalar_mul_slice() {
        if !std::is_x86_feature_detected!("ssse3") {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut ssse3 = vec![0; len];

                mul_slice_scalar_for_test(c, &input, &mut scalar);
                rust_ssse3_mul_slice(c, &input, &mut ssse3);

                assert_eq!(scalar, ssse3);
            }
        }
    }

    #[test]
    fn ssse3_matches_scalar_mul_slice_xor() {
        if !std::is_x86_feature_detected!("ssse3") {
            return;
        }
        for &len in &LENGTHS {
            for _ in 0..16 {
                let c = rand::random::<u8>();
                let mut input = vec![0; len];
                fill_random(&mut input);
                let mut scalar = vec![0; len];
                let mut ssse3 = vec![0; len];
                fill_random(&mut scalar);
                ssse3.copy_from_slice(&scalar);

                mul_slice_xor_scalar_for_test(c, &input, &mut scalar);
                rust_ssse3_mul_slice_xor(c, &input, &mut ssse3);

                assert_eq!(scalar, ssse3);
            }
        }
    }
}
