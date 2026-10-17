//! Bounded contract checks shared by libFuzzer and deterministic integration tests.
//! This module is test support only; it is not part of the library's runtime API.

use galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, MatrixMode, galois_8};

fn byte(input: &[u8], index: usize) -> u8 {
    input.get(index).copied().unwrap_or(0)
}

fn coefficients(codec: &ReedSolomon) -> Vec<Vec<u8>> {
    let k = codec.data_shard_count();
    let mut basis = vec![vec![0; k]; k];
    for (index, row) in basis.iter_mut().enumerate() {
        row[index] = 1;
    }
    let mut rows = vec![vec![0; k]; codec.parity_shard_count()];
    codec.encode_sep(&basis, &mut rows).unwrap();
    rows
}

fn make_codec(k: usize, p: usize, options: CodecOptions) -> Result<ReedSolomon, Error> {
    if options.codec_family == CodecFamily::Classic && options.matrix_mode == MatrixMode::Custom {
        // Derive a known MDS matrix by encoding basis vectors through Cauchy.
        // Single-parity fast mode derives its required all-ones row instead.
        let reference = ReedSolomon::with_options(
            k,
            p,
            CodecOptions::builder()
                .matrix_mode(MatrixMode::Cauchy)
                .fast_one_parity(options.fast_one_parity)
                .build(),
        )?;
        ReedSolomon::with_custom_matrix(k, p, &coefficients(&reference), options)
    } else {
        ReedSolomon::with_options(k, p, options)
    }
}

fn encode(codec: &ReedSolomon, shards: &mut [Vec<u8>], api: u8) -> Result<(), Error> {
    match api {
        0 => codec.encode(shards),
        2 => codec.encode_opt(shards),
        1 | 3 => {
            let split = codec.data_shard_count().min(shards.len());
            let (data, parity) = shards.split_at_mut(split);
            if api == 1 {
                codec.encode_sep(&*data, parity)
            } else {
                codec.encode_sep_opt(&*data, parity)
            }
        }
        _ => unreachable!(),
    }
}

fn recover(
    codec: &ReedSolomon,
    shards: &mut [Option<Vec<u8>>],
    required: &[bool],
    api: u8,
) -> Result<(), Error> {
    match api {
        0 => codec.reconstruct(shards),
        1 => codec.reconstruct_data(shards),
        2 => codec.reconstruct_some(shards, required),
        3 => codec.reconstruct_opt(shards),
        4 => codec.reconstruct_data_opt(shards),
        5 => codec.reconstruct_some_opt(shards, required),
        6 => {
            let workspace = codec.prepare_reconstruct_opt_workspace(shards)?;
            codec.reconstruct_opt_with_workspace(shards, &workspace)
        }
        _ => unreachable!(),
    }
}

fn assert_recovery_error(
    codec: &ReedSolomon,
    mut shards: Vec<Option<Vec<u8>>>,
    required: &[bool],
    api: u8,
    expected: Error,
) {
    let expected = if api == 6
        && codec.codec_family() != CodecFamily::Classic
        && shards.len() == codec.total_shard_count()
    {
        Error::UnsupportedCodecFamily
    } else {
        expected
    };
    let before = shards.clone();
    assert_eq!(recover(codec, &mut shards, required, api), Err(expected));
    assert_eq!(shards, before, "invalid recovery must not publish data");
}

#[allow(clippy::too_many_arguments)]
fn malformed_contract(
    codec: &ReedSolomon,
    full: &[Vec<u8>],
    missing: &[Option<Vec<u8>>],
    required: &[bool],
    encode_api: u8,
    recovery_api: u8,
    scenario: u8,
    selector: usize,
) {
    let total = full.len();
    match scenario {
        0 => {}
        1..=5 => {
            let mut malformed = full.to_vec();
            let index = selector % total;
            let len = malformed[index].len();
            let (encode_error, recovery_error) = match scenario {
                1 | 2 => {
                    let changed_len = if scenario == 1 && len > 1 {
                        len - 1
                    } else {
                        len + 1
                    };
                    malformed[index].resize(changed_len, 0);
                    (Error::IncorrectShardSize, Error::IncorrectShardSize)
                }
                3 => {
                    for shard in &mut malformed {
                        shard.clear();
                    }
                    (Error::EmptyShard, Error::EmptyShard)
                }
                4 => {
                    malformed.pop();
                    let error = if encode_api == 1 || encode_api == 3 {
                        Error::TooFewParityShards
                    } else {
                        Error::TooFewShards
                    };
                    (error, Error::TooFewShards)
                }
                5 => {
                    malformed.push(full[0].clone());
                    let error = if encode_api == 1 || encode_api == 3 {
                        Error::TooManyParityShards
                    } else {
                        Error::TooManyShards
                    };
                    (error, Error::TooManyShards)
                }
                _ => unreachable!(),
            };
            let before = malformed.clone();
            assert_eq!(encode(codec, &mut malformed, encode_api), Err(encode_error));
            assert_eq!(malformed, before, "invalid encode must not change buffers");
            assert_recovery_error(
                codec,
                before.into_iter().map(Some).collect(),
                required,
                recovery_api,
                recovery_error,
            );
        }
        6 => {
            let mut unavailable: Vec<_> = full.iter().cloned().map(Some).collect();
            for shard in unavailable.iter_mut().take(codec.parity_shard_count() + 1) {
                *shard = None;
            }
            assert_recovery_error(
                codec,
                unavailable,
                required,
                recovery_api,
                Error::TooFewShardsPresent,
            );
        }
        7 => {
            let flags = vec![true; total + 1];
            for api in [2, 5] {
                assert_recovery_error(
                    codec,
                    missing.to_vec(),
                    &flags,
                    api,
                    Error::InvalidShardFlags,
                );
            }
        }
        8 | 9 => {
            let workspace = codec.prepare_reconstruct_opt_workspace(missing);
            if codec.codec_family() != CodecFamily::Classic {
                assert!(matches!(workspace, Err(Error::UnsupportedCodecFamily)));
                // A custom matrix cannot be assigned to a Leopard codec.
                let options = CodecOptions::builder()
                    .codec_family(codec.codec_family())
                    .build();
                assert_eq!(
                    ReedSolomon::with_custom_matrix(
                        codec.data_shard_count(),
                        codec.parity_shard_count(),
                        &[],
                        options
                    )
                    .unwrap_err(),
                    Error::UnsupportedCodecFamily
                );
                return;
            }
            let workspace = workspace.unwrap();
            if scenario == 8 {
                // Nonzero row scaling preserves the reference matrix's MDS
                // property and guarantees a different effective matrix.
                let mut rows = coefficients(codec);
                for coefficient in &mut rows[0] {
                    *coefficient = galois_8::mul(*coefficient, 2);
                }
                let other = ReedSolomon::with_custom_matrix(
                    codec.data_shard_count(),
                    codec.parity_shard_count(),
                    &rows,
                    CodecOptions::default(),
                )
                .unwrap();
                assert_ne!(&other, codec);
                let mut candidate = missing.to_vec();
                assert_eq!(
                    other.reconstruct_opt_with_workspace(&mut candidate, &workspace),
                    Err(Error::InvalidCustomMatrix)
                );
                assert_eq!(candidate, missing);
            } else {
                let mut resized = missing.to_vec();
                for shard in resized.iter_mut().flatten() {
                    shard.push(0);
                }
                let before = resized.clone();
                assert_eq!(
                    codec.reconstruct_opt_with_workspace(&mut resized, &workspace),
                    Err(Error::IncorrectShardSize)
                );
                assert_eq!(resized, before);
                let mut changed = missing.to_vec();
                let index = selector % total;
                changed[index] = if changed[index].is_some() {
                    None
                } else {
                    Some(full[index].clone())
                };
                let before = changed.clone();
                assert_eq!(
                    codec.reconstruct_opt_with_workspace(&mut changed, &workspace),
                    Err(Error::InvalidShardFlags)
                );
                assert_eq!(changed, before);
                assert_eq!(
                    ReedSolomon::with_custom_matrix(
                        codec.data_shard_count(),
                        codec.parity_shard_count(),
                        &[],
                        CodecOptions::default()
                    )
                    .unwrap_err(),
                    Error::InvalidCustomMatrix
                );
            }
        }
        _ => unreachable!(),
    }
}

/// Map every byte string, including the empty string, to one bounded contract
/// case. The 16-byte header is documented in `fuzz/README.md`; missing bytes are
/// zero. No payload length gate or attacker-controlled repetition count exists.
pub fn exercise_codec_contracts(input: &[u8]) {
    let family = [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ][usize::from(byte(input, 0) % 3)];
    let matrix = [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ][usize::from(byte(input, 1) % 4)];
    let k = usize::from(byte(input, 3) % 8) + 1;
    let p = usize::from(byte(input, 4) % 4) + 1;
    let options = CodecOptions::builder()
        .codec_family(family)
        .matrix_mode(matrix)
        .fast_one_parity(byte(input, 2) & 1 != 0)
        .max_parallel_jobs(1)
        .build();
    let codec = make_codec(k, p, options);
    if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
        assert_eq!(codec.unwrap_err(), Error::UnsupportedCodecFamily);
        return;
    }
    let codec = codec.unwrap();
    assert_eq!(codec.codec_family(), family);
    let len = if family == CodecFamily::Classic {
        [1, 63, 64, 65, 128, 192][usize::from(byte(input, 5) % 6)]
    } else {
        64 * (usize::from(byte(input, 5) % 3) + 1)
    };
    let payload = input.get(16..).unwrap_or_default();
    let mut full: Vec<Vec<u8>> = (0..k + p)
        .map(|index| {
            (0..len)
                .map(|offset| {
                    let source = payload
                        .get((index * len + offset) % payload.len().max(1))
                        .copied()
                        .unwrap_or(0);
                    source.wrapping_add(byte(input, 15).wrapping_mul((index + offset + 1) as u8))
                })
                .collect()
        })
        .collect();
    let mut reference = full.clone();
    codec.encode(&mut reference).unwrap();
    let encode_api = byte(input, 6) % 4;
    encode(&codec, &mut full, encode_api).unwrap();
    assert_eq!(full, reference);
    assert!(codec.verify(&full).unwrap());
    assert!(codec.verify_opt(&full).unwrap());

    let count = usize::from(byte(input, 8)) % (p + 1);
    let start = usize::from(byte(input, 9)) % (k + p);
    let mut missing: Vec<_> = full.iter().cloned().map(Some).collect();
    // Resolve repeated stride positions by advancing to the next present
    // shard. At most k+p <= 12 positions are inspected for each erasure.
    let stride = usize::from(byte(input, 14)) + 1;
    for offset in 0..count {
        let mut index = (start + offset * stride) % (k + p);
        while missing[index].is_none() {
            index = (index + 1) % (k + p);
        }
        missing[index] = None;
    }
    let mask = u16::from_le_bytes([byte(input, 10), byte(input, 11)]);
    let required_len = if byte(input, 12) & 1 == 0 { k } else { k + p };
    let required: Vec<_> = (0..required_len)
        .map(|index| mask & (1 << index) != 0)
        .collect();
    let recovery_api = byte(input, 7) % 7;
    let mut candidate = missing.clone();
    let result = recover(&codec, &mut candidate, &required, recovery_api);
    if recovery_api == 6 && family != CodecFamily::Classic {
        assert_eq!(result, Err(Error::UnsupportedCodecFamily));
        assert_eq!(candidate, missing);
    } else {
        result.unwrap();
        for index in 0..k + p {
            let requested = match recovery_api {
                1 | 4 => index < k,
                2 | 5 => required.get(index).copied().unwrap_or(false),
                _ => true,
            };
            if missing[index].is_some() || requested {
                assert_eq!(
                    candidate[index].as_ref(),
                    Some(&full[index]),
                    "recovered shard {index}"
                );
            } else {
                assert!(
                    candidate[index].is_none(),
                    "unrequested shard {index} was published"
                );
            }
        }
    }
    malformed_contract(
        &codec,
        &full,
        &missing,
        &required,
        encode_api,
        recovery_api,
        byte(input, 13) % 10,
        usize::from(byte(input, 14)),
    );
}
