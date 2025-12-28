//! Custom hash map implementation for learning purposes.

/// Custom HashMap implementation.
#[derive(Debug)]
pub struct HashMap {
    /// Available slots in the HashMap.
    buckets: Vec<Vec<(String, String)>>,
    /// Number of stored key/value pairs.
    len: usize,
}

impl HashMap {
    /// Creates a new HashMap with 10 available buckets.
    pub fn new() -> Self {
        Self::with_capacity(10)
    }

    /// Creates a new HashMap with the given capacity. If the size is 0, a HashMap with 10 buckets
    /// is created.
    pub fn with_capacity(size: usize) -> Self {
        let size = size.max(10);

        Self {
            buckets: (0..size).map(|_| Vec::new()).collect(),
            len: 0,
        }
    }

    /// Returns the amount of items currently stored.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns the number of available buckets.
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Computes the load factor of the hash map.
    pub fn load_factor(&self) -> f64 {
        self.len() as f64 / self.bucket_count() as f64
    }

    /// FNV-1a 64-bit hash.
    fn hash(key: &str) -> u64 {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x00000100000001B3;

        let mut hash = FNV_OFFSET_BASIS;
        for b in key.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    fn bucket_index_for(bucket_len: usize, key: &str) -> usize {
        (Self::hash(key) as usize) % bucket_len
    }

    /// Map a key to a bucket index.
    fn bucket_index(&self, key: &str) -> usize {
        Self::bucket_index_for(self.buckets.len(), key)
    }

    /// Grow bucket count and rehash everything.
    fn rehash_to(&mut self, new_bucket_count: usize) {
        assert!(new_bucket_count > 0);

        let mut new_buckets: Vec<Vec<(String, String)>> =
            (0..new_bucket_count).map(|_| Vec::new()).collect();

        // Move all existing pairs into new buckets (no clones).
        for mut bucket in self.buckets.drain(..) {
            for (k, v) in bucket.drain(..) {
                let idx = Self::bucket_index_for(new_bucket_count, &k);
                new_buckets[idx].push((k, v));
            }
        }

        self.buckets = new_buckets;
    }

    fn maybe_grow_for_insert(&mut self) {
        // If we insert one more element, will load factor be >= 0.75?
        let projected_len = self.len + 1;
        let projected_load = projected_len as f64 / self.buckets.len() as f64;

        if projected_load >= 0.75 {
            // Minimal strategy: double bucket count (common approach).
            let new_count = self.buckets.len() * 2;
            self.rehash_to(new_count);
        }
    }

    /// Insert key/value. If key existed, replace and return old value.
    /// If new key would push load factor to >= 0.75, grow + rehash first.
    pub fn insert(&mut self, key: String, value: String) -> Option<String> {
        // First check if key already exists (so we don't grow unnecessarily).
        let idx = self.bucket_index(&key);
        if let Some((_, v)) = self.buckets[idx].iter_mut().find(|(k, _)| k == &key) {
            return Some(std::mem::replace(v, value));
        }

        // It's a new key -> may increase len, so grow if needed.
        self.maybe_grow_for_insert();

        // Recompute index because we might have rehashed.
        let idx = self.bucket_index(&key);
        self.buckets[idx].push((key, value));
        self.len += 1;
        None
    }

    /// Get a given't key's value if it exists.
    pub fn get(&self, key: &str) -> Option<&str> {
        let idx = self.bucket_index(key);
        self.buckets[idx]
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashmap_tests() {
        let mut m = HashMap::new();
        assert_eq!(m.insert("lang".to_string(), "rust".to_string()), None);
        assert_eq!(m.get("lang"), Some("rust"));

        // Update existing key:
        assert_eq!(
            m.insert("lang".to_string(), "Rust".to_string()),
            Some("rust".to_string())
        );
        assert_eq!(m.get("lang"), Some("Rust"));
    }

    #[test]
    fn load_factor_threshold_test() {
        let mut m = HashMap::with_capacity(4);

        // Insert enough to trigger growth (0.75 threshold).
        m.insert("a".into(), "1".into()); // len=1, 1/4=0.25
        m.insert("b".into(), "2".into()); // len=2, 2/4=0.5
        // Next insert would make 3/4 = 0.75 -> triggers grow before insert
        m.insert("c".into(), "3".into());

        assert_eq!(m.get("a"), Some("1"));
        assert_eq!(m.get("b"), Some("2"));
        assert_eq!(m.get("c"), Some("3"));
    }
}
