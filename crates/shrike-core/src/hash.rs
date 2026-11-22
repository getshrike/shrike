//! Content hashing.
//!
//! Graph and snapshot hashes use BLAKE3 and carry a `blake3:` prefix. Receipt
//! content hashes are computed by the host with SHA-256 and carry a `sha256:`
//! prefix; both satisfy the same schema pattern.

use std::io::Write;

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
    // Stream the canonical JSON straight into the hasher. The bytes are
    // identical to `to_vec`, but large snapshots avoid a second full copy.
    struct HashWriter(blake3::Hasher);

    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // serde_json emits many small writes; buffer them so the hasher sees a
    // few large chunks instead of one update per token.
    let mut writer =
        std::io::BufWriter::with_capacity(256 * 1024, HashWriter(blake3::Hasher::new()));
    if serde_json::to_writer(&mut writer, value).is_err() || writer.flush().is_err() {
        return hash_bytes(b"<unserializable>");
    }
    let HashWriter(hasher) = match writer.into_inner() {
        Ok(writer) => writer,
        Err(_) => return hash_bytes(b"<unserializable>"),
    };
    format!("blake3:{}", hasher.finalize().to_hex())
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
