use rustfs_erasure_codec::{Error, galois_8::ReedSolomon};
use wasm_bindgen::prelude::*;

pub const RESULT_OK: u8 = 0;
pub const RESULT_ERROR_TOO_FEW_SHARDS: u8 = 1;
pub const RESULT_ERROR_TOO_MANY_SHARDS: u8 = 2;
pub const RESULT_ERROR_TOO_FEW_DATA_SHARDS: u8 = 3;
pub const RESULT_ERROR_TOO_MANY_DATA_SHARDS: u8 = 4;
pub const RESULT_ERROR_TOO_FEW_PARITY_SHARDS: u8 = 5;
pub const RESULT_ERROR_TOO_MANY_PARITY_SHARDS: u8 = 6;
pub const RESULT_ERROR_TOO_FEW_BUFFER_SHARDS: u8 = 7;
pub const RESULT_ERROR_TOO_MANY_BUFFER_SHARDS: u8 = 8;
pub const RESULT_ERROR_INCORRECT_SHARD_SIZE: u8 = 9;
pub const RESULT_ERROR_TOO_FEW_SHARDS_PRESENT: u8 = 10;
pub const RESULT_ERROR_EMPTY_SHARD: u8 = 11;
pub const RESULT_ERROR_INVALID_SHARD_FLAGS: u8 = 12;
pub const RESULT_ERROR_INVALID_INDEX: u8 = 13;
pub const RESULT_ERROR_INVALID_CUSTOM_MATRIX: u8 = 14;
pub const RESULT_ERROR_UNSUPPORTED_CODEC_FAMILY: u8 = 15;
pub const RESULT_ERROR_UNSUPPORTED_LEOPARD_PROTOTYPE: u8 = 16;

fn result_to_number(result: Result<(), Error>) -> u8 {
    match result {
        Ok(()) => RESULT_OK,
        Err(Error::TooFewShards) => RESULT_ERROR_TOO_FEW_SHARDS,
        Err(Error::TooManyShards) => RESULT_ERROR_TOO_MANY_SHARDS,
        Err(Error::TooFewDataShards) => RESULT_ERROR_TOO_FEW_DATA_SHARDS,
        Err(Error::TooManyDataShards) => RESULT_ERROR_TOO_MANY_DATA_SHARDS,
        Err(Error::TooFewParityShards) => RESULT_ERROR_TOO_FEW_PARITY_SHARDS,
        Err(Error::TooManyParityShards) => RESULT_ERROR_TOO_MANY_PARITY_SHARDS,
        Err(Error::TooFewBufferShards) => RESULT_ERROR_TOO_FEW_BUFFER_SHARDS,
        Err(Error::TooManyBufferShards) => RESULT_ERROR_TOO_MANY_BUFFER_SHARDS,
        Err(Error::IncorrectShardSize) => RESULT_ERROR_INCORRECT_SHARD_SIZE,
        Err(Error::TooFewShardsPresent) => RESULT_ERROR_TOO_FEW_SHARDS_PRESENT,
        Err(Error::EmptyShard) => RESULT_ERROR_EMPTY_SHARD,
        Err(Error::InvalidShardFlags) => RESULT_ERROR_INVALID_SHARD_FLAGS,
        Err(Error::InvalidIndex) => RESULT_ERROR_INVALID_INDEX,
        Err(Error::InvalidCustomMatrix) => RESULT_ERROR_INVALID_CUSTOM_MATRIX,
        Err(Error::UnsupportedCodecFamily) => RESULT_ERROR_UNSUPPORTED_CODEC_FAMILY,
        Err(Error::UnsupportedLeopardPrototype) => RESULT_ERROR_UNSUPPORTED_LEOPARD_PROTOTYPE,
    }
}

fn codec_and_shard_size(
    buffer_len: usize,
    data_shards: usize,
    parity_shards: usize,
) -> Result<(ReedSolomon, usize), Error> {
    // The constructor validates zero counts, the field limit, and addition overflow.
    let codec = ReedSolomon::new(data_shards, parity_shards)?;
    let total_shards = codec.total_shard_count();
    if buffer_len == 0 {
        return Err(Error::EmptyShard);
    }
    if buffer_len < total_shards || !buffer_len.is_multiple_of(total_shards) {
        return Err(Error::IncorrectShardSize);
    }
    Ok((codec, buffer_len / total_shards))
}

#[wasm_bindgen]
pub fn encode(shards: &mut [u8], data_shards: usize, parity_shards: usize) -> u8 {
    let (reed_solomon, shard_size) =
        match codec_and_shard_size(shards.len(), data_shards, parity_shards) {
            Ok(validated) => validated,
            Err(error) => return result_to_number(Err(error)),
        };

    let mut separate_slice_shards: Vec<_> = shards.chunks_exact_mut(shard_size).collect();

    result_to_number(reed_solomon.encode(separate_slice_shards.as_mut_slice()))
}

#[wasm_bindgen]
pub fn reconstruct(
    shards: &mut [u8],
    data_shards: usize,
    parity_shards: usize,
    shards_available: &[u8],
) -> u8 {
    let (reed_solomon, shard_size) =
        match codec_and_shard_size(shards.len(), data_shards, parity_shards) {
            Ok(validated) => validated,
            Err(error) => return result_to_number(Err(error)),
        };
    if shards_available.len() != reed_solomon.total_shard_count()
        || shards_available.iter().any(|&flag| flag > 1)
    {
        return RESULT_ERROR_INVALID_SHARD_FLAGS;
    }

    let shards_available_slice_iter = shards_available.iter().map(|&num| num == 1u8);

    let mut separate_slice_shards: Vec<_> = shards
        .chunks_exact_mut(shard_size)
        .zip(shards_available_slice_iter)
        .collect();

    result_to_number(reed_solomon.reconstruct_data(&mut separate_slice_shards))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_reconstruct_restore_missing_data() {
        let mut shards = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(encode(&mut shards, 3, 2), RESULT_OK);
        let encoded = shards.clone();
        shards[..8].fill(0);
        assert_eq!(reconstruct(&mut shards, 3, 2, &[0, 0, 1, 1, 1]), RESULT_OK);
        assert_eq!(shards, encoded);
    }

    #[test]
    fn reconstruct_leaves_missing_parity_untouched() {
        let mut shards = vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0];
        assert_eq!(encode(&mut shards, 2, 1), RESULT_OK);
        shards[8..].fill(0);
        assert_eq!(reconstruct(&mut shards, 2, 1, &[1, 1, 0]), RESULT_OK);
        assert_eq!(&shards[8..], &[0; 4]);
    }

    #[test]
    fn invalid_counts_return_errors_without_modifying_input() {
        for (data, parity, expected) in [
            (0, 0, RESULT_ERROR_TOO_FEW_DATA_SHARDS),
            (0, 1, RESULT_ERROR_TOO_FEW_DATA_SHARDS),
            (1, 0, RESULT_ERROR_TOO_FEW_PARITY_SHARDS),
            (256, 1, RESULT_ERROR_TOO_MANY_SHARDS),
            (1, usize::MAX, RESULT_ERROR_TOO_MANY_SHARDS),
            (usize::MAX, 1, RESULT_ERROR_TOO_MANY_SHARDS),
        ] {
            let mut shards = [7; 12];
            assert_eq!(encode(&mut shards, data, parity), expected);
            assert_eq!(reconstruct(&mut shards, data, parity, &[]), expected);
            assert_eq!(shards, [7; 12]);
        }
    }

    #[test]
    fn invalid_buffer_lengths_return_errors_without_modifying_input() {
        for len in [0, 1, 2, 4, 5, 7] {
            let mut shards = vec![7; len];
            let expected = if len == 0 {
                RESULT_ERROR_EMPTY_SHARD
            } else {
                RESULT_ERROR_INCORRECT_SHARD_SIZE
            };
            assert_eq!(encode(&mut shards, 2, 1), expected);
            assert_eq!(reconstruct(&mut shards, 2, 1, &[1, 1, 1]), expected);
            assert_eq!(shards, vec![7; len]);
        }
    }

    #[test]
    fn invalid_availability_flags_return_errors_without_modifying_input() {
        for flags in [&[][..], &[1, 1], &[1, 1, 1, 1], &[1, 2, 1], &[255, 1, 1]] {
            let mut shards = [7; 12];
            assert_eq!(
                reconstruct(&mut shards, 2, 1, flags),
                RESULT_ERROR_INVALID_SHARD_FLAGS
            );
            assert_eq!(shards, [7; 12]);
        }
    }

    #[test]
    fn insufficient_present_shards_return_an_error() {
        assert_eq!(
            reconstruct(&mut [7; 12], 2, 1, &[0, 0, 1]),
            RESULT_ERROR_TOO_FEW_SHARDS_PRESENT
        );
    }

    #[test]
    fn unsupported_configuration_errors_have_stable_codes() {
        assert_eq!(result_to_number(Err(Error::InvalidCustomMatrix)), 14);
        assert_eq!(result_to_number(Err(Error::UnsupportedCodecFamily)), 15);
        assert_eq!(
            result_to_number(Err(Error::UnsupportedLeopardPrototype)),
            16
        );
    }
}
