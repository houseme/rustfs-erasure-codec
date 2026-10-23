#[cfg(test)]
extern crate alloc;

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
        _mm256_gf2p8affine_epi64_epi8, _mm256_loadu_si256, _mm256_set1_epi64x, _mm256_storeu_si256,
        _mm256_xor_si256,
    };

    let matrix = _mm256_set1_epi64x(super::super::GFNI_MULTIPLY_MATRICES[c as usize]);

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
        let restored = _mm256_gf2p8affine_epi64_epi8(input_vec, matrix, 0);
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
        _mm512_gf2p8affine_epi64_epi8, _mm512_loadu_si512, _mm512_set1_epi64, _mm512_storeu_si512,
        _mm512_xor_si512,
    };

    let matrix = _mm512_set1_epi64(super::super::GFNI_MULTIPLY_MATRICES[c as usize]);

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
        let restored = _mm512_gf2p8affine_epi64_epi8::<0>(input_vec, matrix);
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

    fn exercise_all_coefficients(
        multiply: fn(u8, &[u8], &mut [u8]),
        accumulate: fn(u8, &[u8], &mut [u8]),
    ) {
        let input: alloc::vec::Vec<u8> = (0..1030).map(|i| (i * 173 + 11) as u8).collect();
        for coefficient in 0..=u8::MAX {
            for (input_start, output_start) in [(0, 0), (1, 3), (3, 1)] {
                for len in [
                    0, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 1023,
                    1024, 1025,
                ] {
                    let source = &input[input_start..input_start + len];
                    let mut output = vec![0xa5; output_start + len + 1];
                    multiply(
                        coefficient,
                        source,
                        &mut output[output_start..output_start + len],
                    );
                    for (offset, &actual) in output.iter().enumerate() {
                        let expected = if (output_start..output_start + len).contains(&offset) {
                            crate::galois_8::mul(coefficient, source[offset - output_start])
                        } else {
                            0xa5
                        };
                        assert_eq!(
                            actual, expected,
                            "multiply c={coefficient}, len={len}, offset={offset}"
                        );
                    }
                    output.fill(0xa5);
                    accumulate(
                        coefficient,
                        source,
                        &mut output[output_start..output_start + len],
                    );
                    for (offset, &actual) in output.iter().enumerate() {
                        let expected = if (output_start..output_start + len).contains(&offset) {
                            0xa5 ^ crate::galois_8::mul(coefficient, source[offset - output_start])
                        } else {
                            0xa5
                        };
                        assert_eq!(
                            actual, expected,
                            "accumulate c={coefficient}, len={len}, offset={offset}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn gfni_avx2_exhaustive_coefficients_and_boundaries() {
        if !(std::is_x86_feature_detected!("gfni") && std::is_x86_feature_detected!("avx2")) {
            return;
        }
        exercise_all_coefficients(rust_gfni_avx2_mul_slice, rust_gfni_avx2_mul_slice_xor);
        std::println!("GFNI_AVX2_EXHAUSTIVE_EXECUTED");
    }

    #[test]
    fn gfni_avx512_exhaustive_coefficients_and_boundaries() {
        if !(std::is_x86_feature_detected!("gfni")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        exercise_all_coefficients(rust_gfni_avx512_mul_slice, rust_gfni_avx512_mul_slice_xor);
        std::println!("GFNI_AVX512_EXHAUSTIVE_EXECUTED");
    }

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
