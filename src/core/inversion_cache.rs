extern crate alloc;

use alloc::vec::Vec;

use hashlink::LruCache;

use super::decode_matrix::DecodeMatrix;
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
        Self {
            entries: LruCache::new(capacity),
            retained_bytes: 0,
            max_bytes,
        }
    }

    pub(crate) fn get(&mut self, key: &[usize]) -> Option<&DecodeMatrix<F>> {
        self.entries.get(key).map(|entry| &entry.matrix)
    }

    pub(crate) fn insert(&mut self, key: Vec<usize>, matrix: DecodeMatrix<F>) -> CacheInsertion {
        // Reject oversized entries before changing recency or dropping a useful
        // working set. Saturated accounting must also reject arithmetic overflow.
        let bytes = key
            .capacity()
            .checked_mul(core::mem::size_of::<usize>())
            .and_then(|key_bytes| matrix.retained_bytes().checked_add(key_bytes));
        let Some(bytes) = bytes.filter(|&bytes| bytes <= self.max_bytes) else {
            return CacheInsertion {
                inserted: false,
                evictions: 0,
            };
        };
        if self.capacity() == 0 {
            return CacheInsertion {
                inserted: false,
                evictions: 0,
            };
        }

        // Concurrent misses may build the same inverse outside the mutex.
        // Removing its previous charge makes replacement count exactly once.
        if let Some(previous) = self.entries.remove(key.as_slice()) {
            self.retained_bytes -= previous.bytes;
        }
        let mut evictions = 0;
        while self.entries.len() >= self.capacity() || self.retained_bytes > self.max_bytes - bytes
        {
            let (_, removed) = self
                .entries
                .remove_lru()
                .expect("cache has an eviction candidate");
            self.retained_bytes -= removed.bytes;
            evictions += 1;
        }
        self.retained_bytes += bytes;
        self.entries.insert(key, CachedMatrix { matrix, bytes });
        CacheInsertion {
            inserted: true,
            evictions,
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub(crate) fn memory(&self) -> ReconstructionCacheMemory {
        ReconstructionCacheMemory {
            entries: self.entries.len(),
            retained_bytes: self.retained_bytes,
            max_bytes: self.max_bytes,
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
        let mut cache = InversionCache::new(10, bytes * 2);
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
        let mut cache = InversionCache::new(10, bytes);
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
        let mut cache = InversionCache::new(1, new_bytes);
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
        let mut cache = InversionCache::new(1, usize::MAX);
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

        let mut cache = InversionCache::new(1, bytes);
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
}
