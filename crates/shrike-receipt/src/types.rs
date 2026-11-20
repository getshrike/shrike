use serde::{Deserialize, Serialize};

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
