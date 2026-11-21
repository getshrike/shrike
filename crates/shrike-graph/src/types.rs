use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use shrike_core::{Diagnostic, Span};
use shrike_syntax::ParseStatus;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub path: String,
    pub content_hash: String,
    pub module_kind: String,
    pub parse_status: ParseStatus,
    pub generated: bool,
    pub package: Option<String>,
    pub public_api: bool,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRecord {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub span: Span,
    pub exported: bool,
    pub entry: bool,
    pub public_api: bool,
    pub generated: bool,
    pub type_only: bool,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRecord {
    pub symbol: String,
    pub framework: String,
    pub kind: String,
    pub route: Option<String>,
    pub runtime: String,
    pub certainty: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRecord {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub certainty: String,
    pub file: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRecord {
    pub path: String,
    pub file: String,
    pub specifiers: Vec<String>,
    pub resolved: Vec<String>,
    pub unresolved: Vec<String>,
    /// Specifiers classified as external packages by the resolver. This is the
    /// only list dependency analysis may treat as npm package evidence.
    #[serde(default)]
    pub external: Vec<String>,
    /// Type-position module references (`import("pkg")` types and JSDoc
    /// `@type {import("pkg").X}`). Dependency evidence, never runtime edges.
    #[serde(default)]
    pub type_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownRecord {
    pub reason: String,
    pub scope: String,
    pub package: Option<String>,
    pub file: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WitnessRecord {
    pub symbol: String,
    pub path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofRecord {
    pub symbol: String,
    pub reason: String,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshot {
    pub schema_version: String,
    pub protocol_version: String,
    pub tool_version: String,
    pub config_hash: String,
    pub resolver_hash: String,
    pub files: Vec<FileRecord>,
    pub symbols: Vec<SymbolRecord>,
    pub modules: Vec<ModuleRecord>,
    pub entries: Vec<EntryRecord>,
    pub edges: Vec<EdgeRecord>,
    pub sccs: Vec<Vec<String>>,
    pub live: Vec<String>,
    pub candidates: Vec<String>,
    /// Files that no entry and no other module reach: no entry symbol, no
    /// incoming edge from another file, and a dead module symbol. A dead
    /// declaration in an otherwise imported file is not a dead file, and a
    /// file imported only by dead code is left to the cascade on a later pass.
    #[serde(default)]
    pub unused_files: Vec<String>,
    /// Exported symbols that are live only because their own module references
    /// them. They are not deletion candidates; this list exists for parity with
    /// tools that report unused exports.
    #[serde(default)]
    pub internal_only: Vec<String>,
    pub witnesses: Vec<WitnessRecord>,
    pub proofs: Vec<ProofRecord>,
    pub unknowns: Vec<UnknownRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub graph_hash: String,
}

impl GraphSnapshot {
    #[must_use]
    pub fn symbol(&self, key: &str) -> Option<&SymbolRecord> {
        self.symbols.iter().find(|symbol| symbol.key == key)
    }

    #[must_use]
    pub fn is_live(&self, key: &str) -> bool {
        self.live.iter().any(|value| value == key)
    }

    #[must_use]
    pub fn is_candidate(&self, key: &str) -> bool {
        self.candidates.iter().any(|value| value == key)
    }
}

/// Files no entry and no other module reaches. A file is unused only when it
/// parsed, is neither public API nor generated, has no entry symbol, has no
/// incoming edge from another file, and its module symbol is not live. This is
/// stricter than "every declaration is a candidate": an imported barrel is
/// used even when its exports are dead, and a file imported only by dead code
/// waits for the deletion cascade.
#[must_use]
pub fn unused_files(snapshot: &GraphSnapshot) -> Vec<String> {
    fn file_of(key: &str) -> &str {
        key.split_once('#').map_or(key, |(file, _)| file)
    }
    let entry_files: BTreeSet<&str> = snapshot
        .entries
        .iter()
        .map(|entry| file_of(entry.symbol.as_str()))
        .collect();
    let incoming_files: BTreeSet<&str> = snapshot
        .edges
        .iter()
        .filter(|edge| file_of(&edge.source) != file_of(&edge.target))
        .map(|edge| file_of(&edge.target))
        .collect();
    let live: BTreeSet<&str> = snapshot.live.iter().map(String::as_str).collect();
    snapshot
        .files
        .iter()
        .filter(|file| file.parse_status == ParseStatus::Ok)
        .filter(|file| !file.public_api && !file.generated)
        .filter(|file| !entry_files.contains(file.path.as_str()))
        .filter(|file| !incoming_files.contains(file.path.as_str()))
        .filter(|file| !live.contains(format!("{}#__module__", file.path).as_str()))
        .map(|file| file.path.clone())
        .collect()
}
