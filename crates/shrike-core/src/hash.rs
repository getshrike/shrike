//! Content hashing.
//!
//! Graph and snapshot hashes use BLAKE3 and carry a `blake3:` prefix. Receipt
//! content hashes are computed by the host with SHA-256 and carry a `sha256:`
//! prefix; both satisfy the same schema pattern.

use serde::Serialize;

/// Hash raw bytes and return `blake3:<64 hex chars>`.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

/// Hash a UTF-8 string.
#[must_use]
pub fn content_hash(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

/// Hash a serializable value using canonical JSON.
///
/// # Panics
/// Panics only if a `Serialize` implementation is broken; callers pass owned
/// data structures with derived implementations.
#[must_use]
pub fn hash_json<T: Serialize>(value: &T) -> String {
    match serde_json::to_vec(value) {
        Ok(bytes) => hash_bytes(&bytes),
        Err(_) => hash_bytes(b"<unserializable>"),
    }
}

/// Short stable prefix used in branch names and cache paths.
#[must_use]
pub fn short_hash(hash: &str) -> String {
    let hex = hash.split_once(':').map_or(hash, |(_, hex)| hex);
    hex.chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_prefixed_and_deterministic() {
        let a = hash_bytes(b"shrike");
        let b = hash_bytes(b"shrike");
        assert_eq!(a, b);
        assert!(a.starts_with("blake3:"));
        assert_eq!(a.len(), "blake3:".len() + 64);
        assert_ne!(a, hash_bytes(b"Shrike"));
    }

    #[test]
    fn short_hash_strips_prefix() {
        let hash = hash_bytes(b"x");
        assert_eq!(short_hash(&hash).len(), 12);
    }
}
