#![forbid(unsafe_code)]
//! Content-addressed verification receipts.
//!
//! The id is SHA-256 over canonical JSON excluding `createdAt`. Signing is a
//! later feature; content addressing is the v1 integrity mechanism.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    pub commit: String,
    pub graph_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRef {
    pub id: String,
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRef {
    pub source: String,
    pub snapshot_hash: String,
    pub version: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRef {
    pub id: String,
    pub symbols: Vec<String>,
    pub files: Vec<String>,
    pub lines_removed: u32,
    pub patches: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRecord {
    pub name: String,
    pub command: String,
    pub status: String,
    pub baseline_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub reruns: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_signature: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rejection {
    pub check: String,
    pub symbols: Vec<String>,
    pub failure_signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRecord {
    pub entry_policy: String,
    pub max_symbols: u32,
    pub max_files: u32,
    pub allow_oversized: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub node: String,
    pub package_manager: String,
    pub platform: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub schema_version: String,
    pub id: String,
    pub created_at: String,
    pub tool: Tool,
    pub base: GraphRef,
    pub head: GraphRef,
    pub plan: PlanRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildRef>,
    pub batch: BatchRef,
    pub checks: Vec<CheckRecord>,
    pub outcome: String,
    pub rejected: Vec<Rejection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRecord>,
    pub environment: Environment,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> Receipt {
        Receipt {
            schema_version: "1.0".into(),
            id: String::new(),
            created_at: "2026-11-20T00:00:00.000Z".into(),
            tool: Tool {
                name: "shrike".into(),
                version: "1.0.0".into(),
            },
            base: GraphRef {
                r#ref: Some("main".into()),
                commit: "a".into(),
                graph_hash: "sha256:a".into(),
            },
            head: GraphRef {
                r#ref: None,
                commit: "b".into(),
                graph_hash: "sha256:b".into(),
            },
            plan: PlanRef {
                id: "plan_x".into(),
                hash: "sha256:p".into(),
            },
            build: None,
            batch: BatchRef {
                id: "batch_1".into(),
                symbols: vec!["a.ts#x".into()],
                files: vec!["a.ts".into()],
                lines_removed: 1,
                patches: vec![],
            },
            checks: vec![CheckRecord {
                name: "typecheck".into(),
                command: "pnpm typecheck".into(),
                status: "pass".into(),
                baseline_status: "pass".into(),
                duration_ms: Some(1),
                reruns: 0,
                failure_signature: None,
            }],
            outcome: "verified".into(),
            rejected: vec![],
            policy: None,
            environment: Environment {
                node: "v24".into(),
                package_manager: "pnpm".into(),
                platform: "linux".into(),
            },
        }
    }

    #[test]
    fn id_is_stable_and_verifies() {
        let mut first = receipt();
        first.id = first.compute_id();
        assert!(first.verify());
        let mut second = receipt();
        second.created_at = "2030-01-01T00:00:00.000Z".into();
        second.id = second.compute_id();
        assert_eq!(first.id, second.id);
    }
}
