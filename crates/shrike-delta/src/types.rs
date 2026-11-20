use serde::{Deserialize, Serialize};
use shrike_core::hash_json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRef {
    pub r#ref: Option<String>,
    pub commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadRef {
    pub commit: String,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub file: String,
    pub kind: String,
    pub change: String,
    pub certainty: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaItem {
    pub symbol: String,
    pub kind: String,
    pub file: String,
    pub last_witness: Vec<String>,
    pub trigger: Trigger,
    pub confidence: String,
    pub estimated_lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cascade {
    pub symbols: usize,
    pub files: usize,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub id: String,
    pub symbols: usize,
    pub files: usize,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incomparable {
    pub reason: String,
    pub base: String,
    pub head: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaReport {
    pub schema_version: String,
    pub base: CommitRef,
    pub head: HeadRef,
    pub newly_dead: Vec<DeltaItem>,
    pub newly_dead_files: Vec<String>,
    pub cascade: Cascade,
    pub batches: Vec<BatchSummary>,
    pub incomparable: Option<Incomparable>,
    pub diagnostics: Vec<shrike_core::Diagnostic>,
}

impl DeltaReport {
    #[must_use]
    pub fn hash(&self) -> String {
        hash_json(self)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.newly_dead.is_empty() && self.newly_dead_files.is_empty()
    }
}
