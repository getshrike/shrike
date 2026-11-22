use std::collections::{BTreeMap, BTreeSet, VecDeque};

use shrike_graph::GraphSnapshot;

use crate::types::BatchSummary;

pub(crate) fn cascade_closure(head: &GraphSnapshot, seeds: &BTreeSet<String>) -> BTreeSet<String> {
    let mut reverse: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in &head.edges {
        reverse.entry(&edge.target).or_default().push(&edge.source);
    }
    let candidates: BTreeSet<&str> = head.candidates.iter().map(String::as_str).collect();
    let mut result: BTreeSet<String> = seeds.clone();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    while let Some(symbol) = queue.pop_front() {
        if let Some(dependents) = reverse.get(symbol.as_str()) {
            for dependent in dependents {
                if candidates.contains(dependent) && result.insert((*dependent).to_string()) {
                    queue.push_back((*dependent).to_string());
                }
            }
        }
    }
    result
}

pub(crate) fn partition(head: &GraphSnapshot, symbols: &BTreeSet<String>) -> Vec<BatchSummary> {
    let mut by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for key in symbols {
        if let Some(symbol) = head.symbol(key) {
            by_file
                .entry(symbol.file.clone())
                .or_default()
                .insert(key.clone());
        }
    }
    let mut batches = Vec::new();
    let mut current_files = Vec::new();
    let mut current_symbols: Vec<String> = Vec::new();
    let flush =
        |symbols: &mut Vec<String>, files: &mut Vec<String>, batches: &mut Vec<BatchSummary>| {
            if symbols.is_empty() {
                return;
            }
            symbols.sort();
            symbols.dedup();
            files.sort();
            files.dedup();
            // Estimate deletion impact from the orphan spans, not from every line
            // in the file. A one-line orphan in a large file removes one line.
            let lines: u32 = symbols
                .iter()
                .filter_map(|key| head.symbol(key))
                .map(|symbol| symbol.lines)
                .sum();
            let id = format!("batch_{}", batches.len() + 1);
            batches.push(BatchSummary {
                id,
                symbols: symbols.len(),
                files: files.len(),
                lines,
            });
            symbols.clear();
            files.clear();
        };
    for (file, keys) in by_file {
        if !current_symbols.is_empty()
            && (current_symbols.len() + keys.len() > 50 || current_files.len() >= 10)
        {
            flush(&mut current_symbols, &mut current_files, &mut batches);
        }
        current_symbols.extend(keys);
        current_files.push(file);
    }
    flush(&mut current_symbols, &mut current_files, &mut batches);
    batches
}
