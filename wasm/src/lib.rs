use std::{cell::RefCell, rc::Rc};

use rustfs_erasure_codec::{CodecOptions, Error, galois_8::ReedSolomon};
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

// Each WASM instance owns its linear memory and this cache. Native callers get
// one cache per thread. Retained inverse storage is bounded independently for
// each of the two layouts; active Rc owners can outlive cache eviction.
const INVERSION_CACHE_ENTRIES: usize = 64;
const INVERSION_CACHE_BYTES: usize = 256 * 1024;

struct CodecCache {
    // Most recently used layout first.
    entries: [Option<Rc<ReedSolomon>>; 2],
}

impl CodecCache {
    fn get(&mut self, data: usize, parity: usize) -> Option<Rc<ReedSolomon>> {
        let index = self.entries.iter().position(|entry| {
            entry.as_ref().is_some_and(|codec| {
                codec.data_shard_count() == data && codec.parity_shard_count() == parity
            })
        })?;
        self.entries.swap(0, index);
        self.entries[0].clone()
    }

    fn insert(&mut self, codec: Rc<ReedSolomon>) {
        self.entries[1] = self.entries[0].take();
        self.entries[0] = Some(codec);
    }
}

thread_local! {
    static CODEC_CACHE: RefCell<CodecCache> = const {
        RefCell::new(CodecCache { entries: [None, None] })
    };
}

fn codec_for(data_shards: usize, parity_shards: usize) -> Result<Rc<ReedSolomon>, Error> {
    if let Some(codec) =
        CODEC_CACHE.with(|cache| cache.borrow_mut().get(data_shards, parity_shards))
    {
        return Ok(codec);
    }
    // Release the RefCell borrow before construction and before returning a
    // codec to the operation. Caller-owned slices never enter the cache.
    let codec = Rc::new(ReedSolomon::with_options(
        data_shards,
        parity_shards,
        CodecOptions::builder()
            .inversion_cache_capacity(INVERSION_CACHE_ENTRIES)
            .inversion_cache_max_bytes(INVERSION_CACHE_BYTES)
            .build(),
    )?);
    CODEC_CACHE.with(|cache| cache.borrow_mut().insert(Rc::clone(&codec)));
    Ok(codec)
}

fn validate_shard_size(
    buffer_len: usize,
    data_shards: usize,
    parity_shards: usize,
) -> Result<usize, Error> {
    // Preserve constructor error precedence while rejecting malformed requests
    // before constructing a matrix or changing the retained layouts.
    if data_shards == 0 {
        return Err(Error::TooFewDataShards);
    }
    if parity_shards == 0 {
        return Err(Error::TooFewParityShards);
    }
    let total_shards = data_shards
        .checked_add(parity_shards)
        .ok_or(Error::TooManyShards)?;
    if total_shards > 256 {
        return Err(Error::TooManyShards);
    }
    if buffer_len == 0 {
        return Err(Error::EmptyShard);
    }
    if buffer_len < total_shards || !buffer_len.is_multiple_of(total_shards) {
        return Err(Error::IncorrectShardSize);
    }
    Ok(buffer_len / total_shards)
}

#[wasm_bindgen]
pub fn encode(shards: &mut [u8], data_shards: usize, parity_shards: usize) -> u8 {
    let shard_size = match validate_shard_size(shards.len(), data_shards, parity_shards) {
        Ok(validated) => validated,
        Err(error) => return result_to_number(Err(error)),
    };
    let reed_solomon = match codec_for(data_shards, parity_shards) {
        Ok(codec) => codec,
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
    let shard_size = match validate_shard_size(shards.len(), data_shards, parity_shards) {
        Ok(validated) => validated,
        Err(error) => return result_to_number(Err(error)),
    };
    if shards_available.len() != data_shards + parity_shards
        || shards_available.iter().any(|&flag| flag > 1)
    {
        return RESULT_ERROR_INVALID_SHARD_FLAGS;
    }
    let reed_solomon = match codec_for(data_shards, parity_shards) {
        Ok(codec) => codec,
        Err(error) => return result_to_number(Err(error)),
    };

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

    fn clear_cache() {
        CODEC_CACHE.with(|cache| cache.borrow_mut().entries = [None, None]);
    }

    fn cached_codecs() -> Vec<Rc<ReedSolomon>> {
        CODEC_CACHE.with(|cache| cache.borrow().entries.iter().flatten().cloned().collect())
    }

    fn encoded(data: usize, parity: usize, len: usize) -> Vec<u8> {
        let mut shards: Vec<_> = (0..(data + parity) * len)
            .map(|index| (index * 31 + index / 17) as u8)
            .collect();
        assert_eq!(encode(&mut shards, data, parity), RESULT_OK);
        shards
    }

    #[test]
    fn repeated_public_calls_reuse_codec_and_inverse_across_shard_lengths() {
        clear_cache();
        let _ = encoded(3, 2, 1);
        let first = cached_codecs().remove(0);
        assert_eq!(first.reconstruction_cache_memory().entries, 0);
        for len in [1, 17, 65, 4097] {
            let mut shards = encoded(3, 2, len);
            assert!(Rc::ptr_eq(&first, &cached_codecs()[0]));
            let original = shards.clone();
            shards[..len].fill(0);
            assert_eq!(reconstruct(&mut shards, 3, 2, &[0, 1, 1, 1, 1]), RESULT_OK);
            assert_eq!(shards, original);
            assert!(Rc::ptr_eq(&first, &cached_codecs()[0]));
            assert_eq!(first.reconstruction_cache_memory().entries, 1);
        }
    }

    #[test]
    fn alternating_layouts_reuse_both_entries_and_evict_the_least_recently_used() {
        clear_cache();
        let _ = encoded(3, 2, 17);
        let first = cached_codecs().remove(0);
        let _ = encoded(4, 2, 17);
        let second = cached_codecs().remove(0);
        for _ in 0..8 {
            let _ = encoded(3, 2, 17);
            assert!(Rc::ptr_eq(&first, &cached_codecs()[0]));
            let _ = encoded(4, 2, 17);
            assert!(Rc::ptr_eq(&second, &cached_codecs()[0]));
        }
        let _ = encoded(3, 2, 17);
        let _ = encoded(1, 1, 17);
        let cached = cached_codecs();
        assert_eq!(cached.len(), 2);
        assert!(Rc::ptr_eq(&first, &cached[1]));
        assert!(!cached.iter().any(|codec| Rc::ptr_eq(codec, &second)));
        let _ = encoded(4, 2, 17);
        assert!(!Rc::ptr_eq(&second, &cached_codecs()[0]));
    }

    #[test]
    fn invalid_requests_preserve_warm_layouts_and_error_precedence() {
        clear_cache();
        let _ = encoded(3, 2, 1);
        let _ = encoded(4, 2, 1);
        let before = cached_codecs();
        invalid_counts_return_errors_without_modifying_input();
        invalid_buffer_lengths_return_errors_without_modifying_input();
        invalid_availability_flags_return_errors_without_modifying_input();
        assert_eq!(
            reconstruct(&mut [], 0, 0, &[]),
            RESULT_ERROR_TOO_FEW_DATA_SHARDS
        );
        assert_eq!(
            reconstruct(&mut [], 1, 0, &[]),
            RESULT_ERROR_TOO_FEW_PARITY_SHARDS
        );
        assert_eq!(reconstruct(&mut [], 2, 1, &[]), RESULT_ERROR_EMPTY_SHARD);
        assert_eq!(
            reconstruct(&mut [7; 5], 2, 1, &[]),
            RESULT_ERROR_INCORRECT_SHARD_SIZE
        );
        let after = cached_codecs();
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(after) {
            assert!(Rc::ptr_eq(before, &after));
        }
    }

    #[test]
    fn retained_inverse_storage_respects_the_wasm_budget() {
        clear_cache();
        let original = encoded(64, 4, 1);
        let codec = cached_codecs().remove(0);
        assert_eq!(codec.inversion_cache_capacity(), INVERSION_CACHE_ENTRIES);
        assert_eq!(codec.inversion_cache_max_bytes(), INVERSION_CACHE_BYTES);
        for missing in 0..64 {
            let mut shards = original.clone();
            shards[missing] = 0;
            let mut flags = [1; 68];
            flags[missing] = 0;
            assert_eq!(reconstruct(&mut shards, 64, 4, &flags), RESULT_OK);
            assert_eq!(shards, original);
            let memory = codec.reconstruction_cache_memory();
            assert!(memory.entries <= INVERSION_CACHE_ENTRIES);
            assert!(memory.retained_bytes <= INVERSION_CACHE_BYTES);
        }
        let memory = codec.reconstruction_cache_memory();
        // Compact rows let all 64 sparse patterns fit within the byte budget.
        assert_eq!(memory.entries, INVERSION_CACHE_ENTRIES);
    }

    #[test]
    fn active_codec_owners_survive_nested_entry_calls_and_eviction() {
        clear_cache();
        let original = encoded(3, 2, 17);
        let active = cached_codecs().remove(0);
        let _ = encoded(4, 2, 17);
        let _ = encoded(1, 1, 17);
        assert!(
            !cached_codecs()
                .iter()
                .any(|codec| Rc::ptr_eq(codec, &active))
        );
        let shards = original.as_chunks::<17>().0;
        assert!(active.verify(shards).unwrap());
        let _ = encoded(3, 2, 17);
        assert!(!Rc::ptr_eq(&active, &cached_codecs()[0]));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn native_threads_own_independent_codec_caches() {
        clear_cache();
        let _ = encoded(3, 2, 17);
        let current = cached_codecs().remove(0);
        std::thread::spawn(|| {
            assert!(cached_codecs().is_empty());
            let _ = encoded(4, 2, 17);
            assert_eq!(cached_codecs()[0].data_shard_count(), 4);
        })
        .join()
        .unwrap();
        assert!(Rc::ptr_eq(&current, &cached_codecs()[0]));
        assert_eq!(cached_codecs().len(), 1);
    }

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
