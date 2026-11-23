use rayon::prelude::*;
use serde::Serialize;
pub(crate) use shrike_core::parent_dir;
use shrike_syntax::DeclarationKind;

use crate::types::{EdgeRecord, GraphSnapshot};

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

/// Serialize a snapshot to canonical JSON.
///
/// The bytes are identical to `serde_json::to_string(&snapshot)`. The large
/// arrays (edges, symbols, witnesses) are serialized in parallel chunks and
/// concatenated, which avoids spending most of a cold scan on one core. The
/// result is reused for both the cache file and the protocol response.
#[must_use]
pub fn snapshot_json(snapshot: &GraphSnapshot) -> String {
    let estimate = snapshot.edges.len() * 190
        + snapshot.symbols.len() * 320
        + snapshot.witnesses.len() * 120
        + snapshot.files.len() * 260
        + snapshot.modules.len() * 500
        + snapshot.entries.len() * 130
        + snapshot.sccs.len() * 40
        + snapshot.live.len() * 40
        + 64 * 1024;
    let mut out = String::with_capacity(estimate + estimate / 4);
    out.push('{');
    let mut first = true;
    push_json_field(
        &mut out,
        &mut first,
        "schemaVersion",
        &snapshot.schema_version,
    );
    push_json_field(
        &mut out,
        &mut first,
        "protocolVersion",
        &snapshot.protocol_version,
    );
    push_json_field(&mut out, &mut first, "toolVersion", &snapshot.tool_version);
    push_json_field(&mut out, &mut first, "configHash", &snapshot.config_hash);
    push_json_field(
        &mut out,
        &mut first,
        "resolverHash",
        &snapshot.resolver_hash,
    );
    push_json_array(&mut out, &mut first, "files", &snapshot.files);
    push_json_array(&mut out, &mut first, "symbols", &snapshot.symbols);
    push_json_array(&mut out, &mut first, "modules", &snapshot.modules);
    push_json_array(&mut out, &mut first, "entries", &snapshot.entries);
    push_json_array(&mut out, &mut first, "edges", &snapshot.edges);
    push_json_array(&mut out, &mut first, "sccs", &snapshot.sccs);
    push_json_array(&mut out, &mut first, "live", &snapshot.live);
    push_json_array(&mut out, &mut first, "candidates", &snapshot.candidates);
    push_json_array(&mut out, &mut first, "unusedFiles", &snapshot.unused_files);
    push_json_array(
        &mut out,
        &mut first,
        "internalOnly",
        &snapshot.internal_only,
    );
    push_json_array(&mut out, &mut first, "witnesses", &snapshot.witnesses);
    push_json_array(&mut out, &mut first, "proofs", &snapshot.proofs);
    push_json_array(&mut out, &mut first, "unknowns", &snapshot.unknowns);
    push_json_array(&mut out, &mut first, "diagnostics", &snapshot.diagnostics);
    push_json_field(&mut out, &mut first, "graphHash", &snapshot.graph_hash);
    out.push('}');
    out
}

fn push_json_field<T: Serialize + ?Sized>(
    out: &mut String,
    first: &mut bool,
    key: &str,
    value: &T,
) {
    if !*first {
        out.push(',');
    }
    *first = false;
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(&serde_json::to_string(value).expect("graph snapshot field is serializable"));
}

fn push_json_array<T: Serialize + Sync>(
    out: &mut String,
    first: &mut bool,
    key: &str,
    values: &[T],
) {
    if !*first {
        out.push(',');
    }
    *first = false;
    out.push('"');
    out.push_str(key);
    out.push_str("\":[");
    if !values.is_empty() {
        // Chunks are serialized as arrays; strip their brackets and join with
        // commas so the concatenation is byte-identical to one array.
        let threads = rayon::current_num_threads().max(1);
        let chunk = (values.len() / (threads * 2)).max(256);
        let parts: Vec<String> = values
            .par_chunks(chunk)
            .map(|part| serde_json::to_string(part).expect("graph snapshot array is serializable"))
            .collect();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let inner = &part[1..part.len() - 1];
            out.push_str(inner);
        }
    }
    out.push(']');
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
        files: &'a [crate::types::FileRecord],
        symbols: &'a [crate::types::SymbolRecord],
        modules: &'a [crate::types::ModuleRecord],
        entries: &'a [crate::types::EntryRecord],
        edges: &'a [EdgeRecord],
        sccs: &'a [Vec<String>],
        live: &'a [String],
        candidates: &'a [String],
        unknowns: &'a [crate::types::UnknownRecord],
    }
    shrike_core::hash_json(&Canonical {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn snapshot_json_matches_serde_for_empty_and_large_arrays() {
        let edges: Vec<serde_json::Value> = (0..300)
            .map(|index| {
                json!({
                    "source": format!("file{index}.ts#a"),
                    "target": format!("file{index}.ts#b"),
                    "kind": "reference",
                    "certainty": "certain",
                    "file": format!("file{index}.ts"),
                    "span": {
                        "start": index,
                        "end": index + 1,
                        "line": 1,
                        "column": 1,
                        "endLine": 1
                    }
                })
            })
            .collect();
        let snapshot: GraphSnapshot = serde_json::from_value(json!({
            "schemaVersion": "1",
            "protocolVersion": "1.0",
            "toolVersion": "1.0.0",
            "configHash": "blake3:config",
            "resolverHash": "blake3:resolver",
            "files": [],
            "symbols": [],
            "modules": [],
            "entries": [],
            "edges": edges,
            "sccs": [["file0.ts#a", "file0.ts#b"]],
            "live": ["file0.ts#a"],
            "candidates": ["file1.ts#b"],
            "unusedFiles": [],
            "internalOnly": [],
            "witnesses": [],
            "proofs": [],
            "unknowns": [],
            "diagnostics": [],
            "graphHash": "blake3:graph"
        }))
        .expect("test snapshot is valid");
        assert_eq!(
            snapshot_json(&snapshot),
            serde_json::to_string(&snapshot).expect("snapshot is serializable")
        );
    }
}
