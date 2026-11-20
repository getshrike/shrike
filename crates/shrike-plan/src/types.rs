use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanBatch {
    pub id: String,
    pub symbols: Vec<String>,
    pub files: Vec<String>,
    pub lines: u32,
    pub patch: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedItem {
    pub symbol: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanTotals {
    pub symbols: usize,
    pub files: usize,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPolicy {
    pub entry_policy: String,
    pub max_symbols: u32,
    pub max_files: u32,
    pub allow_oversized: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionPlan {
    pub schema_version: String,
    pub id: String,
    pub base: shrike_delta::CommitRef,
    pub head: shrike_delta::HeadRef,
    pub delta_hash: String,
    pub totals: PlanTotals,
    pub batches: Vec<PlanBatch>,
    pub blocked: Vec<BlockedItem>,
    pub policy: PlanPolicy,
    pub diagnostics: Vec<shrike_core::Diagnostic>,
}
