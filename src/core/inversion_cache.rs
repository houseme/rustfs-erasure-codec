extern crate alloc;

use alloc::vec::Vec;

use core::hash::BuildHasher;
use hashlink::{LruCache, linked_hash_map::RawEntryMut};

use super::{Mutex, decode_matrix::DecodeMatrix};
use crate::Field;

pub(crate) const DEFAULT_INVERSION_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Snapshot of Classic inverse-cache storage retained by one codec.
///
/// `retained_bytes` accounts for dense or compact inverse values (including
/// inline storage), owned coefficient/index capacity and erasure-key capacity. It excludes the LRU's
/// bookkeeping, Arc counters, allocator overhead and matrices kept alive by
/// external reconstruction workspaces after eviction. The budget bounds this
/// accounted storage; peak live memory can include those additional owners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReconstructionCacheMemory {
    /// Number of retained inverse matrices.
    pub entries: usize,
    /// Bytes charged to retained matrix and key storage.
    pub retained_bytes: usize,
    /// Maximum retained matrix and key storage in bytes.
    pub max_bytes: usize,
}

#[derive(Debug)]
struct CachedMatrix<F: Field> {
    matrix: DecodeMatrix<F>,
    bytes: usize,
}

#[derive(Debug)]
pub(crate) struct InversionCache<F: Field> {
    hash_builder: hashlink::DefaultHashBuilder,
    state: Mutex<CacheState<F>>,
}

#[derive(Debug)]
struct CacheState<F: Field> {
    entries: LruCache<Vec<usize>, CachedMatrix<F>>,
    retained_bytes: usize,
    max_bytes: usize,
}

pub(crate) struct CacheInsertion {
    /// Includes replacement of an existing key, matching the insertion metric.
    pub(crate) inserted: bool,
    pub(crate) evictions: usize,
}

impl<F: Field> InversionCache<F> {
    pub(crate) fn new(capacity: usize, max_bytes: usize) -> Self {
        let hash_builder = hashlink::DefaultHashBuilder::default();
        Self {
            state: Mutex::new(CacheState {
                entries: LruCache::with_hasher(capacity, hash_builder.clone()),
                retained_bytes: 0,
                max_bytes,
            }),
            hash_builder,
        }
    }

    #[inline]
    pub(crate) fn hash_key(&self, key: &[usize]) -> u64 {
        self.hash_builder.hash_one(key)
    }

    #[cfg(test)]
    fn get(&self, key: &[usize]) -> Option<DecodeMatrix<F>> {
        self.get_hashed(self.hash_key(key), key)
    }

    #[inline]
    pub(crate) fn get_hashed(&self, hash: u64, key: &[usize]) -> Option<DecodeMatrix<F>> {
        let mut state = self.state.lock();
        match state
            .entries
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key)
        {
            RawEntryMut::Occupied(mut entry) => {
                entry.to_back();
                Some(entry.get().matrix.clone())
            }
            RawEntryMut::Vacant(_) => None,
        }
    }

    #[cfg(test)]
    fn insert(&self, key: Vec<usize>, matrix: DecodeMatrix<F>) -> CacheInsertion {
        self.insert_hashed(self.hash_key(&key), key, matrix)
    }

    pub(crate) fn insert_hashed(
        &self,
        hash: u64,
        key: Vec<usize>,
        matrix: DecodeMatrix<F>,
    ) -> CacheInsertion {
        // Reject oversized entries before changing recency or dropping a useful
        // working set. Saturated accounting must also reject arithmetic overflow.
        let bytes = key
            .capacity()
            .checked_mul(core::mem::size_of::<usize>())
            .and_then(|key_bytes| matrix.retained_bytes().checked_add(key_bytes));
        let mut state = self.state.lock();
        let Some(bytes) = bytes.filter(|&bytes| bytes <= state.max_bytes) else {
            return CacheInsertion {
                inserted: false,
                evictions: 0,
            };
        };
        if state.entries.capacity() == 0 {
            return CacheInsertion {
                inserted: false,
                evictions: 0,
            };
        }

        // Concurrent misses may build the same inverse outside the mutex.
        // Removing its previous charge makes replacement count exactly once.
        if let RawEntryMut::Occupied(entry) = state
            .entries
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key.as_slice())
        {
            let (_, previous) = entry.remove_entry();
            state.retained_bytes -= previous.bytes;
        }
        let mut evictions = 0;
        while state.entries.len() >= state.entries.capacity()
            || state.retained_bytes > state.max_bytes - bytes
        {
            let (_, removed) = state
                .entries
                .remove_lru()
                .expect("cache has an eviction candidate");
            state.retained_bytes -= removed.bytes;
            evictions += 1;
        }
        match state
            .entries
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key.as_slice())
        {
            RawEntryMut::Vacant(entry) => {
                entry.insert_hashed_nocheck(hash, key, CachedMatrix { matrix, bytes });
            }
            RawEntryMut::Occupied(_) => unreachable!("replacement removed the cached key"),
        }
        state.retained_bytes += bytes;
        CacheInsertion {
            inserted: true,
            evictions,
        }
    }

    pub(crate) fn memory(&self) -> ReconstructionCacheMemory {
        let state = self.state.lock();
        ReconstructionCacheMemory {
            entries: state.entries.len(),
            retained_bytes: state.retained_bytes,
            max_bytes: state.max_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{galois_8, matrix::Matrix};
    use alloc::vec;

    fn matrix(rows: usize) -> DecodeMatrix<galois_8::Field> {
        DecodeMatrix::dense(Matrix::new(rows, rows))
    }

    fn entry_bytes(matrix: &DecodeMatrix<galois_8::Field>, key: &Vec<usize>) -> usize {
        matrix.retained_bytes() + key.capacity() * core::mem::size_of::<usize>()
    }

    #[test]
    fn byte_budget_evicts_least_recently_used_and_preserves_exact_accounting() {
        let matrix = matrix(4);
        let bytes = entry_bytes(&matrix, &vec![0]);
        let cache = InversionCache::new(10, bytes * 2);
        assert!(cache.insert(vec![0], matrix.clone()).inserted);
        assert!(cache.insert(vec![1], matrix.clone()).inserted);
        assert!(cache.get(&[0]).is_some());
        let result = cache.insert(vec![2], matrix);
        assert_eq!(result.evictions, 1);
        assert!(cache.get(&[1]).is_none());
        assert!(cache.get(&[0]).is_some());
        assert!(cache.get(&[2]).is_some());
        assert_eq!(cache.memory().retained_bytes, bytes * 2);
        assert_eq!(cache.memory().entries, 2);
    }

    #[test]
    fn oversized_entries_preserve_small_cached_matrices() {
        let small = matrix(4);
        let bytes = entry_bytes(&small, &vec![0]);
        let cache = InversionCache::new(10, bytes);
        cache.insert(vec![0], small.clone());
        let result = cache.insert(vec![1], matrix(64));
        assert!(!result.inserted);
        assert_eq!(result.evictions, 0);
        assert!(cache.get(&[0]).unwrap().ptr_eq(&small));
        assert_eq!(cache.memory().retained_bytes, bytes);
    }

    #[test]
    fn replacement_counts_key_capacity_and_matrix_capacity_once() {
        let old = matrix(4);
        let new = matrix(64);
        let mut key = Vec::with_capacity(32);
        key.push(0);
        let new_bytes = entry_bytes(&new, &key);
        let cache = InversionCache::new(1, new_bytes);
        cache.insert(vec![0], old);
        let result = cache.insert(key, new.clone());
        assert!(result.inserted);
        assert_eq!(result.evictions, 0);
        assert_eq!(cache.memory().entries, 1);
        assert_eq!(cache.memory().retained_bytes, new_bytes);
        assert!(cache.get(&[0]).unwrap().ptr_eq(&new));
    }

    #[test]
    fn entry_capacity_still_evicts_when_the_byte_budget_allows_more() {
        let cache = InversionCache::new(1, usize::MAX);
        cache.insert(vec![0], matrix(4));
        assert_eq!(cache.insert(vec![1], matrix(4)).evictions, 1);
        assert_eq!(cache.memory().entries, 1);
        assert!(cache.get(&[0]).is_none());
    }

    #[test]
    fn compact_storage_charges_capacity_and_keeps_cloned_owners_alive_after_eviction() {
        let mut coefficients = Vec::with_capacity(256);
        coefficients.resize(64, 7u8);
        let compact = DecodeMatrix::<galois_8::Field>::compact(64, &[0], coefficients);
        let bytes = entry_bytes(&compact, &vec![0]);
        let mut smaller = Vec::with_capacity(64);
        smaller.resize(64, 7u8);
        let exact = DecodeMatrix::<galois_8::Field>::compact(64, &[0], smaller);
        assert_eq!(compact.retained_bytes() - exact.retained_bytes(), 192);
        assert!(compact.retained_bytes() < matrix(64).retained_bytes());

        let cache = InversionCache::new(1, bytes);
        cache.insert(vec![0], compact.clone());
        assert_eq!(cache.memory().retained_bytes, bytes);
        let replacement = DecodeMatrix::<galois_8::Field>::compact(64, &[1], vec![9; 64]);
        assert_eq!(cache.insert(vec![1], replacement).evictions, 1);
        assert!(cache.get(&[0]).is_none());
        assert_eq!(compact.missing_row(0), &[7; 64]);
        assert_eq!(compact.get(1, 0), 1);
        assert_eq!(compact.get(1, 1), 0);
        assert_eq!(cache.memory().retained_bytes, entry_bytes(&exact, &vec![1]));
    }

    #[test]
    fn borrowed_hashes_ignore_capacity_and_reject_colliding_distinct_keys() {
        let cache = InversionCache::new(8, usize::MAX);
        let mut owned = Vec::with_capacity(128);
        owned.extend([1, 3, 5]);
        let hash = cache.hash_key(&owned);
        assert_eq!(hash, cache.hash_key(&[1, 3, 5]));
        let value = matrix(4);
        assert!(cache.insert_hashed(hash, owned, value.clone()).inserted);
        assert!(cache.get_hashed(hash, &[1, 3, 5]).unwrap().ptr_eq(&value));
        for other in [&[1, 3][..], &[1, 3, 6][..], &[3, 1, 5][..], &[]] {
            assert!(cache.get_hashed(hash, other).is_none());
        }
        let zero = cache.hash_key(&[]);
        assert!(cache.insert_hashed(zero, vec![], value).inserted);
        assert!(cache.get_hashed(zero, &[]).is_some());
    }

    #[test]
    fn zero_limits_reject_insertions_without_retaining_entries() {
        for (capacity, budget) in [(0, usize::MAX), (8, 0)] {
            let cache = InversionCache::new(capacity, budget);
            let hash = cache.hash_key(&[0]);
            let insertion = cache.insert_hashed(hash, vec![0], matrix(4));
            assert!(!insertion.inserted);
            assert_eq!(insertion.evictions, 0);
            assert!(cache.get_hashed(hash, &[0]).is_none());
            assert_eq!(cache.memory().entries, 0);
            assert_eq!(cache.memory().retained_bytes, 0);
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn hashing_completes_while_another_thread_holds_the_cache_state() {
        let cache = InversionCache::<galois_8::Field>::new(8, 4096);
        let guard = cache.state.lock();
        let (sender, receiver) = std::sync::mpsc::channel();
        let completed = std::thread::scope(|scope| {
            scope.spawn(|| sender.send(cache.hash_key(&[0, 2, 4])).unwrap());
            let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
            drop(guard);
            result.is_ok()
        });
        assert!(completed, "hashing waited for the cache-state mutex");
    }

    #[cfg(feature = "std")]
    #[test]
    fn concurrent_prehashed_replacements_preserve_weighted_accounting() {
        let value = matrix(4);
        let bytes = value.retained_bytes() + 32 * core::mem::size_of::<usize>();
        let cache = InversionCache::new(8, bytes * 3);
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let cache = &cache;
                let value = &value;
                scope.spawn(move || {
                    for step in 0..100 {
                        let mut key = Vec::with_capacity(1 + (step % 32));
                        key.push((worker + step) % 16);
                        let identity = key[0];
                        let hash = cache.hash_key(&key);
                        cache.insert_hashed(hash, key, value.clone());
                        if let Some(found) = cache.get_hashed(hash, &[identity]) {
                            assert!(found.ptr_eq(value));
                        }
                        let snapshot = cache.memory();
                        assert!(snapshot.entries <= 8);
                        assert!(snapshot.retained_bytes <= snapshot.max_bytes);
                    }
                });
            }
        });
        let state = cache.state.lock();
        assert_eq!(
            state.retained_bytes,
            state.entries.iter().map(|(_, entry)| entry.bytes).sum()
        );
    }
}
