use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use shrike_core::EngineConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheManifest {
    pub(crate) version: u32,
    pub(crate) entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestEntry {
    pub(crate) size: u64,
    pub(crate) mtime_ns: u128,
    pub(crate) hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzePayload {
    pub(crate) root: String,
    #[serde(default)]
    pub(crate) config: EngineConfig,
    #[serde(default)]
    pub(crate) cache_dir: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeltaPayload {
    pub(crate) base_snapshot: shrike_graph::GraphSnapshot,
    pub(crate) head_snapshot: shrike_graph::GraphSnapshot,
    #[serde(default)]
    pub(crate) base_ref: Option<String>,
    pub(crate) base_commit: String,
    pub(crate) head_commit: String,
    #[serde(default)]
    pub(crate) dirty: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanPayload {
    pub(crate) snapshot: shrike_graph::GraphSnapshot,
    pub(crate) delta: shrike_delta::DeltaReport,
    #[serde(default)]
    pub(crate) config: EngineConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProofPayload {
    pub(crate) snapshot: shrike_graph::GraphSnapshot,
    pub(crate) symbol: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildPayload {
    pub(crate) snapshot: shrike_graph::GraphSnapshot,
    pub(crate) build: shrike_evidence::BuildEvidence,
}
