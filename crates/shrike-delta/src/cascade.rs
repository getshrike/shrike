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
    let mut units: Vec<(String, Vec<String>, Vec<String>)> =
        shrike_graph::deletion_groups(head, symbols)
            .into_iter()
            .filter_map(|group| {
                let mut keys: Vec<String> = group
                    .into_iter()
                    .filter(|key| head.symbol(key).is_some())
                    .collect();
                if keys.is_empty() {
                    return None;
                }
                keys.sort();
                let mut files: Vec<String> = keys
                    .iter()
                    .filter_map(|key| head.symbol(key))
                    .map(|symbol| symbol.file.clone())
                    .collect();
                files.sort();
                files.dedup();
                Some((files.first().cloned().unwrap_or_default(), keys, files))
            })
            .collect();
    units.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let mut batches = Vec::new();
    let mut current_files: Vec<String> = Vec::new();
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
    for (_sort, keys, files) in units {
        let new_files = files
            .iter()
            .filter(|file| !current_files.contains(*file))
            .count();
        if !current_symbols.is_empty()
            && (current_symbols.len() + keys.len() > 50 || current_files.len() + new_files > 10)
        {
            flush(&mut current_symbols, &mut current_files, &mut batches);
        }
        current_symbols.extend(keys);
        for file in files {
            if !current_files.contains(&file) {
                current_files.push(file);
            }
        }
    }
    flush(&mut current_symbols, &mut current_files, &mut batches);
    batches
}
