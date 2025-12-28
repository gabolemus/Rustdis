//! Custom hash map implementation for learning purposes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A minimal, learning-oriented hash map using **separate chaining**.
///
/// - Buckets are stored as a `Vec` of `Vec`s.
/// - Each bucket holds `(K, V)` key/value pairs.
/// - Collisions are resolved by storing multiple pairs in the same bucket vector.
///
/// This implementation supports:
/// - `insert` / `get` / `remove`
/// - Grow when projected load factor >= 0.75
/// - Shrink after removals when load factor < 0.2
/// - Simple metrics: cumulative collision inserts + max chain length
#[derive(Debug)]
pub struct HashMap<K, V> {
    /// Buckets of chained key/value pairs.
    ///
    /// The outer vector length is the number of buckets. Each inner vector is a chain.
    buckets: Vec<Vec<(K, V)>>,

    /// Total number of stored key/value pairs across all buckets.
    len: usize,

    /// Minimum bucket count this map will ever shrink to.
    ///
    /// In this implementation it is set to the **initial bucket count** at construction time
    /// (after applying the minimum default of 10).
    min_buckets: usize,

    /// Cumulative metric: number of *new-key* inserts that landed in a non-empty bucket.
    ///
    /// This is a simple proxy for “how often did we collide on insert over the lifetime of the map”.
    collision_inserts: usize,

    /// Current maximum chain length across all buckets.
    ///
    /// This is recomputed after `rehash_to` and after successful `remove`.
    max_chain_len: usize,
}

impl<K, V> HashMap<K, V>
where
    K: Eq + Hash,
{
    // ========================= Creation functions =========================

    /// Creates a new `HashMap` with a default of **10 buckets**.
    pub fn new() -> Self {
        Self::with_capacity(10)
    }

    /// Creates a new `HashMap` with at least `size` buckets.
    ///
    /// If `size < 10`, this will allocate **10 buckets** (minimum default).
    ///
    /// Note: the resulting initial bucket count becomes the minimum shrink size (`min_buckets`).
    pub fn with_capacity(size: usize) -> Self {
        let size = size.max(10);

        Self {
            buckets: (0..size).map(|_| Vec::new()).collect(),
            len: 0,
            min_buckets: size,
            collision_inserts: 0,
            max_chain_len: 0,
        }
    }

    // ============================ Basic stats =============================

    /// Returns the number of stored key/value pairs.
    ///
    /// With separate chaining, this is the total count of pairs across all chains,
    /// *not* the number of non-empty buckets.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the map contains no key/value pairs.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of buckets (the outer vector length).
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Returns the load factor α = `len / bucket_count`.
    ///
    /// For separate chaining, this is also the **average expected chain length**
    /// under uniform hashing.
    pub fn load_factor(&self) -> f64 {
        self.len() as f64 / self.bucket_count() as f64
    }

    /// Returns the cumulative number of *new-key* inserts that collided
    /// (i.e., were inserted into a non-empty bucket).
    pub fn collision_inserts(&self) -> usize {
        self.collision_inserts
    }

    /// Returns the current maximum chain length across all buckets.
    pub fn max_chain_len(&self) -> usize {
        self.max_chain_len
    }

    /// Returns a derived “current collisions” count.
    ///
    /// This counts how many items are stored beyond the first in each bucket:
    /// `sum(max(bucket_len - 1, 0))`.
    pub fn current_collisions(&self) -> usize {
        self.buckets.iter().map(|b| b.len().saturating_sub(1)).sum()
    }

    /// Returns a vector of references to all keys in the map.
    ///
    /// The keys are returned in *__no particular order__*.
    /// The returned references are valid as long as `self` is borrowed.
    pub fn keys(&self) -> Vec<&K> {
        self.buckets
            .iter()
            .flat_map(|bucket| bucket.iter().map(|(k, _v)| k))
            .collect()
    }

    /// Returns a vector of references to all values in the map.
    ///
    /// The values are returned in *__no particular order__*.
    /// The returned references are valid as long as `self` is borrowed.
    pub fn values(&self) -> Vec<&V> {
        self.buckets
            .iter()
            .flat_map(|bucket| bucket.iter().map(|(_k, v)| v))
            .collect()
    }

    // =========================== Hashing/indexing ===========================

    /// Hashes a key using Rust's standard `Hash` trait and returns a 64-bit hash.
    fn hash(key: &K) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Computes the bucket index for `key` given a bucket count.
    fn bucket_index_for(bucket_len: usize, key: &K) -> usize {
        (Self::hash(key) as usize) % bucket_len
    }

    /// Computes the bucket index for `key` in the current table.
    fn bucket_index(&self, key: &K) -> usize {
        Self::bucket_index_for(self.buckets.len(), key)
    }

    // =========================== Rehash/resizing ===========================

    /// Recomputes `max_chain_len` from scratch.
    fn recompute_max_chain_len(&mut self) {
        self.max_chain_len = self.buckets.iter().map(|b| b.len()).max().unwrap_or(0);
    }

    /// Rehashes all entries into a new table with `new_bucket_count` buckets.
    ///
    /// This moves keys and values without cloning.
    fn rehash_to(&mut self, new_bucket_count: usize) {
        assert!(new_bucket_count > 0);

        let mut new_buckets: Vec<Vec<(K, V)>> = (0..new_bucket_count).map(|_| Vec::new()).collect();

        for mut bucket in self.buckets.drain(..) {
            for (k, v) in bucket.drain(..) {
                let idx = Self::bucket_index_for(new_bucket_count, &k);
                new_buckets[idx].push((k, v));
            }
        }

        self.buckets = new_buckets;
        // len is unchanged.
        self.recompute_max_chain_len();
    }

    /// Grows the table if inserting one new element would make load factor >= 0.75.
    fn maybe_grow_for_new_insert(&mut self) {
        let projected_len = self.len + 1;
        let projected_load = projected_len as f64 / self.buckets.len() as f64;

        if projected_load >= 0.75 {
            self.rehash_to(self.buckets.len() * 2);
        }
    }

    /// Shrinks the table after a successful removal if load factor drops below 0.2.
    ///
    /// Never shrinks below `min_buckets`.
    fn maybe_shrink_after_remove(&mut self) {
        if self.buckets.len() <= self.min_buckets {
            return;
        }

        if self.len == 0 {
            if self.buckets.len() != self.min_buckets {
                self.rehash_to(self.min_buckets);
            }
            return;
        }

        if self.load_factor() < 0.2 {
            let mut new_count = self.buckets.len() / 2;
            if new_count < self.min_buckets {
                new_count = self.min_buckets;
            }
            if new_count != self.buckets.len() {
                self.rehash_to(new_count);
            }
        }
    }

    // =========================== Core operations ===========================

    /// Inserts a key/value pair.
    ///
    /// - If the key already exists, replaces the value and returns the old value (`Some(old)`).
    /// - If the key is new:
    ///   - grows the table if projected load factor >= 0.75
    ///   - inserts the pair into the computed bucket
    ///   - increments `len`
    ///   - updates metrics (`collision_inserts`, `max_chain_len`)
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        // Existing key? Replace in-place; do not grow; do not change len.
        let idx = self.bucket_index(&key);
        if let Some((_, v)) = self.buckets[idx].iter_mut().find(|(k, _)| k == &key) {
            return Some(std::mem::replace(v, value));
        }

        // New key: maybe grow first.
        self.maybe_grow_for_new_insert();

        // Recompute index in case we rehashed.
        let idx = self.bucket_index(&key);
        let bucket = &mut self.buckets[idx];

        if !bucket.is_empty() {
            self.collision_inserts += 1;
        }

        bucket.push((key, value));
        self.len += 1;

        if bucket.len() > self.max_chain_len {
            self.max_chain_len = bucket.len();
        }

        None
    }

    /// Returns the value for `key` if present.
    pub fn get(&self, key: &K) -> Option<&V> {
        let idx = self.bucket_index(key);
        self.buckets[idx]
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Removes `key` from the map, returning its value if present.
    ///
    /// This searches only the computed bucket (the chain) and removes the entry
    /// using `swap_remove` (does not preserve chain order).
    ///
    /// On successful removal:
    /// - `len` is decremented
    /// - `max_chain_len` is recomputed
    /// - the map may shrink if load factor < 0.2
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let idx = self.bucket_index(key);
        let bucket = &mut self.buckets[idx];

        let pos = bucket.iter().position(|(k, _)| k == key)?;
        let (_k, v) = bucket.swap_remove(pos);

        self.len -= 1;
        self.recompute_max_chain_len();
        self.maybe_shrink_after_remove();

        Some(v)
    }
}

pub struct Iter<'a, K, V> {
    buckets: std::slice::Iter<'a, Vec<(K, V)>>,
    current_bucket_iter: Option<std::slice::Iter<'a, (K, V)>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to pull from the current bucket first
            if let Some(ref mut bucket_iter) = self.current_bucket_iter {
                if let Some((k, v)) = bucket_iter.next() {
                    return Some((k, v));
                }
            }

            // Current bucket exhausted (or none yet), move to next bucket
            let next_bucket = self.buckets.next()?;
            self.current_bucket_iter = Some(next_bucket.iter());
        }
    }
}

impl<K, V> HashMap<K, V>
where
    K: Eq + Hash,
{
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            buckets: self.buckets.iter(),
            current_bucket_iter: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HashMap;

    #[test]
    fn insert_and_get_basic() {
        let mut m: HashMap<String, String> = HashMap::new();
        assert_eq!(m.insert("lang".into(), "rust".into()), None);

        let key = "lang".to_string();
        assert_eq!(m.get(&key).map(|s| s.as_str()), Some("rust"));

        assert_eq!(m.len(), 1);
        assert!(m.bucket_count() >= 10);
    }

    #[test]
    fn insert_replaces_value_without_changing_len() {
        let mut m: HashMap<String, String> = HashMap::new();
        assert_eq!(m.insert("k".into(), "v1".into()), None);
        assert_eq!(m.insert("k".into(), "v2".into()), Some("v1".into()));

        let key = "k".to_string();
        assert_eq!(m.get(&key).map(|s| s.as_str()), Some("v2"));

        assert_eq!(m.len(), 1);
    }

    #[test]
    fn remove_existing_and_nonexisting() {
        let mut m: HashMap<String, String> = HashMap::new();
        m.insert("x".into(), "10".into());
        m.insert("y".into(), "20".into());

        let key_x = "x".to_string();
        let key_y = "y".to_string();
        let key_missing = "does_not_exist".to_string();

        assert_eq!(m.remove(&key_x), Some("10".into()));
        assert_eq!(m.get(&key_x), None);
        assert_eq!(m.len(), 1);

        assert_eq!(m.remove(&key_missing), None);
        assert_eq!(m.len(), 1);

        // still present:
        assert_eq!(m.get(&key_y).map(|s| s.as_str()), Some("20"));
    }

    #[test]
    fn grow_when_projected_load_reaches_75_percent() {
        let mut m: HashMap<String, String> = HashMap::with_capacity(4);
        assert_eq!(m.bucket_count(), 10);

        for i in 0..7 {
            m.insert(format!("k{i}"), format!("v{i}"));
        }
        assert_eq!(m.len(), 7);
        assert_eq!(m.bucket_count(), 10);

        m.insert("k7".into(), "v7".into());
        assert_eq!(m.len(), 8);
        assert_eq!(m.bucket_count(), 20);

        let k0 = "k0".to_string();
        let k6 = "k6".to_string();
        let k7 = "k7".to_string();

        assert_eq!(m.get(&k0).map(|s| s.as_str()), Some("v0"));
        assert_eq!(m.get(&k6).map(|s| s.as_str()), Some("v6"));
        assert_eq!(m.get(&k7).map(|s| s.as_str()), Some("v7"));
    }

    #[test]
    fn shrink_after_growth_keeps_items() {
        let mut m: HashMap<String, String> = HashMap::with_capacity(4);
        assert_eq!(m.bucket_count(), 10);

        for i in 0..8 {
            m.insert(format!("k{i}"), format!("v{i}"));
        }
        assert_eq!(m.bucket_count(), 20);
        assert_eq!(m.len(), 8);

        for i in 0..5 {
            let key = format!("k{i}");
            assert!(m.remove(&key).is_some());
        }

        assert_eq!(m.len(), 3);
        assert_eq!(m.bucket_count(), 10);

        let k5 = "k5".to_string();
        let k6 = "k6".to_string();
        let k7 = "k7".to_string();

        assert_eq!(m.get(&k5).map(|s| s.as_str()), Some("v5"));
        assert_eq!(m.get(&k6).map(|s| s.as_str()), Some("v6"));
        assert_eq!(m.get(&k7).map(|s| s.as_str()), Some("v7"));
    }

    #[test]
    fn shrink_when_load_factor_below_0_2_but_not_below_min() {
        let mut m: HashMap<String, String> = HashMap::with_capacity(16);
        assert_eq!(m.bucket_count(), 16);

        m.insert("a".into(), "1".into());
        m.insert("b".into(), "2".into());
        m.insert("c".into(), "3".into());
        m.insert("d".into(), "4".into());

        let a = "a".to_string();
        let b = "b".to_string();
        let c = "c".to_string();
        let d = "d".to_string();

        m.remove(&a);
        m.remove(&b);
        m.remove(&c);

        assert_eq!(m.bucket_count(), 16);
        assert_eq!(m.get(&d).map(|s| s.as_str()), Some("4"));
    }

    #[test]
    fn collisions_and_max_chain_length_are_tracked() {
        let mut m: HashMap<String, String> = HashMap::new();

        for i in 0..200 {
            m.insert(format!("k{i}"), format!("v{i}"));
        }

        assert_eq!(m.len(), 200);
        assert!(m.max_chain_len() >= 1);

        assert!(m.collision_inserts() > 0);
        assert!(m.current_collisions() > 0);
        assert!(m.max_chain_len() >= 2);
    }
}
