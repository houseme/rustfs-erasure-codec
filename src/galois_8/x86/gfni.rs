#[cfg(test)]
extern crate alloc;

// Keep the general scalar loop out of the native vector frame. It is also
// the direct path for inputs shorter than one vector; zero/unit shortcuts are
// shared by the four checked entries below.
#[inline(never)]
fn scalar_tail<const XOR: bool>(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    let table = &super::super::MUL_TABLE[c as usize];
    for (&input, output) in input.iter().zip(out) {
        let product = table[input as usize];
        if XOR {
            *output ^= product;
        } else {
            *output = product;
        }
    }
}

#[inline]
fn handle_scalar_case<const XOR: bool, const WIDTH: usize>(
    c: u8,
    input: &[u8],
    out: &mut [u8],
) -> bool {
    assert_eq!(input.len(), out.len());
    if input.is_empty() {
        return true;
    }
    if c == 0 {
        if !XOR {
            out.fill(0);
        }
        return true;
    }
    if c == 1 {
        if XOR {
            for (&input, output) in input.iter().zip(out) {
                *output ^= input;
            }
        } else {
            out.copy_from_slice(input);
        }
        return true;
    }
    if input.len() < WIDTH {
        scalar_tail::<XOR>(c, input, out);
        return true;
    }
    false
}

#[cfg(all(
    feature = "simd-gfni",
    target_arch = "x86_64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_gfni_avx2_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
    if handle_scalar_case::<false, 32>(c, input, out) {
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
    if handle_scalar_case::<true, 32>(c, input, out) {
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
    if handle_scalar_case::<false, 64>(c, input, out) {
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
    if handle_scalar_case::<true, 64>(c, input, out) {
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

    if !tail_input.is_empty() {
        scalar_tail::<XOR>(c, tail_input, tail_out);
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

    if !tail_input.is_empty() {
        scalar_tail::<XOR>(c, tail_input, tail_out);
    }
}

// Within each 128-bit lane, keep eight X coordinates followed by eight
// constants. GFNI assigns one independent 8x8 map to each group of eight bytes.
const TOWER_PLANES: [u8; 16] = [0, 2, 4, 6, 8, 10, 12, 14, 1, 3, 5, 7, 9, 11, 13, 15];
const TOWER_PACK: [u8; 16] = [0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15];

fn tower_scalar_tail(coefficients: [u8; 4], input: &[[u8; 2]], out: &mut [[u8; 2]], xor: bool) {
    let [a, b, c, d] = coefficients;
    for (input, output) in input.iter().zip(out) {
        let product = [
            super::super::mul(a, input[0]) ^ super::super::mul(b, input[1]),
            super::super::mul(c, input[0]) ^ super::super::mul(d, input[1]),
        ];
        if xor {
            output[0] ^= product[0];
            output[1] ^= product[1];
        } else {
            *output = product;
        }
    }
}

pub(crate) fn rust_gfni_avx2_mul_tower(
    coefficients: [u8; 4],
    input: &[[u8; 2]],
    out: &mut [[u8; 2]],
    xor: bool,
) {
    assert_eq!(input.len(), out.len());
    // SAFETY: the selected backend has already checked GFNI and AVX2. Each
    // implementation rounds to complete vectors and handles the remaining
    // elements with safe scalar operations.
    unsafe {
        if xor {
            gfni_avx2_tower_impl::<true>(coefficients, input, out);
        } else {
            gfni_avx2_tower_impl::<false>(coefficients, input, out);
        }
    }
}

#[target_feature(enable = "gfni,avx2")]
unsafe fn gfni_avx2_tower_impl<const XOR: bool>(
    coefficients: [u8; 4],
    input: &[[u8; 2]],
    out: &mut [[u8; 2]],
) {
    use core::arch::x86_64::*;
    let [a, b, c, d] = coefficients.map(|c| super::super::GFNI_MULTIPLY_MATRICES[c as usize]);
    let diagonal = _mm256_set_epi64x(d, a, d, a);
    let crossed = _mm256_set_epi64x(c, b, c, b);
    // SAFETY: each mask contains exactly 16 initialized bytes.
    let split_mask =
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(TOWER_PLANES.as_ptr().cast()) });
    // SAFETY: each mask contains exactly 16 initialized bytes.
    let pack_mask =
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(TOWER_PACK.as_ptr().cast()) });
    let done = input.len() / 16 * 16;
    let (vectors, tail) = input.split_at(done);
    let (outputs, output_tail) = out.split_at_mut(done);
    for (input, output) in vectors
        .as_chunks::<16>()
        .0
        .iter()
        .zip(outputs.as_chunks_mut::<16>().0)
    {
        // SAFETY: each chunk contains 16 complete two-byte elements.
        let input = unsafe { _mm256_loadu_si256(input.as_ptr().cast()) };
        let planes = _mm256_shuffle_epi8(input, split_mask);
        // Swap adjacent 64-bit planes within each 128-bit lane.
        let other = _mm256_shuffle_epi32::<0x4e>(planes);
        let product = _mm256_xor_si256(
            _mm256_gf2p8affine_epi64_epi8::<0>(planes, diagonal),
            _mm256_gf2p8affine_epi64_epi8::<0>(other, crossed),
        );
        let mut packed = _mm256_shuffle_epi8(product, pack_mask);
        if XOR {
            // SAFETY: this output chunk contains 32 initialized bytes.
            packed = _mm256_xor_si256(packed, unsafe {
                _mm256_loadu_si256(output.as_ptr().cast())
            });
        }
        // SAFETY: this output chunk contains 32 writable bytes. Unaligned
        // loads/stores preserve the alignment-one GF16 element contract.
        unsafe { _mm256_storeu_si256(output.as_mut_ptr().cast(), packed) };
    }
    tower_scalar_tail(coefficients, tail, output_tail, XOR);
}

pub(crate) fn rust_gfni_avx512_mul_tower(
    coefficients: [u8; 4],
    input: &[[u8; 2]],
    out: &mut [[u8; 2]],
    xor: bool,
) {
    assert_eq!(input.len(), out.len());
    // SAFETY: the selected backend has checked GFNI, AVX512F and AVX512BW.
    // Complete chunks and the safe tail preserve both slice bounds.
    unsafe {
        if xor {
            gfni_avx512_tower_impl::<true>(coefficients, input, out);
        } else {
            gfni_avx512_tower_impl::<false>(coefficients, input, out);
        }
    }
}

#[target_feature(enable = "gfni,avx512f,avx512bw")]
unsafe fn gfni_avx512_tower_impl<const XOR: bool>(
    coefficients: [u8; 4],
    input: &[[u8; 2]],
    out: &mut [[u8; 2]],
) {
    use core::arch::x86_64::*;
    let [a, b, c, d] = coefficients.map(|c| super::super::GFNI_MULTIPLY_MATRICES[c as usize]);
    let diagonal = _mm512_set_epi64(d, a, d, a, d, a, d, a);
    let crossed = _mm512_set_epi64(c, b, c, b, c, b, c, b);
    // SAFETY: each mask contains exactly 16 initialized bytes.
    let split_mask =
        _mm512_broadcast_i32x4(unsafe { _mm_loadu_si128(TOWER_PLANES.as_ptr().cast()) });
    // SAFETY: each mask contains exactly 16 initialized bytes.
    let pack_mask = _mm512_broadcast_i32x4(unsafe { _mm_loadu_si128(TOWER_PACK.as_ptr().cast()) });
    let done = input.len() / 32 * 32;
    let (vectors, tail) = input.split_at(done);
    let (outputs, output_tail) = out.split_at_mut(done);
    for (input, output) in vectors
        .as_chunks::<32>()
        .0
        .iter()
        .zip(outputs.as_chunks_mut::<32>().0)
    {
        // SAFETY: each chunk contains 32 complete two-byte elements.
        let input = unsafe { _mm512_loadu_si512(input.as_ptr().cast()) };
        let planes = _mm512_shuffle_epi8(input, split_mask);
        let other = _mm512_shuffle_epi32::<0x4e>(planes);
        let product = _mm512_xor_si512(
            _mm512_gf2p8affine_epi64_epi8::<0>(planes, diagonal),
            _mm512_gf2p8affine_epi64_epi8::<0>(other, crossed),
        );
        let mut packed = _mm512_shuffle_epi8(product, pack_mask);
        if XOR {
            // SAFETY: this output chunk contains 64 initialized bytes.
            packed = _mm512_xor_si512(packed, unsafe {
                _mm512_loadu_si512(output.as_ptr().cast())
            });
        }
        // SAFETY: this chunk contains 64 writable bytes; unaligned storage is
        // valid because each GF16 element has byte alignment.
        unsafe { _mm512_storeu_si512(output.as_mut_ptr().cast(), packed) };
    }
    tower_scalar_tail(coefficients, tail, output_tail, XOR);
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
        let input: alloc::vec::Vec<u8> = (0..4102).map(|i| (i * 173 + 11) as u8).collect();
        for coefficient in 0..=u8::MAX {
            for (input_start, output_start) in [(0, 0), (1, 3), (3, 1)] {
                for len in [
                    0, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 1023,
                    1024, 1025, 4096, 4097,
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

    type TowerSliceFn = fn([u8; 4], &[[u8; 2]], &mut [[u8; 2]], bool);

    fn exercise_tower_coefficients(multiply: TowerSliceFn) {
        use crate::Field as _;
        use crate::galois_16::{Field, tower_mul_coefficients};

        let basis: [[u8; 2]; 32] = core::array::from_fn(|i| {
            let bit = i % 16;
            if bit < 8 {
                [0, 1 << bit]
            } else {
                [1 << (bit - 8), 0]
            }
        });
        // Every one of the 65,536 coefficient maps is linear in both input
        // bytes. Check all sixteen input basis bits in every vector lane.
        for coefficient in 0..65536 {
            let scalar = Field::nth_internal(coefficient);
            let coefficients = tower_mul_coefficients(scalar);
            for xor in [false, true] {
                let mut output = [[0xa5, 0x5a]; 34];
                multiply(coefficients, &basis, &mut output[1..33], xor);
                assert_eq!(output[0], [0xa5, 0x5a]);
                assert_eq!(output[33], [0xa5, 0x5a]);
                for (input, output) in basis.iter().zip(&output[1..33]) {
                    let product = Field::mul(scalar, *input);
                    assert_eq!(
                        *output,
                        if xor {
                            Field::add([0xa5, 0x5a], product)
                        } else {
                            product
                        }
                    );
                }
            }
        }

        let input: alloc::vec::Vec<u8> = (0..2060).map(|i| (i * 173 + 11) as u8).collect();
        for scalar in [
            [0, 0],
            [0, 1],
            [0, 0x8e],
            [1, 0],
            [1, 1],
            [0x9a, 0x3f],
            [0xff, 0xff],
        ] {
            for (input_start, output_start) in [(0, 0), (1, 3), (3, 1)] {
                for len in [
                    0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 1023, 1024,
                    1025,
                ] {
                    let source = input[input_start..input_start + len * 2].as_chunks::<2>().0;
                    for xor in [false, true] {
                        let mut output = vec![0xa5; output_start + len * 2 + 1];
                        multiply(
                            tower_mul_coefficients(scalar),
                            source,
                            output[output_start..output_start + len * 2]
                                .as_chunks_mut::<2>()
                                .0,
                            xor,
                        );
                        assert!(output[..output_start].iter().all(|&b| b == 0xa5));
                        assert_eq!(output[output_start + len * 2], 0xa5);
                        for (i, input) in source.iter().enumerate() {
                            let product = Field::mul(scalar, *input);
                            let expected = if xor {
                                Field::add([0xa5; 2], product)
                            } else {
                                product
                            };
                            assert_eq!(
                                &output[output_start + i * 2..output_start + i * 2 + 2],
                                &expected
                            );
                        }
                    }
                }
            }
        }

        let all_inputs: alloc::vec::Vec<_> = (0..65536).map(Field::nth_internal).collect();
        for scalar in [[1, 0], [1, 1], [0x9a, 0x3f], [0xff, 0xff]] {
            for xor in [false, true] {
                let mut output = vec![[0xa5, 0x5a]; all_inputs.len()];
                multiply(
                    tower_mul_coefficients(scalar),
                    &all_inputs,
                    &mut output,
                    xor,
                );
                for (input, output) in all_inputs.iter().zip(output) {
                    let product = Field::mul(scalar, *input);
                    assert_eq!(
                        output,
                        if xor {
                            Field::add([0xa5, 0x5a], product)
                        } else {
                            product
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn gfni_avx2_tower_coefficients_and_unaligned_boundaries() {
        if !(std::is_x86_feature_detected!("gfni") && std::is_x86_feature_detected!("avx2")) {
            return;
        }
        exercise_tower_coefficients(rust_gfni_avx2_mul_tower);
        std::println!("GFNI_AVX2_TOWER_EXECUTED");
    }

    #[test]
    fn gfni_avx512_tower_coefficients_and_unaligned_boundaries() {
        if !(std::is_x86_feature_detected!("gfni")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw"))
        {
            return;
        }
        exercise_tower_coefficients(rust_gfni_avx512_mul_tower);
        std::println!("GFNI_AVX512_TOWER_EXECUTED");
    }

    #[test]
    fn scalar_entry_handles_only_short_or_trivial_cases_before_any_native_write() {
        fn check<const XOR: bool, const WIDTH: usize>() {
            let input: alloc::vec::Vec<u8> = (0..4097).map(|i| (i * 173 + 11) as u8).collect();
            for coefficient in 0..=u8::MAX {
                for len in [0, 1, 31, 32, 33, 63, 64, 65, 128, 1025, 4096] {
                    let mut output = vec![0xa5; len];
                    let handled =
                        handle_scalar_case::<XOR, WIDTH>(coefficient, &input[..len], &mut output);
                    assert_eq!(handled, coefficient <= 1 || len < WIDTH);
                    for (offset, &actual) in output.iter().enumerate() {
                        let expected = if handled {
                            let product = crate::galois_8::mul(coefficient, input[offset]);
                            if XOR { 0xa5 ^ product } else { product }
                        } else {
                            0xa5
                        };
                        assert_eq!(actual, expected);
                    }
                }
            }
            for coefficient in [0, 1, 2, 0x9a] {
                for len in [0, 1, 31, 32, 64, 65] {
                    let mut output = vec![0xa5; len + 1];
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handle_scalar_case::<XOR, WIDTH>(coefficient, &input[..len], &mut output)
                    }));
                    assert!(result.is_err());
                    assert!(output.iter().all(|&value| value == 0xa5));
                }
            }
        }
        check::<false, 32>();
        check::<true, 32>();
        check::<false, 64>();
        check::<true, 64>();
        std::println!("GFNI_SCALAR_ENTRY_CONTRACT_EXECUTED");
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
