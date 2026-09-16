#[cfg(feature = "std")]
use super::super::profile::RUST_NEON_PROFILE_METRICS;

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_neon_mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
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

    // SAFETY: NEON is unconditionally available on aarch64, so the callee's
    // `#[target_feature(enable = "neon")]` requirement is always satisfied.
    unsafe { rust_neon_mul_slice_impl(c, input, out) }
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) fn rust_neon_mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if input.is_empty() {
        return;
    }
    if c == 0 {
        return;
    }
    if c == 1 {
        // SAFETY: NEON is unconditionally available on aarch64, so the callee's
        // `#[target_feature(enable = "neon")]` requirement is always satisfied.
        unsafe { rust_neon_mul_slice_xor_c1_impl(input, out) }
        return;
    }

    // SAFETY: NEON is unconditionally available on aarch64, so the callee's
    // `#[target_feature(enable = "neon")]` requirement is always satisfied.
    unsafe { rust_neon_mul_slice_xor_impl(c, input, out) }
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "neon")]
unsafe fn rust_neon_mul_slice_impl(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::aarch64::{
        uint8x16_t, uint8x16x4_t, vandq_u8, vdupq_n_u8, veorq_u8, vld1q_u8, vld1q_u8_x4,
        vqtbl1q_u8, vshrq_n_u8, vst1q_u8, vst1q_u8_x4,
    };

    // SAFETY: reads a 16-byte multiply-table row via an unaligned NEON load;
    // NEON is available in this `#[target_feature]` fn.
    let low_tbl = unsafe { vld1q_u8(super::super::MUL_TABLE_LOW[c as usize].as_ptr()) };
    // SAFETY: reads a 16-byte multiply-table row via an unaligned NEON load;
    // NEON is available in this `#[target_feature]` fn.
    let high_tbl = unsafe { vld1q_u8(super::super::MUL_TABLE_HIGH[c as usize].as_ptr()) };
    let nibble_mask = vdupq_n_u8(0x0f);
    // `bytes_done` rounds down to the largest multiple of 16 (NEON register width),
    // ensuring all SIMD loads/stores operate on in-bounds, 16-byte-aligned chunks.
    let bytes_done = input.len() & !15usize;
    let bytes_done_unrolled = input.len() & !63usize;
    #[cfg(feature = "std")]
    {
        let vector_64b_chunks = bytes_done_unrolled / 64;
        let vector_16b_chunks = (bytes_done - bytes_done_unrolled) / 16;
        let tail_bytes = input.len() - bytes_done;
        RUST_NEON_PROFILE_METRICS.record_call(
            false,
            input.len(),
            vector_64b_chunks,
            vector_16b_chunks,
            tail_bytes,
        );
    }

    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);
    let (unrolled_input, remainder_input) = simd_input.split_at(bytes_done_unrolled);
    let (unrolled_out, remainder_out) = simd_out.split_at_mut(bytes_done_unrolled);

    let (unrolled_input_chunks, []) = unrolled_input.as_chunks::<64>() else {
        unreachable!("unrolled NEON input is split on a 64-byte boundary");
    };
    let (unrolled_out_chunks, []) = unrolled_out.as_chunks_mut::<64>() else {
        unreachable!("unrolled NEON output is split on a 64-byte boundary");
    };
    for (input_chunk, out_chunk) in unrolled_input_chunks
        .iter()
        .zip(unrolled_out_chunks.iter_mut())
    {
        // SAFETY: `as_chunks::<64>()` guarantees the pointer spans 64 valid bytes.
        let inputs: uint8x16x4_t = unsafe { vld1q_u8_x4(input_chunk.as_ptr()) };
        let input0 = inputs.0;
        let input1 = inputs.1;
        let input2 = inputs.2;
        let input3 = inputs.3;

        let low0 = vandq_u8(input0, nibble_mask);
        let low1 = vandq_u8(input1, nibble_mask);
        let low2 = vandq_u8(input2, nibble_mask);
        let low3 = vandq_u8(input3, nibble_mask);

        let high0 = vshrq_n_u8::<4>(input0);
        let high1 = vshrq_n_u8::<4>(input1);
        let high2 = vshrq_n_u8::<4>(input2);
        let high3 = vshrq_n_u8::<4>(input3);

        let result0: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low0), vqtbl1q_u8(high_tbl, high0));
        let result1: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low1), vqtbl1q_u8(high_tbl, high1));
        let result2: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low2), vqtbl1q_u8(high_tbl, high2));
        let result3: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low3), vqtbl1q_u8(high_tbl, high3));

        // SAFETY: `as_chunks_mut::<64>()` guarantees 64 valid bytes for the store.
        unsafe {
            vst1q_u8_x4(
                out_chunk.as_mut_ptr(),
                uint8x16x4_t(result0, result1, result2, result3),
            )
        };
    }

    // Scalar-tail fallback for remaining 0..15 bytes after SIMD processing.
    let (remainder_input_chunks, []) = remainder_input.as_chunks::<16>() else {
        unreachable!("remainder NEON input is split on a 16-byte boundary");
    };
    let (remainder_out_chunks, []) = remainder_out.as_chunks_mut::<16>() else {
        unreachable!("remainder NEON output is split on a 16-byte boundary");
    };
    for (input_chunk, out_chunk) in remainder_input_chunks
        .iter()
        .zip(remainder_out_chunks.iter_mut())
    {
        // SAFETY: `as_chunks::<16>()` guarantees 16 valid bytes.
        let input_vec = unsafe { vld1q_u8(input_chunk.as_ptr()) };
        let low = vandq_u8(input_vec, nibble_mask);
        let high = vshrq_n_u8::<4>(input_vec);
        let result: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low), vqtbl1q_u8(high_tbl, high));
        // SAFETY: `as_chunks_mut::<16>()` guarantees 16 valid bytes for the store.
        unsafe { vst1q_u8(out_chunk.as_mut_ptr(), result) };
    }

    super::super::scalar::mul_slice_pure_rust(c, tail_input, tail_out);
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "neon")]
unsafe fn rust_neon_mul_slice_xor_impl(c: u8, input: &[u8], out: &mut [u8]) {
    use core::arch::aarch64::{
        uint8x16_t, uint8x16x4_t, vandq_u8, vdupq_n_u8, veorq_u8, vld1q_u8, vld1q_u8_x4,
        vqtbl1q_u8, vshrq_n_u8, vst1q_u8, vst1q_u8_x4,
    };

    // SAFETY: reads a 16-byte multiply-table row via an unaligned NEON load;
    // NEON is available in this `#[target_feature]` fn.
    let low_tbl = unsafe { vld1q_u8(super::super::MUL_TABLE_LOW[c as usize].as_ptr()) };
    // SAFETY: reads a 16-byte multiply-table row via an unaligned NEON load;
    // NEON is available in this `#[target_feature]` fn.
    let high_tbl = unsafe { vld1q_u8(super::super::MUL_TABLE_HIGH[c as usize].as_ptr()) };
    let nibble_mask = vdupq_n_u8(0x0f);

    // 4x unrolled main loop: process 64 bytes per iteration.
    let bytes_done = input.len() & !15usize;
    let bytes_done_unrolled = input.len() & !63usize;
    #[cfg(feature = "std")]
    {
        let vector_64b_chunks = bytes_done_unrolled / 64;
        let vector_16b_chunks = (bytes_done - bytes_done_unrolled) / 16;
        let tail_bytes = input.len() - bytes_done;
        RUST_NEON_PROFILE_METRICS.record_call(
            true,
            input.len(),
            vector_64b_chunks,
            vector_16b_chunks,
            tail_bytes,
        );
    }

    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);
    let (unrolled_input, remainder_input) = simd_input.split_at(bytes_done_unrolled);
    let (unrolled_out, remainder_out) = simd_out.split_at_mut(bytes_done_unrolled);

    let (unrolled_input_chunks, []) = unrolled_input.as_chunks::<64>() else {
        unreachable!("unrolled NEON input is split on a 64-byte boundary");
    };
    let (unrolled_out_chunks, []) = unrolled_out.as_chunks_mut::<64>() else {
        unreachable!("unrolled NEON output is split on a 64-byte boundary");
    };
    for (input_chunk, out_chunk) in unrolled_input_chunks
        .iter()
        .zip(unrolled_out_chunks.iter_mut())
    {
        // SAFETY: `as_chunks::<64>()` yields exactly 64 valid bytes for this unaligned NEON load.
        let inputs: uint8x16x4_t = unsafe { vld1q_u8_x4(input_chunk.as_ptr()) };
        let input0 = inputs.0;
        let input1 = inputs.1;
        let input2 = inputs.2;
        let input3 = inputs.3;

        let low0 = vandq_u8(input0, nibble_mask);
        let low1 = vandq_u8(input1, nibble_mask);
        let low2 = vandq_u8(input2, nibble_mask);
        let low3 = vandq_u8(input3, nibble_mask);

        let high0 = vshrq_n_u8::<4>(input0);
        let high1 = vshrq_n_u8::<4>(input1);
        let high2 = vshrq_n_u8::<4>(input2);
        let high3 = vshrq_n_u8::<4>(input3);

        let product0: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low0), vqtbl1q_u8(high_tbl, high0));
        let product1: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low1), vqtbl1q_u8(high_tbl, high1));
        let product2: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low2), vqtbl1q_u8(high_tbl, high2));
        let product3: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low3), vqtbl1q_u8(high_tbl, high3));
        // SAFETY: `as_chunks_mut::<64>()` yields exactly 64 valid bytes for this unaligned load of the current output.
        let outs: uint8x16x4_t = unsafe { vld1q_u8_x4(out_chunk.as_ptr()) };
        // SAFETY: `as_chunks_mut::<64>()` yields exactly 64 valid bytes for this unaligned NEON store.
        unsafe {
            vst1q_u8_x4(
                out_chunk.as_mut_ptr(),
                uint8x16x4_t(
                    veorq_u8(outs.0, product0),
                    veorq_u8(outs.1, product1),
                    veorq_u8(outs.2, product2),
                    veorq_u8(outs.3, product3),
                ),
            )
        };
    }

    let (remainder_input_chunks, []) = remainder_input.as_chunks::<16>() else {
        unreachable!("remainder NEON input is split on a 16-byte boundary");
    };
    let (remainder_out_chunks, []) = remainder_out.as_chunks_mut::<16>() else {
        unreachable!("remainder NEON output is split on a 16-byte boundary");
    };
    for (input_chunk, out_chunk) in remainder_input_chunks
        .iter()
        .zip(remainder_out_chunks.iter_mut())
    {
        // SAFETY: `as_chunks::<16>()` yields exactly 16 valid bytes for this unaligned NEON load.
        let input_vec = unsafe { vld1q_u8(input_chunk.as_ptr()) };
        let low = vandq_u8(input_vec, nibble_mask);
        let high = vshrq_n_u8::<4>(input_vec);
        let product: uint8x16_t = veorq_u8(vqtbl1q_u8(low_tbl, low), vqtbl1q_u8(high_tbl, high));
        // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned load of the current output.
        let out_vec = unsafe { vld1q_u8(out_chunk.as_ptr()) };
        // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned NEON store.
        unsafe { vst1q_u8(out_chunk.as_mut_ptr(), veorq_u8(out_vec, product)) };
    }

    super::super::scalar::mul_slice_xor_pure_rust(c, tail_input, tail_out);
}

#[cfg(all(
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
#[target_feature(enable = "neon")]
unsafe fn rust_neon_mul_slice_xor_c1_impl(input: &[u8], out: &mut [u8]) {
    use core::arch::aarch64::{
        uint8x16x4_t, veorq_u8, vld1q_u8, vld1q_u8_x4, vst1q_u8, vst1q_u8_x4,
    };

    let bytes_done = input.len() & !15usize;
    let bytes_done_unrolled = input.len() & !63usize;
    let (simd_input, tail_input) = input.split_at(bytes_done);
    let (simd_out, tail_out) = out.split_at_mut(bytes_done);
    let (unrolled_input, remainder_input) = simd_input.split_at(bytes_done_unrolled);
    let (unrolled_out, remainder_out) = simd_out.split_at_mut(bytes_done_unrolled);

    let (unrolled_input_chunks, []) = unrolled_input.as_chunks::<64>() else {
        unreachable!("unrolled NEON input is split on a 64-byte boundary");
    };
    let (unrolled_out_chunks, []) = unrolled_out.as_chunks_mut::<64>() else {
        unreachable!("unrolled NEON output is split on a 64-byte boundary");
    };
    for (input_chunk, out_chunk) in unrolled_input_chunks
        .iter()
        .zip(unrolled_out_chunks.iter_mut())
    {
        // SAFETY: `as_chunks::<64>()` yields exactly 64 valid bytes for this unaligned NEON load.
        let inputs: uint8x16x4_t = unsafe { vld1q_u8_x4(input_chunk.as_ptr()) };
        // SAFETY: `as_chunks_mut::<64>()` yields exactly 64 valid bytes for this unaligned load of the current output.
        let outs: uint8x16x4_t = unsafe { vld1q_u8_x4(out_chunk.as_ptr()) };

        // SAFETY: `as_chunks_mut::<64>()` yields exactly 64 valid bytes for this unaligned NEON store.
        unsafe {
            vst1q_u8_x4(
                out_chunk.as_mut_ptr(),
                uint8x16x4_t(
                    veorq_u8(inputs.0, outs.0),
                    veorq_u8(inputs.1, outs.1),
                    veorq_u8(inputs.2, outs.2),
                    veorq_u8(inputs.3, outs.3),
                ),
            )
        };
    }

    let (remainder_input_chunks, []) = remainder_input.as_chunks::<16>() else {
        unreachable!("remainder NEON input is split on a 16-byte boundary");
    };
    let (remainder_out_chunks, []) = remainder_out.as_chunks_mut::<16>() else {
        unreachable!("remainder NEON output is split on a 16-byte boundary");
    };
    for (input_chunk, out_chunk) in remainder_input_chunks
        .iter()
        .zip(remainder_out_chunks.iter_mut())
    {
        // SAFETY: `as_chunks::<16>()` yields exactly 16 valid bytes for this unaligned NEON load.
        let input_vec = unsafe { vld1q_u8(input_chunk.as_ptr()) };
        // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned load of the current output.
        let out_vec = unsafe { vld1q_u8(out_chunk.as_ptr()) };
        // SAFETY: `as_chunks_mut::<16>()` yields exactly 16 valid bytes for this unaligned NEON store.
        unsafe { vst1q_u8(out_chunk.as_mut_ptr(), veorq_u8(input_vec, out_vec)) };
    }

    super::super::scalar::mul_slice_xor_pure_rust(1, tail_input, tail_out);
}
