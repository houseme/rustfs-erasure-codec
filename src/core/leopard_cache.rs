/// Two owned resources, most recently returned first, with one total heap budget.
/// Taking a resource releases the cache borrow before computation or callbacks.
pub(super) struct TwoEntryCache<K, T> {
    entries: [Option<Entry<K, T>>; 2],
    max_bytes: usize,
}

struct Entry<K, T> {
    key: K,
    value: T,
    bytes: usize,
}

impl<K: PartialEq, T> TwoEntryCache<K, T> {
    pub(super) const fn new(max_bytes: usize) -> Self {
        Self {
            entries: [None, None],
            max_bytes,
        }
    }

    pub(super) fn take(&mut self, key: &K) -> Option<T> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|entry| entry.key == *key))?;
        self.entries[index].take().map(|entry| entry.value)
    }

    pub(super) fn insert(&mut self, key: K, value: T, bytes: usize) {
        if bytes > self.max_bytes {
            return;
        }
        let previous = core::mem::take(&mut self.entries);
        // A nested operation may have returned the same key while this resource
        // was checked out. Replace it, and retain the newest distinct entry that
        // fits the remaining budget. Cache metadata itself never allocates.
        let retained = previous
            .into_iter()
            .flatten()
            .find(|entry| entry.key != key && entry.bytes <= self.max_bytes - bytes);
        self.entries = [Some(Entry { key, value, bytes }), retained];
    }

    #[cfg(test)]
    pub(super) fn get(&self, key: &K) -> Option<&T> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.key == *key)
            .map(|entry| &entry.value)
    }

    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        self.entries.iter().flatten().map(|entry| entry.bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::TwoEntryCache;

    #[test]
    fn two_layouts_reuse_resources_and_misses_preserve_both() {
        let mut cache = TwoEntryCache::new(100);
        cache.insert(1, 11, 40);
        cache.insert(2, 22, 60);
        assert_eq!(cache.retained_bytes(), 100);
        assert_eq!(cache.take(&3), None);
        assert_eq!(cache.take(&1), Some(11));
        cache.insert(1, 11, 40);
        assert_eq!(cache.take(&2), Some(22));
        assert_eq!(cache.take(&1), Some(11));
        assert_eq!(cache.retained_bytes(), 0);
    }

    #[test]
    fn insertion_obeys_combined_budget_and_preserves_cache_on_oversize() {
        let mut cache = TwoEntryCache::new(100);
        cache.insert(1, 11, 40);
        cache.insert(2, 22, 60);
        cache.insert(3, 33, 101);
        assert_eq!(cache.retained_bytes(), 100);
        assert_eq!(cache.get(&1), Some(&11));
        assert_eq!(cache.get(&2), Some(&22));
        cache.insert(3, 33, 70);
        assert_eq!(cache.retained_bytes(), 70);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), None);
        cache.insert(4, 44, 30);
        assert_eq!(cache.retained_bytes(), 100);
        cache.insert(5, 55, 30);
        assert_eq!(cache.get(&3), None);
        assert_eq!(cache.get(&4), Some(&44));
        assert_eq!(cache.get(&5), Some(&55));
    }

    #[test]
    fn returning_nested_same_key_replaces_without_duplicate_retention() {
        let mut cache = TwoEntryCache::new(100);
        cache.insert(1, 11, 30);
        let outer = cache.take(&1).unwrap();
        cache.insert(1, 22, 30);
        cache.insert(1, outer, 40);
        assert_eq!(cache.retained_bytes(), 40);
        assert_eq!(cache.take(&1), Some(11));
        assert_eq!(cache.take(&1), None);
    }
}
