use shrike_core::{EngineConfig, Span};
use shrike_delta::{Cascade, CommitRef, DeltaItem, DeltaReport, HeadRef, Trigger};
use shrike_graph::{EdgeRecord, GraphSnapshot, SymbolRecord};

use super::*;

fn symbol(file: &str, name: &str, kind: &str) -> SymbolRecord {
    SymbolRecord {
        key: format!("{file}#{name}"),
        name: name.to_string(),
        kind: kind.to_string(),
        file: file.to_string(),
        span: Span::new(0, 1, 1, 1),
        exported: true,
        entry: false,
        public_api: false,
        generated: false,
        type_only: false,
        lines: 1,
    }
}

fn re_export(source_file: &str, target_file: &str, name: &str) -> EdgeRecord {
    EdgeRecord {
        source: format!("{source_file}#{name}"),
        target: format!("{target_file}#{name}"),
        kind: "re-export".to_string(),
        certainty: "certain".to_string(),
        file: source_file.to_string(),
        span: Span::new(0, 1, 1, 1),
    }
}

fn snapshot(symbols: Vec<SymbolRecord>, edges: Vec<EdgeRecord>) -> GraphSnapshot {
    GraphSnapshot {
        schema_version: "1.0".to_string(),
        protocol_version: "1.0".to_string(),
        tool_version: "test".to_string(),
        config_hash: "config".to_string(),
        resolver_hash: "resolver".to_string(),
        files: Vec::new(),
        symbols,
        modules: Vec::new(),
        entries: Vec::new(),
        edges,
        sccs: Vec::new(),
        live: Vec::new(),
        candidates: Vec::new(),
        unused_files: Vec::new(),
        internal_only: Vec::new(),
        witnesses: Vec::new(),
        proofs: Vec::new(),
        unknowns: Vec::new(),
        diagnostics: Vec::new(),
        graph_hash: "graph".to_string(),
    }
}

fn delta(snapshot: &GraphSnapshot) -> DeltaReport {
    DeltaReport {
        schema_version: "1.0".to_string(),
        base: CommitRef {
            r#ref: None,
            commit: "a".to_string(),
        },
        head: HeadRef {
            commit: "b".to_string(),
            dirty: false,
        },
        newly_dead: snapshot
            .symbols
            .iter()
            .map(|symbol| DeltaItem {
                symbol: symbol.key.clone(),
                kind: symbol.kind.clone(),
                file: symbol.file.clone(),
                last_witness: Vec::new(),
                trigger: Trigger {
                    file: symbol.file.clone(),
                    kind: "reference".to_string(),
                    change: "removed".to_string(),
                    certainty: "certain".to_string(),
                },
                confidence: "certain".to_string(),
                estimated_lines: symbol.lines,
            })
            .collect(),
        newly_dead_files: Vec::new(),
        cascade: Cascade {
            symbols: 0,
            files: 0,
            lines: 0,
        },
        batches: Vec::new(),
        incomparable: None,
        diagnostics: Vec::new(),
    }
}

#[test]
fn empty_delta_yields_empty_plan() {
    let snapshot = shrike_graph::build_snapshot(
        std::path::Path::new("."),
        Vec::new(),
        &EngineConfig::default(),
    );
    let delta =
        shrike_delta::compute_delta(&snapshot, &snapshot, None, "a".into(), "b".into(), false);
    let plan = build_plan(&snapshot, &delta, &EngineConfig::default());
    assert_eq!(plan.totals.symbols, 0);
    assert!(plan.id.starts_with("plan_"));
}

#[test]
fn one_barrel_does_not_merge_every_target_into_one_batch() {
    // A barrel alias belongs with its target, not with every other alias in
    // the barrel. Merging through the barrel would join unrelated files into
    // one oversized unit.
    let mut symbols = Vec::new();
    for index in 0..30 {
        symbols.push(symbol("lib/a.ts", &format!("a{index}"), "function"));
        symbols.push(symbol("lib/b.ts", &format!("b{index}"), "function"));
    }
    symbols.push(symbol("lib/index.ts", "a0", "re-export"));
    symbols.push(symbol("lib/index.ts", "b0", "re-export"));
    let snapshot = snapshot(
        symbols,
        vec![
            re_export("lib/index.ts", "lib/a.ts", "a0"),
            re_export("lib/index.ts", "lib/b.ts", "b0"),
        ],
    );
    let plan = build_plan(&snapshot, &delta(&snapshot), &EngineConfig::default());
    assert_eq!(plan.batches.len(), 2);
    let batch_of = |key: &str| {
        plan.batches
            .iter()
            .position(|batch| batch.symbols.iter().any(|symbol| symbol == key))
    };
    assert_eq!(batch_of("lib/a.ts#a0"), batch_of("lib/index.ts#a0"));
    assert_eq!(batch_of("lib/b.ts#b0"), batch_of("lib/index.ts#b0"));
}

#[test]
fn symbols_in_one_file_share_a_batch() {
    // A type and its same-file users are one unit. Per-symbol packing can fill
    // a batch with the type and leave its users for the next batch, removing
    // the type while annotations still name it.
    let mut symbols = vec![symbol("lib/complexity.ts", "MessageComplexity", "type")];
    symbols.extend(
        (0..50).map(|index| symbol("lib/complexity.ts", &format!("use{index}"), "function")),
    );
    let snapshot = snapshot(symbols, Vec::new());
    let plan = build_plan(&snapshot, &delta(&snapshot), &EngineConfig::default());
    assert_eq!(plan.batches.len(), 1);
    assert_eq!(plan.batches[0].symbols.len(), 51);
}

#[test]
fn repeated_files_do_not_consume_the_file_budget() {
    // One symbol per group means the same file is visited repeatedly. Its
    // repeated appearances must not count against maxFiles, or a small batch
    // flushes long before the 50-symbol cap.
    let symbols: Vec<SymbolRecord> = (0..20)
        .map(|index| symbol("lib/one.ts", &format!("dead{index}"), "function"))
        .collect();
    let snapshot = snapshot(symbols, Vec::new());
    let plan = build_plan(&snapshot, &delta(&snapshot), &EngineConfig::default());
    assert_eq!(plan.batches.len(), 1);
    assert_eq!(plan.batches[0].symbols.len(), 20);
}

#[test]
fn a_dead_declaration_and_its_barrel_specifier_share_a_batch() {
    // `lib/decl.ts` sorts before `lib/index.ts`, and 50 dead declarations fill
    // a batch. A file-ordered partition schedules the barrel specifier in the
    // next batch, which removes the declaration while the live barrel still
    // names it and breaks typecheck between batches.
    let mut symbols: Vec<SymbolRecord> = (0..50)
        .map(|index| symbol("lib/decl.ts", &format!("dead{index}"), "function"))
        .collect();
    symbols.push(symbol("lib/index.ts", "dead0", "re-export"));
    let snapshot = snapshot(
        symbols,
        vec![re_export("lib/index.ts", "lib/decl.ts", "dead0")],
    );
    let plan = build_plan(&snapshot, &delta(&snapshot), &EngineConfig::default());
    let batch_of = |key: &str| {
        plan.batches
            .iter()
            .position(|batch| batch.symbols.iter().any(|symbol| symbol == key))
    };
    assert_eq!(
        batch_of("lib/decl.ts#dead0"),
        batch_of("lib/index.ts#dead0")
    );
    assert!(plan.batches.iter().any(|batch| {
        batch
            .symbols
            .iter()
            .any(|symbol| symbol == "lib/decl.ts#dead0")
    }));
}
