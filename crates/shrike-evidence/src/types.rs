use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use shrike_core::Diagnostic;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEvidence {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub routes: Vec<BuildRoute>,
    #[serde(default)]
    pub sizes: BTreeMap<String, u64>,
    #[serde(default)]
    pub snapshot_hash: Option<String>,
    /// File or symbol key to `client` / `server` / `edge` / `unknown`.
    #[serde(default)]
    pub layers: BTreeMap<String, String>,
    /// Build module id to normalized repository path.
    #[serde(default)]
    pub module_ids: BTreeMap<String, String>,
    /// Server-to-client and client-to-server boundary records.
    #[serde(default)]
    pub client_references: Vec<ClientReference>,
    /// Source hashes captured by the build, used to detect stale snapshots.
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>,
    /// Records the host could not classify. Additive evidence is skipped.
    #[serde(default)]
    pub unknown_records: usize,
    /// Host-detected override: `stale`, `malformed`, `build-failed`, or `unsupported`.
    #[serde(default)]
    pub status: Option<String>,
    /// Emit `client-boundary-leak` diagnostics. Defaults to true.
    #[serde(default)]
    pub client_boundary_leaks: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRoute {
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    /// `client`, `server`, `edge`, `both`, or absent when unknown.
    #[serde(default)]
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientReference {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildVerdict {
    pub symbol: String,
    pub file: String,
    /// `present`, `server-only`, `absent`, `unknown`, `unsupported`, or `stale`.
    pub verdict: String,
    pub layer: String,
    pub routes: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEdge {
    pub from: String,
    pub to: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRouteJoin {
    pub route: String,
    pub files: usize,
    pub symbols: usize,
    pub bytes: u64,
    /// `client`, `server`, `edge`, `both`, or absent when the export omitted it.
    pub runtime: Option<String>,
    /// Matched route items whose size is unknown.
    pub unknown: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildBytes {
    /// Bytes of every matched module with a known size, counted once.
    pub matched: u64,
    /// Bytes of matched modules that appear on more than one route.
    pub shared: u64,
    /// Sum of route-local bytes; shared modules are not counted here.
    pub total_route: u64,
    /// Matched modules with no size. They contribute zero to the sums above
    /// but are reported as unknown rather than silently treated as empty.
    pub unknown: usize,
    /// True when at least one matched module has no size.
    pub partial: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildJoin {
    pub status: String,
    pub joined: usize,
    pub total: usize,
    pub coverage: f64,
    pub unmatched: Vec<String>,
    pub routes: Vec<BuildRouteJoin>,
    pub bytes: BuildBytes,
    pub verdicts: Vec<BuildVerdict>,
    pub edges: Vec<BuildEdge>,
    pub unknown_sizes: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}
