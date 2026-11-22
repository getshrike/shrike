use serde::Serialize;
use shrike_core::hash_json;
use shrike_syntax::DeclarationKind;

use crate::types::{
    EdgeRecord, EntryRecord, FileRecord, GraphSnapshot, ModuleRecord, SymbolRecord, UnknownRecord,
};

pub(crate) fn edge(
    source: &str,
    target: &str,
    kind: &str,
    certainty: &str,
    file: &str,
    span: shrike_core::Span,
) -> EdgeRecord {
    EdgeRecord {
        source: source.to_string(),
        target: target.to_string(),
        kind: kind.to_string(),
        certainty: certainty.to_string(),
        file: file.to_string(),
        span,
    }
}

pub(crate) fn declaration_kind(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Function => "function",
        DeclarationKind::Class => "class",
        DeclarationKind::Variable => "variable",
        DeclarationKind::Type => "type",
        DeclarationKind::Interface => "interface",
        DeclarationKind::Enum => "enum",
        DeclarationKind::Namespace => "namespace",
        DeclarationKind::Default => "default",
    }
}

pub(crate) fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_string())
}

pub(crate) fn route_of(path: &str) -> Option<String> {
    let stripped = path
        .strip_prefix("app/")
        .or_else(|| path.split_once("/app/").map(|(_, rest)| rest))
        .or_else(|| path.strip_prefix("pages/"))
        .or_else(|| path.split_once("/pages/").map(|(_, rest)| rest))?;
    let mut route = stripped.to_string();
    for suffix in [".tsx", ".ts", ".jsx", ".js", ".mdx"] {
        if let Some(stem) = route.strip_suffix(suffix) {
            route = stem.to_string();
            break;
        }
    }
    for suffix in ["/page", "/route", "/layout", "/index"] {
        if let Some(stem) = route.strip_suffix(suffix) {
            route = stem.to_string();
        }
    }
    if route.is_empty() {
        route = "/".to_string();
    } else if !route.starts_with('/') {
        route = format!("/{route}");
    }
    Some(route)
}

/// Recompute the content hash after an evidence join adds liveness.
pub fn rehash_snapshot(snapshot: &mut GraphSnapshot) {
    snapshot.graph_hash = graph_hash(snapshot);
}

pub(crate) fn graph_hash(snapshot: &GraphSnapshot) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Canonical<'a> {
        schema_version: &'a str,
        protocol_version: &'a str,
        tool_version: &'a str,
        config_hash: &'a str,
        resolver_hash: &'a str,
        files: &'a [FileRecord],
        symbols: &'a [SymbolRecord],
        modules: &'a [ModuleRecord],
        entries: &'a [EntryRecord],
        edges: &'a [EdgeRecord],
        sccs: &'a [Vec<String>],
        live: &'a [String],
        candidates: &'a [String],
        unknowns: &'a [UnknownRecord],
    }
    hash_json(&Canonical {
        schema_version: &snapshot.schema_version,
        protocol_version: &snapshot.protocol_version,
        tool_version: &snapshot.tool_version,
        config_hash: &snapshot.config_hash,
        resolver_hash: &snapshot.resolver_hash,
        files: &snapshot.files,
        symbols: &snapshot.symbols,
        modules: &snapshot.modules,
        entries: &snapshot.entries,
        edges: &snapshot.edges,
        sccs: &snapshot.sccs,
        live: &snapshot.live,
        candidates: &snapshot.candidates,
        unknowns: &snapshot.unknowns,
    })
}
