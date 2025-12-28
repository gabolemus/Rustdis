//! Custom hash map implementation for learning purposes.

/// Custom HashMap implementation.
#[derive(Debug)]
pub struct HashMap {
    /// Available slots in the HashMap.
    buckets: Vec<Vec<(String, String)>>,
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
        }
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

    /// Map a key to a bucket index.
    fn bucket_index(&self, key: &str) -> usize {
        let h = Self::hash(key);
        (h as usize) % self.buckets.len()
    }

    /// Insert key/value pair. If the key existed previously, it will be replaced and return the
    /// old value.
    pub fn insert(&mut self, key: String, value: String) -> Option<String> {
        let idx = self.bucket_index(&key);
        let bucket = &mut self.buckets[idx];

        // Separate chaining: scan the bucket for the existing key
        for (k, v) in bucket.iter_mut() {
            if k == &key {
                // Replace value and return the old one
                return Some(std::mem::replace(v, value));
            }
        }

        // The key didn't previously exist. Push it into the bucket.
        bucket.push((key, value));
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
}
