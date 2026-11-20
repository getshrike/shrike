use std::collections::BTreeMap;

use super::*;
use shrike_core::{Span, hash_json};
use shrike_graph::{FileRecord, GraphSnapshot, SymbolRecord};

mod augment;
mod diagnostics;
mod join;
mod verdicts;

fn snapshot() -> GraphSnapshot {
    let span = Span {
        start: 0,
        end: 10,
        line: 1,
        column: 0,
        end_line: 1,
    };
    GraphSnapshot {
        schema_version: "1.0".into(),
        protocol_version: "1.0".into(),
        tool_version: "1.0.0".into(),
        config_hash: "blake3:00".into(),
        resolver_hash: "blake3:00".into(),
        files: vec![
            FileRecord {
                path: "app/page.tsx".into(),
                content_hash: "blake3:00".into(),
                module_kind: "module".into(),
                parse_status: shrike_syntax::ParseStatus::Ok,
                generated: false,
                package: None,
                public_api: false,
                lines: 1,
            },
            FileRecord {
                path: "lib/format.ts".into(),
                content_hash: "blake3:01".into(),
                module_kind: "module".into(),
                parse_status: shrike_syntax::ParseStatus::Ok,
                generated: false,
                package: None,
                public_api: false,
                lines: 1,
            },
        ],
        symbols: vec![SymbolRecord {
            key: "lib/format.ts#formatInvoice".into(),
            name: "formatInvoice".into(),
            kind: "function".into(),
            file: "lib/format.ts".into(),
            span,
            exported: true,
            entry: false,
            public_api: false,
            generated: false,
            type_only: false,
            lines: 1,
        }],
        modules: Vec::new(),
        entries: Vec::new(),
        edges: Vec::new(),
        sccs: Vec::new(),
        live: Vec::new(),
        candidates: Vec::new(),
        witnesses: Vec::new(),
        proofs: Vec::new(),
        unknowns: Vec::new(),
        diagnostics: Vec::new(),
        graph_hash: hash_json(&"test"),
    }
}

fn candidate_snapshot() -> GraphSnapshot {
    let mut snapshot = snapshot();
    let span = Span {
        start: 0,
        end: 10,
        line: 1,
        column: 0,
        end_line: 1,
    };
    snapshot.symbols.push(SymbolRecord {
        key: "app/page.tsx#default".into(),
        name: "default".into(),
        kind: "function".into(),
        file: "app/page.tsx".into(),
        span,
        exported: true,
        entry: true,
        public_api: false,
        generated: false,
        type_only: false,
        lines: 1,
    });
    snapshot.live = vec!["app/page.tsx#default".into()];
    snapshot.candidates = vec!["lib/format.ts#formatInvoice".into()];
    snapshot
}

fn verdict_for<'a>(join: &'a BuildJoin, key: &str) -> &'a str {
    join.verdicts
        .iter()
        .find(|item| item.symbol == key)
        .map_or("missing", |item| item.verdict.as_str())
}
