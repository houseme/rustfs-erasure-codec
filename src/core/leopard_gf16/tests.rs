use super::ops::{
    add_mod16, fft_dit2_16, gf16_mul, ifft_dit2_16, mul_log16, mulgf16, slice_xor_u16, sub_mod16,
};
use super::tables::build_tables16;
use super::*;
use alloc::vec;

#[test]
fn test_leopard_gf16_tables_shapes() {
    let tables = build_tables16();
    assert_eq!(tables.log_lut.len(), ORDER16);
    assert_eq!(tables.exp_lut.len(), ORDER16 * 2);
    assert_eq!(tables.fft_skew.len(), MODULUS16);
    assert_eq!(tables.log_walsh.len(), ORDER16);
}

#[test]
fn test_leopard_gf16_log_exp_roundtrip() {
    let tables = build_tables16();
    for x in 1..ORDER16 {
        let log_x = tables.log_lut[x];
        let recovered = tables.exp_lut[log_x as usize];
        assert_eq!(recovered, x as u16, "x={x}");
    }
}

#[test]
fn test_leopard_gf16_check_luts() {
    let _tables = build_tables16();
    // Check the LFSR cycle directly: x = 2*x mod polynomial
    let mut x: u32 = 1;
    let mut count = 0usize;
    loop {
        count += 1;
        x <<= 1;
        if x >= ORDER16 as u32 {
            x ^= super::POLYNOMIAL16;
        }
        if x == 1 {
            break;
        }
        if count > MODULUS16 {
            break;
        }
    }
    assert_eq!(
        count, MODULUS16,
        "LFSR should cycle through all 65535 nonzero elements"
    );
}

#[test]
fn test_leopard_gf16_mul_identity() {
    let tables = build_tables16();
    for x in 0..ORDER16 {
        let result = gf16_mul(x as u16, 1, &tables.log_lut, &tables.exp_lut);
        assert_eq!(result, x as u16, "x={x}");
    }
}

#[test]
fn test_leopard_gf16_mul_zero() {
    let tables = build_tables16();
    for x in 0..ORDER16 {
        assert_eq!(gf16_mul(0, x as u16, &tables.log_lut, &tables.exp_lut), 0);
        assert_eq!(gf16_mul(x as u16, 0, &tables.log_lut, &tables.exp_lut), 0);
    }
}

#[test]
fn test_leopard_gf16_fft_ifft_roundtrip() {
    let tables = build_tables16();
    let n = 16;
    let original: Vec<u16> = (0..n).map(|i| (i as u16 * 137 + 42) % 65535).collect();
    let mut data = original.clone();

    // Apply FFT butterflies using split_at_mut.
    for i in 0..n / 2 {
        let (left, right) = data.split_at_mut(n / 2);
        fft_dit2_16(
            &mut left[i..i + 1],
            &mut right[i..i + 1],
            (i + 1) as u16,
            &tables,
        );
    }

    // Apply inverse FFT butterflies (reverse order).
    for i in (0..n / 2).rev() {
        let (left, right) = data.split_at_mut(n / 2);
        ifft_dit2_16(
            &mut left[i..i + 1],
            &mut right[i..i + 1],
            (i + 1) as u16,
            &tables,
        );
    }

    assert_eq!(data, original, "FFT -> IFFT should recover original data");
}

#[test]
fn test_leopard_gf16_mulgf16_basic() {
    let tables = build_tables16();
    let input = vec![1u16, 2, 3, 4, 5];
    let mut output = vec![0u16; 5];

    // Multiply by g^0 = 1 (identity).
    mulgf16(&mut output, &input, 0, &tables);
    assert_eq!(output, input, "mul by g^0 should be identity");

    // Multiply by g^MODULUS16 = g^65535 = 1 (wraparound).
    output.fill(0);
    mulgf16(&mut output, &input, super::MODULUS16 as u16, &tables);
    assert_eq!(output, input, "mul by g^65535 should be identity");
}

#[test]
fn test_leopard_gf16_slice_xor_u16() {
    let a = vec![0x1234u16, 0x5678, 0x9ABC, 0xDEF0];
    let mut b = vec![0xFFFFu16, 0x0000, 0x1234, 0x5678];
    slice_xor_u16(&mut b, &a);
    assert_eq!(
        b,
        vec![0x1234 ^ 0xFFFF, 0x5678, 0x9ABC ^ 0x1234, 0xDEF0 ^ 0x5678]
    );
}

#[test]
fn test_leopard_gf16_encode_basic() {
    let data_shards = 4;
    let parity_shards = 2;
    let shard_size = 128;

    let data: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| {
            (0..shard_size)
                .map(|j| ((i * 137 + j * 31) & 0xFF) as u8)
                .collect()
        })
        .collect();
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    let driver =
        encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    assert_eq!(driver.shard_size, shard_size);
    assert_eq!(driver.m, 2);

    let all_zeros = parity.iter().all(|p| p.iter().all(|&b| b == 0));
    assert!(!all_zeros, "parity should be non-zero after encoding");
}

#[test]
fn test_leopard_gf16_encode_verify_roundtrip() {
    let data_shards = 3;
    let parity_shards = 2;
    let shard_size = 64;

    let data: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| vec![i as u8 + 1; shard_size])
        .collect();
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    let mut parity2: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];
    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity2).unwrap();

    assert_eq!(parity, parity2, "re-encoding should produce same parity");
}

/// Test FWHT self-inverse property: fwht(fwht(x)) = x.
#[test]
fn test_leopard_gf16_fwht_self_inverse() {
    // Build a small test vector.
    let mut data = [0u16; super::ORDER16];
    data[0] = 42;
    data[1] = 1000;
    data[2] = 65534;
    data[3] = 12345;
    data[4] = 7;

    let original = data;

    // Apply FWHT twice — should recover original.
    super::ops::fwht16_mtrunc(&mut data, super::ORDER16, super::ORDER16);
    super::ops::fwht16_variable(&mut data[..super::ORDER16]);

    for i in 0..16 {
        assert_eq!(
            data[i], original[i],
            "position {i}: got {:#06x}, expected {:#06x}",
            data[i], original[i]
        );
    }
}

/// Test encode-decode with non-uniform per-position data.
#[test]
fn test_leopard_gf16_reconstruct_nonuniform() {
    let data_shards = 3;
    let parity_shards = 2;
    let total_shards = data_shards + parity_shards;
    let shard_size = 64;

    // Non-uniform data: each byte position has different values across shards.
    let data: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| {
            (0..shard_size)
                .map(|j| ((i * shard_size + j) & 0xFF) as u8)
                .collect()
        })
        .collect();
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    let mut shards: Vec<Vec<u8>> = Vec::new();
    for d in &data {
        shards.push(d.clone());
    }
    for p in &parity {
        shards.push(p.clone());
    }

    let original_0 = shards[0].clone();
    shards[0] = vec![0u8; shard_size];

    let present: Vec<bool> = (0..total_shards).map(|i| i != 0).collect();
    let input_snapshots: Vec<Option<Vec<u8>>> = shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i != 0 { Some(s.clone()) } else { None })
        .collect();
    let input_data: Vec<Option<&[u8]>> = input_snapshots.iter().map(|o| o.as_deref()).collect();
    let mut outputs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    assert_eq!(
        outputs[0],
        original_0.as_slice(),
        "recovered shard 0 should match original"
    );
}

/// Verify basic GF16 arithmetic properties.
#[test]
fn test_leopard_gf16_arithmetic() {
    let tables = build_tables16();

    // Verify log/exp consistency: exp_lut[log_lut[x]] = x for all nonzero x.
    for x in 1u16..65535 {
        let log_x = tables.log_lut[x as usize];
        let recovered = tables.exp_lut[log_x as usize];
        assert_eq!(recovered, x, "exp(log({x})) != {x}, got {recovered}");
    }

    // Verify exp_lut[0] = 1 (multiplicative identity).
    assert_eq!(tables.exp_lut[0], 1);

    // Verify log_lut[1] = 0 (log of identity).
    assert_eq!(tables.log_lut[1], 0);

    // Verify gf16_mul(a, 1) = a for all a.
    for a in 0u16..65535 {
        let result = gf16_mul(a, 1, &tables.log_lut, &tables.exp_lut);
        assert_eq!(result, a, "gf16_mul({a}, 1) = {result}");
    }

    // Verify gf16_mul(0, x) = 0 for all x.
    for x in 0u16..65535 {
        assert_eq!(gf16_mul(0, x, &tables.log_lut, &tables.exp_lut), 0);
        assert_eq!(gf16_mul(x, 0, &tables.log_lut, &tables.exp_lut), 0);
    }

    // Verify mul_log16(a, log_lut[b]) = gf16_mul(a, b).
    let test_vals = [1u16, 2, 3, 100, 1000, 32768, 65534];
    for &a in &test_vals {
        for &b in &test_vals {
            let r1 = gf16_mul(a, b, &tables.log_lut, &tables.exp_lut);
            let r2 = mul_log16(
                a,
                tables.log_lut[b as usize],
                &tables.log_lut,
                &tables.exp_lut,
            );
            assert_eq!(
                r1, r2,
                "gf16_mul({a},{b})={r1} != mul_log16({a},log({b}))={r2}"
            );
        }
    }

    // Verify add_mod16/sub_mod16 roundtrip.
    for a in 0u16..100 {
        for b in 0u16..100 {
            let sum = add_mod16(a, b);
            let diff = sub_mod16(sum, b);
            assert_eq!(
                diff, a,
                "add_mod16/sub_mod16 roundtrip failed for ({a}, {b})"
            );
        }
    }
}

/// Verify fft_skew values match Go's klauspost/reedsolomon library.
#[test]
fn test_leopard_gf16_fft_skew_values() {
    let tables = init_leopard_gf16_tables();

    // Expected values from Go (verified via dump test).
    let expected_skew: [u16; 8] = [
        0xFFFF, 0xFFFF, 0x5555, 0xFFFF, 0x4444, 0x5555, 0x8888, 0xFFFF,
    ];
    for (i, &exp) in expected_skew.iter().enumerate() {
        assert_eq!(
            tables.fft_skew[i], exp,
            "fft_skew[{i}] = {:#06x}, expected {:#06x}",
            tables.fft_skew[i], exp
        );
    }

    // Verify plans build without panic for 3+2 config.
    let data_shards = 3usize;
    let parity_shards = 2usize;
    let m = parity_shards.max(1).next_power_of_two();
    let work_size = m + data_shards;
    let n = work_size.next_power_of_two();
    let input_count = 5usize;

    let _ifft_plan = super::build_ifft_decode_dit16_plan(input_count, n, &tables.fft_skew);
    let _fft_plan = super::build_fft_dit16_plan(work_size, n, &tables.fft_skew);
}

/// Test encode+decode roundtrip directly using encode_with_tables16 and reconstruct_with_tables16.
#[test]
#[allow(clippy::needless_range_loop)]
fn test_leopard_gf16_encode_decode_roundtrip() {
    let data_shards = 3;
    let parity_shards = 2;
    let total_shards = data_shards + parity_shards;
    let shard_size = 64;

    let data: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| {
            (0..shard_size)
                .map(|j| ((i * shard_size + j) & 0xFF) as u8)
                .collect()
        })
        .collect();
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    // Lose shard 0 (data shard).
    let original_0 = data[0].clone();
    let mut shards: Vec<Vec<u8>> = Vec::new();
    for i in 0..data_shards {
        if i == 0 {
            shards.push(vec![0u8; shard_size]);
        } else {
            shards.push(data[i].clone());
        }
    }
    for p in &parity {
        shards.push(p.clone());
    }

    let present: Vec<bool> = (0..total_shards).map(|i| i != 0).collect();
    let input_snapshots: Vec<Option<Vec<u8>>> = shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i != 0 { Some(s.clone()) } else { None })
        .collect();
    let input_data: Vec<Option<&[u8]>> = input_snapshots.iter().map(|o| o.as_deref()).collect();
    let mut outputs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    assert_eq!(
        outputs[0],
        original_0.as_slice(),
        "recovered shard 0 should match original"
    );
}

#[test]
fn test_leopard_gf16_reconstruct_single_missing() {
    let data_shards = 3;
    let parity_shards = 2;
    let total_shards = data_shards + parity_shards;
    let shard_size = 64;

    let mut shards: Vec<Vec<u8>> = (0..total_shards)
        .map(|i| vec![(i + 1) as u8; shard_size])
        .collect();

    // Encode parity.
    {
        let (data_part, parity_part) = shards.split_at_mut(data_shards);
        let data_refs: Vec<&[u8]> = data_part.iter().map(|d| d.as_slice()).collect();
        let mut parity_refs: Vec<&mut [u8]> =
            parity_part.iter_mut().map(|p| p.as_mut_slice()).collect();
        encode::encode_with_tables16(data_shards, parity_shards, &data_refs, &mut parity_refs)
            .unwrap();
    }

    let original_0 = shards[0].clone();

    shards[0] = vec![0u8; shard_size];

    let present: Vec<bool> = (0..total_shards).map(|i| i != 0).collect();
    let input_snapshots: Vec<Option<Vec<u8>>> = shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i != 0 { Some(s.clone()) } else { None })
        .collect();
    let input_data: Vec<Option<&[u8]>> = input_snapshots.iter().map(|o| o.as_deref()).collect();
    let mut outputs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    assert_eq!(
        outputs[0],
        original_0.as_slice(),
        "recovered shard 0 should match original"
    );
}

#[test]
fn test_leopard_gf16_reconstruct_multiple_missing() {
    let data_shards = 4;
    let parity_shards = 3;
    let total_shards = data_shards + parity_shards;
    let shard_size = 128;

    let mut shards: Vec<Vec<u8>> = (0..total_shards)
        .map(|i| {
            (0..shard_size)
                .map(|j| ((i * 137 + j * 31) & 0xFF) as u8)
                .collect()
        })
        .collect();

    {
        let (data_part, parity_part) = shards.split_at_mut(data_shards);
        let data_refs: Vec<&[u8]> = data_part.iter().map(|d| d.as_slice()).collect();
        let mut parity_refs: Vec<&mut [u8]> =
            parity_part.iter_mut().map(|p| p.as_mut_slice()).collect();
        encode::encode_with_tables16(data_shards, parity_shards, &data_refs, &mut parity_refs)
            .unwrap();
    }

    let erased = [0, 2, 5];
    let originals: Vec<(usize, Vec<u8>)> = erased.iter().map(|&i| (i, shards[i].clone())).collect();
    for &i in &erased {
        shards[i] = vec![0u8; shard_size];
    }

    let present: Vec<bool> = (0..total_shards).map(|i| !erased.contains(&i)).collect();
    let input_snapshots: Vec<Option<Vec<u8>>> = shards
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if !erased.contains(&i) {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect();
    let input_data: Vec<Option<&[u8]>> = input_snapshots.iter().map(|o| o.as_deref()).collect();
    let mut outputs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    for (i, original) in &originals {
        assert_eq!(
            outputs[*i],
            original.as_slice(),
            "recovered shard {i} should match original"
        );
    }
}

/// Verify the full decode pipeline with manual step-by-step computation.
#[test]
#[allow(clippy::needless_range_loop, clippy::vec_init_then_push)]
fn test_leopard_gf16_decode_pipeline_manual() {
    let data_shards = 3;
    let parity_shards = 2;
    let total_shards = data_shards + parity_shards;
    let shard_size = 64;

    // Use uniform data for easier debugging.
    let data: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| vec![(i + 1) as u8; shard_size])
        .collect();
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    // Verify: encode then decode roundtrip for shard 0.
    let original_0 = data[0].clone();
    let mut shards: Vec<Vec<u8>> = Vec::new();
    for i in 0..data_shards {
        if i == 0 {
            shards.push(vec![0u8; shard_size]);
        } else {
            shards.push(data[i].clone());
        }
    }
    for p in &parity {
        shards.push(p.clone());
    }

    let present: Vec<bool> = (0..total_shards).map(|i| i != 0).collect();
    let input_snapshots: Vec<Option<Vec<u8>>> = shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i != 0 { Some(s.clone()) } else { None })
        .collect();
    let input_data: Vec<Option<&[u8]>> = input_snapshots.iter().map(|o| o.as_deref()).collect();
    let mut outputs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    assert_eq!(
        outputs[0],
        original_0.as_slice(),
        "recovered shard 0 should match original"
    );
}

#[test]
fn test_leopard_gf16_reconstruct_simple_1plus1() {
    let data_shards = 1;
    let parity_shards = 1;
    let shard_size = 64;

    let data: Vec<Vec<u8>> = vec![(0..shard_size).map(|j| ((j + 1) & 0xFF) as u8).collect()];
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    let mut shards: Vec<Vec<u8>> = vec![
        vec![0u8; shard_size], // missing data shard 0
        parity[0].clone(),
    ];

    let original_0 = data[0].clone();

    let present: Vec<bool> = vec![false, true];
    let input_snapshots: Vec<Option<Vec<u8>>> = shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i != 0 { Some(s.clone()) } else { None })
        .collect();
    let input_data: Vec<Option<&[u8]>> = input_snapshots.iter().map(|o| o.as_deref()).collect();
    let mut outputs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    assert_eq!(
        outputs[0],
        original_0.as_slice(),
        "recovered shard 0 should match original"
    );
}

#[test]
fn test_leopard_gf16_encode_parity_values() {
    // Compare parity output with Go's klauspost/reedsolomon for 3+2 config.
    let data_shards = 3;
    let parity_shards = 2;
    let shard_size = 64;

    #[cfg(feature = "std")]
    {
        let tables = init_leopard_gf16_tables();

        // Print the fft_skew values used for encoding
        let m = 2usize;
        let skew = &tables.fft_skew[m - 1..]; // fft_skew[1..]
        println!("skew (fft_skew[1..]): {:?}", &skew[..8]);
        println!(
            "skew[1] = 0x{:04x} (used by first IFFT final_stage)",
            skew[1]
        );
        println!(
            "skew[3] = 0x{:04x} (used by second IFFT final_stage)",
            skew[3]
        );
    }

    let mut data: Vec<Vec<u8>> = Vec::new();
    for i in 0..data_shards {
        let shard: Vec<u8> = (0..shard_size)
            .map(|j| ((i * 64 + j) & 0xFF) as u8)
            .collect();
        data.push(shard);
    }
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    #[cfg(feature = "std")]
    println!("parity[0][:16] = {:?}", &parity[0][..16]);
    #[cfg(feature = "std")]
    println!("parity[1][:16] = {:?}", &parity[1][..16]);

    // Go reference parity values for the same input:
    let go_parity0: [u8; 16] = [
        59, 148, 199, 104, 147, 60, 111, 192, 227, 76, 31, 176, 75, 228, 183, 24,
    ];
    let go_parity1: [u8; 16] = [
        251, 85, 5, 171, 87, 249, 169, 7, 43, 133, 213, 123, 135, 41, 121, 215,
    ];

    assert_eq!(&parity[0][..16], &go_parity0, "parity[0] mismatch with Go");
    assert_eq!(&parity[1][..16], &go_parity1, "parity[1] mismatch with Go");
}

#[test]
fn test_leopard_gf16_encode_coefficients() {
    // Verify encode coefficients match Go's klauspost/reedsolomon.
    // For 3+2, the encoding matrix should be:
    //   parity[0] = data[0]*0x0003 + data[1]*0x0002 + data[2]*0x0005
    //   parity[1] = data[0]*0x0002 + data[1]*0x0003 + data[2]*0x0004
    // where + is XOR and * is GF(2^16) multiplication.
    let data_shards = 3usize;
    let parity_shards = 2usize;
    let shard_size = 64usize;

    // Expected coefficients from Go
    let expected: [[u16; 3]; 2] = [
        [0x0003, 0x0002, 0x0005], // parity[0]
        [0x0002, 0x0003, 0x0004], // parity[1]
    ];

    for data_idx in 0..data_shards {
        let mut data: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; data_shards];
        data[data_idx][0] = 1; // u16 LE = 0x0001
        let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];

        encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

        for p in 0..parity_shards {
            let got = parity[p][0] as u16 | ((parity[p][1] as u16) << 8);
            let exp = expected[p][data_idx];
            assert_eq!(
                got, exp,
                "coeff[data{data_idx}][parity{p}] = 0x{got:04x}, expected 0x{exp:04x}"
            );
        }
    }
}

/// Regression for the GF16 u16-reinterpret soundness fix: shard buffers that
/// start at an odd byte offset are 2-byte-misaligned for `u16`. The previous
/// implementation reinterpreted `&[u8]` as `&[u16]` via `from_raw_parts`
/// (undefined behaviour on such buffers). Encode + reconstruct over
/// odd-offset buffers must succeed and match the aligned result. Run under
/// `cargo +nightly miri test` to exercise the alignment guarantee directly.
#[test]
fn test_gf16_reconstruct_unaligned_odd_offset_buffers() {
    let data_shards = 4usize;
    let parity_shards = 2usize;
    let total_shards = data_shards + parity_shards;
    let shard_size = 128usize; // multiple of 64

    // Aligned reference encode.
    let data: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| {
            (0..shard_size)
                .map(|j| ((i * 31 + j * 17) & 0xFF) as u8)
                .collect()
        })
        .collect();
    let mut parity: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; parity_shards];
    encode::encode_with_tables16(data_shards, parity_shards, &data, &mut parity).unwrap();

    // All shards in order (data then parity).
    let mut all: Vec<Vec<u8>> = Vec::new();
    for d in &data {
        all.push(d.clone());
    }
    for p in &parity {
        all.push(p.clone());
    }

    // Back each shard with `shard_size + 1` bytes and use the sub-slice `[1..]`
    // so the data pointer is offset by one byte (odd address → u16-misaligned).
    let mut backing: Vec<Vec<u8>> = (0..total_shards)
        .map(|_| vec![0u8; shard_size + 1])
        .collect();
    for (i, b) in backing.iter_mut().enumerate() {
        b[1..].copy_from_slice(&all[i]);
    }

    // Lose data shard 0; inputs borrow the odd-offset sub-slices directly.
    let present: Vec<bool> = (0..total_shards).map(|i| i != 0).collect();
    let input_data: Vec<Option<&[u8]>> = backing
        .iter()
        .enumerate()
        .map(|(i, b)| if i != 0 { Some(&b[1..]) } else { None })
        .collect();

    // Odd-offset output buffers.
    let mut out_backing: Vec<Vec<u8>> = (0..total_shards)
        .map(|_| vec![0u8; shard_size + 1])
        .collect();
    let mut outputs: Vec<&mut [u8]> = out_backing.iter_mut().map(|b| &mut b[1..]).collect();

    decode::reconstruct_with_tables16(
        &present,
        &mut outputs,
        &input_data,
        data_shards,
        parity_shards,
        init_leopard_gf16_tables(),
    )
    .unwrap();

    // Recovered shard 0 must match the aligned original byte-for-byte.
    assert_eq!(
        &out_backing[0][1..],
        all[0].as_slice(),
        "odd-offset reconstruct mismatch for shard 0"
    );
}

#[test]
fn gfni_layout_affine_model_matches_scalar_for_all_basis_and_multipliers() {
    fn affine(value: u8, matrix: u64) -> u8 {
        let mut out = 0;
        for bit in 0..8 {
            let row = (matrix >> ((7 - bit) * 8)) as u8;
            out |= (((row & value).count_ones() & 1) as u8) << bit;
        }
        out
    }
    let t = super::init_leopard_gf16_tables();
    for log_m in 0..u16::MAX {
        let mut matrices = [0u64; 4];
        for bit in 0..16 {
            let product = super::ops::mul_log16(1u16 << bit, log_m, &t.log_lut, &t.exp_lut);
            for output_bit in 0..16 {
                let slot = (output_bit / 8) * 2 + bit / 8;
                matrices[slot] |=
                    (((product >> output_bit) & 1) as u64) << ((7 - output_bit % 8) * 8 + bit % 8);
            }
        }
        for bit in 0..16 {
            let value = 1u16 << bit;
            let lo = value as u8;
            let hi = (value >> 8) as u8;
            let got = u16::from(affine(lo, matrices[0]) ^ affine(hi, matrices[1]))
                | (u16::from(affine(lo, matrices[2]) ^ affine(hi, matrices[3])) << 8);
            let expected = super::ops::mul_log16(value, log_m, &t.log_lut, &t.exp_lut);
            assert_eq!(got, expected, "log={log_m} bit={bit}");
        }
    }
}
