use std::collections::{BTreeMap, BTreeSet};

use shrike_core::Diagnostic;
use shrike_graph::GraphSnapshot;

use crate::attribute::{attribute, comparability};
use crate::cascade::{cascade_closure, partition};
use crate::types::{Cascade, CommitRef, DeltaItem, DeltaReport, HeadRef, Incomparable};

/// Compare two snapshots. Comparability failures produce a report with
/// `incomparable` set and no deletion claims.
#[must_use]
pub fn compute_delta(
    base: &GraphSnapshot,
    head: &GraphSnapshot,
    base_ref: Option<String>,
    base_commit: String,
    head_commit: String,
    dirty: bool,
) -> DeltaReport {
    let mut diagnostics = Vec::new();
    let incomparable = comparability(base, head);
    if let Some(reason) = &incomparable {
        diagnostics.push(Diagnostic::warning("incomparable", reason.clone()));
        return DeltaReport {
            schema_version: "1.0".to_string(),
            base: CommitRef {
                r#ref: base_ref,
                commit: base_commit,
            },
            head: HeadRef {
                commit: head_commit,
                dirty,
            },
            newly_dead: Vec::new(),
            newly_dead_files: Vec::new(),
            cascade: Cascade {
                symbols: 0,
                files: 0,
                lines: 0,
            },
            batches: Vec::new(),
            incomparable: Some(Incomparable {
                reason: reason.clone(),
                base: base.graph_hash.clone(),
                head: head.graph_hash.clone(),
            }),
            diagnostics,
        };
    }

    let base_live: BTreeSet<&str> = base.live.iter().map(String::as_str).collect();
    let head_live: BTreeSet<&str> = head.live.iter().map(String::as_str).collect();
    let base_symbols: BTreeMap<&str, &shrike_graph::SymbolRecord> = base
        .symbols
        .iter()
        .map(|symbol| (symbol.key.as_str(), symbol))
        .collect();
    let head_symbols: BTreeMap<&str, &shrike_graph::SymbolRecord> = head
        .symbols
        .iter()
        .map(|symbol| (symbol.key.as_str(), symbol))
        .collect();
    let base_witnesses: BTreeMap<&str, &[String]> = base
        .witnesses
        .iter()
        .map(|witness| (witness.symbol.as_str(), witness.path.as_slice()))
        .collect();
    let head_edges: BTreeSet<(&str, &str, &str)> = head
        .edges
        .iter()
        .map(|edge| {
            (
                edge.source.as_str(),
                edge.target.as_str(),
                edge.kind.as_str(),
            )
        })
        .collect();
    let base_edges: BTreeSet<(&str, &str, &str)> = base
        .edges
        .iter()
        .map(|edge| {
            (
                edge.source.as_str(),
                edge.target.as_str(),
                edge.kind.as_str(),
            )
        })
        .collect();

    let mut items = Vec::new();
    let head_candidates: BTreeSet<&str> = head.candidates.iter().map(String::as_str).collect();
    for symbol in &head.symbols {
        if head_live.contains(symbol.key.as_str()) {
            continue;
        }
        if !head_candidates.contains(symbol.key.as_str()) {
            continue;
        }
        let Some(base_symbol) = base_symbols.get(symbol.key.as_str()) else {
            continue;
        };
        if !base_live.contains(symbol.key.as_str()) {
            continue;
        }
        let witness: Vec<String> = base_witnesses
            .get(symbol.key.as_str())
            .map(|path| path.to_vec())
            .unwrap_or_default();
        let (trigger, confidence) = attribute(
            &witness,
            base,
            head,
            &base_edges,
            &head_edges,
            &base_symbols,
            &head_symbols,
        );
        let _ = base_symbol;
        items.push(DeltaItem {
            symbol: symbol.key.clone(),
            kind: symbol.kind.clone(),
            file: symbol.file.clone(),
            last_witness: witness,
            trigger,
            confidence,
            estimated_lines: symbol.lines,
        });
    }
    items.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.symbol.cmp(&b.symbol)));

    let delta_symbols: BTreeSet<String> = items.iter().map(|item| item.symbol.clone()).collect();
    let cascade_set = cascade_closure(head, &delta_symbols);
    let cascade = Cascade {
        symbols: cascade_set.len(),
        files: cascade_set
            .iter()
            .filter_map(|key| head.symbol(key))
            .map(|symbol| symbol.file.clone())
            .collect::<BTreeSet<_>>()
            .len(),
        lines: cascade_set
            .iter()
            .filter_map(|key| head.symbol(key))
            .map(|symbol| symbol.lines)
            .sum(),
    };

    let newly_dead_files: Vec<String> = head
        .files
        .iter()
        .filter(|file| {
            head.symbols
                .iter()
                .any(|symbol| symbol.file == file.path && delta_symbols.contains(&symbol.key))
                && !head.symbols.iter().any(|symbol| {
                    symbol.file == file.path && head_live.contains(symbol.key.as_str())
                })
        })
        .map(|file| file.path.clone())
        .collect();

    let batches = partition(head, &cascade_set);

    DeltaReport {
        schema_version: "1.0".to_string(),
        base: CommitRef {
            r#ref: base_ref,
            commit: base_commit,
        },
        head: HeadRef {
            commit: head_commit,
            dirty,
        },
        newly_dead: items,
        newly_dead_files,
        cascade,
        batches,
        incomparable: None,
        diagnostics,
    }
}
