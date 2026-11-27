use std::collections::{BTreeMap, BTreeSet};

use shrike_core::{EngineConfig, hash_json, short_hash};
use shrike_delta::DeltaReport;
use shrike_graph::GraphSnapshot;

use crate::types::{BlockedItem, DeletionPlan, PlanBatch, PlanPolicy, PlanTotals};

#[must_use]
pub fn build_plan(
    snapshot: &GraphSnapshot,
    delta: &DeltaReport,
    config: &EngineConfig,
) -> DeletionPlan {
    let mut blocked: Vec<BlockedItem> = snapshot
        .proofs
        .iter()
        .filter(|proof| proof.reason.contains("public API") || proof.reason.contains("generated"))
        .map(|proof| BlockedItem {
            symbol: proof.symbol.clone(),
            reason: proof.reason.clone(),
        })
        .collect();
    blocked.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    blocked.dedup_by(|a, b| a.symbol == b.symbol);

    let mut selected: BTreeSet<String> = delta
        .newly_dead
        .iter()
        .map(|item| item.symbol.clone())
        .collect();
    // Include cascade candidates that are reachable only through the delta.
    let delta_symbols: BTreeSet<String> = selected.clone();
    let mut reverse: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in &snapshot.edges {
        reverse.entry(&edge.target).or_default().push(&edge.source);
    }
    let candidates: BTreeSet<&str> = snapshot.candidates.iter().map(String::as_str).collect();
    let by_key: BTreeMap<&str, &shrike_graph::SymbolRecord> = snapshot
        .symbols
        .iter()
        .map(|symbol| (symbol.key.as_str(), symbol))
        .collect();
    let mut queue: Vec<String> = delta_symbols.iter().cloned().collect();
    while let Some(symbol) = queue.pop() {
        if let Some(dependents) = reverse.get(symbol.as_str()) {
            for dependent in dependents {
                if candidates.contains(dependent) && selected.insert((*dependent).to_string()) {
                    queue.push((*dependent).to_string());
                }
            }
        }
    }

    let groups = shrike_graph::deletion_groups(snapshot, &selected);
    let mut units: Vec<(String, Vec<String>, Vec<String>)> = groups
        .into_iter()
        .filter_map(|group| {
            let mut symbols: Vec<String> = group
                .into_iter()
                .filter(|key| by_key.contains_key(key.as_str()))
                .collect();
            if symbols.is_empty() {
                return None;
            }
            symbols.sort();
            let mut files: Vec<String> = symbols
                .iter()
                .filter_map(|key| by_key.get(key.as_str()).copied())
                .map(|symbol| symbol.file.clone())
                .collect();
            files.sort();
            files.dedup();
            Some((files.first().cloned().unwrap_or_default(), symbols, files))
        })
        .collect();
    units.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let max_symbols = config.batch_policy.max_symbols.max(1) as usize;
    let max_files = config.batch_policy.max_files.max(1) as usize;
    let mut batches = Vec::new();
    let mut current_symbols: Vec<String> = Vec::new();
    let mut current_files: Vec<String> = Vec::new();
    let flush =
        |symbols: &mut Vec<String>, files: &mut Vec<String>, batches: &mut Vec<PlanBatch>| {
            if symbols.is_empty() {
                return;
            }
            symbols.sort();
            symbols.dedup();
            files.sort();
            files.dedup();
            let lines: u32 = symbols
                .iter()
                .filter_map(|key| by_key.get(key.as_str()).copied())
                .map(|symbol| symbol.lines)
                .sum();
            batches.push(PlanBatch {
                id: format!("batch_{}", batches.len() + 1),
                symbols: std::mem::take(symbols),
                files: std::mem::take(files),
                lines,
                patch: String::new(),
            });
        };
    for (_sort, symbols, files) in units {
        let new_files = files
            .iter()
            .filter(|file| !current_files.contains(*file))
            .count();
        if !current_symbols.is_empty()
            && (current_symbols.len() + symbols.len() > max_symbols
                || current_files.len() + new_files > max_files)
        {
            flush(&mut current_symbols, &mut current_files, &mut batches);
        }
        current_symbols.extend(symbols);
        for file in files {
            if !current_files.contains(&file) {
                current_files.push(file);
            }
        }
    }
    flush(&mut current_symbols, &mut current_files, &mut batches);

    let total_symbols: usize = batches.iter().map(|batch| batch.symbols.len()).sum();
    let total_files: usize = batches
        .iter()
        .flat_map(|batch| batch.files.iter())
        .collect::<BTreeSet<_>>()
        .len();
    let total_lines: u32 = batches.iter().map(|batch| batch.lines).sum();
    let delta_hash = delta.hash();
    let id_input = hash_json(&(
        delta_hash.clone(),
        &batches,
        &blocked,
        &config.config_hash(),
    ));
    DeletionPlan {
        schema_version: "1.0".to_string(),
        id: format!("plan_{}", short_hash(&id_input)),
        base: delta.base.clone(),
        head: delta.head.clone(),
        delta_hash,
        totals: PlanTotals {
            symbols: total_symbols,
            files: total_files,
            lines: total_lines,
        },
        batches,
        blocked,
        policy: PlanPolicy {
            entry_policy: match config.entry_policy {
                shrike_core::EntryPolicy::Local => "local".to_string(),
                shrike_core::EntryPolicy::Production => "production".to_string(),
            },
            max_symbols: config.batch_policy.max_symbols,
            max_files: config.batch_policy.max_files,
            allow_oversized: config.batch_policy.allow_oversized,
        },
        diagnostics: delta.diagnostics.clone(),
    }
}
