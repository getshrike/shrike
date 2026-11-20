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
