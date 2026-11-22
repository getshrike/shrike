use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::types::Receipt;

impl Receipt {
    /// Canonical JSON: object keys sorted recursively, no whitespace.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut value {
            map.remove("createdAt");
            map.remove("id");
        }
        canonical_json(&value)
    }

    #[must_use]
    pub fn compute_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical().as_bytes());
        format!("rcpt_{}", &hex(&hasher.finalize())[..24])
    }

    /// Recompute the content hash and compare it with the stored id.
    #[must_use]
    pub fn verify(&self) -> bool {
        self.compute_id() == self.id
    }

    #[must_use]
    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical().as_bytes());
        format!("sha256:{}", hex(&hasher.finalize()))
    }
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut output = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).unwrap_or_default());
                output.push(':');
                output.push_str(&canonical_json(&map[*key]));
            }
            output.push('}');
            output
        }
        Value::Array(items) => {
            let mut output = String::from("[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&canonical_json(item));
            }
            output.push(']');
            output
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
