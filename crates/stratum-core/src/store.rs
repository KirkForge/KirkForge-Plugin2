use parking_lot::RwLock;
use std::collections::HashMap;

pub trait OffloadStore: Send + Sync {
    fn put(&self, payload: &str) -> String;
    fn get(&self, key: &str) -> Option<String>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn backend_name(&self) -> &'static str;
}

pub struct InMemoryOffloadStore {
    data: RwLock<HashMap<String, String>>,
}

impl InMemoryOffloadStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryOffloadStore {
    fn default() -> Self {
        Self::new()
    }
}

impl OffloadStore for InMemoryOffloadStore {
    fn put(&self, payload: &str) -> String {
        let key = derive_key(payload);
        self.data.write().insert(key.clone(), payload.to_string());
        key
    }

    fn get(&self, key: &str) -> Option<String> {
        self.data.read().get(key).cloned()
    }

    fn len(&self) -> usize {
        self.data.read().len()
    }

    fn backend_name(&self) -> &'static str {
        "memory"
    }
}

fn derive_key(payload: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(payload.as_bytes());
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    let mut out = String::with_capacity(24);
    for b in &bytes[..12] {
        use std::fmt::Write;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inmemory_store_roundtrips() {
        let store = InMemoryOffloadStore::new();
        let key = store.put("hello world");
        assert_eq!(store.get(&key), Some("hello world".to_string()));
    }

    #[test]
    fn duplicate_payload_shares_key() {
        let store = InMemoryOffloadStore::new();
        let a = store.put("duplicate");
        let b = store.put("duplicate");
        assert_eq!(a, b);
        assert_eq!(store.len(), 1);
    }
}
