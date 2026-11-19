#![forbid(unsafe_code)]
//! Whole-graph orphan delta: newly dead symbols, lost witnesses, trigger
//! attribution, cascade closure, and batch partitioning.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use shrike_core::{PROTOCOL_VERSION, TOOL_VERSION, hash_json};
use shrike_graph::GraphSnapshot;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRef {
    pub r#ref: Option<String>,
    pub commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadRef {
    pub commit: String,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub file: String,
    pub kind: String,
    pub change: String,
    pub certainty: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaItem {
    pub symbol: String,
    pub kind: String,
    pub file: String,
    pub last_witness: Vec<String>,
    pub trigger: Trigger,
    pub confidence: String,
    pub estimated_lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cascade {
    pub symbols: usize,
    pub files: usize,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub id: String,
    pub symbols: usize,
    pub files: usize,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incomparable {
    pub reason: String,
    pub base: String,
    pub head: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaReport {
    pub schema_version: String,
    pub base: CommitRef,
    pub head: HeadRef,
    pub newly_dead: Vec<DeltaItem>,
    pub newly_dead_files: Vec<String>,
    pub cascade: Cascade,
    pub batches: Vec<BatchSummary>,
    pub incomparable: Option<Incomparable>,
    pub diagnostics: Vec<shrike_core::Diagnostic>,
}

impl DeltaReport {
    #[must_use]
    pub fn hash(&self) -> String {
        hash_json(self)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.newly_dead.is_empty() && self.newly_dead_files.is_empty()
    }
}

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
        diagnostics.push(shrike_core::Diagnostic::warning(
            "incomparable",
            reason.clone(),
        ));
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

fn comparability(base: &GraphSnapshot, head: &GraphSnapshot) -> Option<String> {
    if base.config_hash != head.config_hash {
        return Some("configuration hash changed between base and head".to_string());
    }
    if base.resolver_hash != head.resolver_hash {
        return Some("resolver configuration changed between base and head".to_string());
    }
    if base.protocol_version != head.protocol_version || base.protocol_version != PROTOCOL_VERSION {
        return Some("protocol version changed between base and head".to_string());
    }
    if base.tool_version != head.tool_version || base.tool_version != TOOL_VERSION {
        return Some("tool version changed between base and head".to_string());
    }
    if base.schema_version != head.schema_version {
        return Some("snapshot schema version changed between base and head".to_string());
    }
    None
}

fn attribute(
    witness: &[String],
    base: &GraphSnapshot,
    head: &GraphSnapshot,
    base_edges: &BTreeSet<(&str, &str, &str)>,
    head_edges: &BTreeSet<(&str, &str, &str)>,
    base_symbols: &BTreeMap<&str, &shrike_graph::SymbolRecord>,
    head_symbols: &BTreeMap<&str, &shrike_graph::SymbolRecord>,
) -> (Trigger, String) {
    for pair in witness.windows(2) {
        let source = pair[0].as_str();
        let target = pair[1].as_str();
        let existing: Vec<&str> = base_edges
            .iter()
            .filter(|(from, to, _)| *from == source && *to == target)
            .map(|(_, _, kind)| *kind)
            .collect();
        if existing.is_empty() {
            continue;
        }
        let still_there = existing
            .iter()
            .any(|kind| head_edges.contains(&(source, target, kind)));
        if !still_there {
            let kind = existing[0];
            // The trigger is the owner of the removed edge: the caller that
            // stopped importing/referencing the now-dead symbol. Fall back to
            // the target when the source has no symbol record.
            let source_file = base_symbols
                .get(source)
                .map(|symbol| symbol.file.clone())
                .or_else(|| head_symbols.get(source).map(|symbol| symbol.file.clone()));
            let file = source_file
                .clone()
                .or_else(|| base_symbols.get(target).map(|symbol| symbol.file.clone()))
                .or_else(|| head_symbols.get(target).map(|symbol| symbol.file.clone()))
                .unwrap_or_default();
            let source_was_entry = source_file.as_ref().is_some_and(|path| {
                base.entries.iter().any(|entry| {
                    entry.route.is_some()
                        && base_symbols
                            .get(entry.symbol.as_str())
                            .is_some_and(|symbol| &symbol.file == path)
                })
            });
            let source_deleted = source_file
                .as_ref()
                .is_some_and(|path| !head.files.iter().any(|head_file| &head_file.path == path));
            let trigger_kind = if source_deleted && source_was_entry {
                "removed-route"
            } else {
                match kind {
                    "jsx" => "removed-jsx",
                    "re-export" => "removed-re-export",
                    "dynamic" => "changed-dynamic",
                    "import" => "removed-import",
                    _ => "removed-call",
                }
            };
            return (
                Trigger {
                    file,
                    kind: trigger_kind.to_string(),
                    change: format!("removed {kind} edge from {source} to {target}"),
                    certainty: "certain".to_string(),
                },
                "certain".to_string(),
            );
        }
    }
    // A changed file on the witness is the closest explanation.
    for key in witness {
        if let Some(base_symbol) = base_symbols.get(key.as_str()) {
            let changed = match head_symbols.get(key.as_str()) {
                None => true,
                Some(head_symbol) => {
                    let base_file = base.files.iter().find(|file| file.path == base_symbol.file);
                    let head_file = head.files.iter().find(|file| file.path == head_symbol.file);
                    match (base_file, head_file) {
                        (Some(a), Some(b)) => a.content_hash != b.content_hash,
                        _ => true,
                    }
                }
            };
            if changed {
                let kind = if head_symbols.get(key.as_str()).is_none() {
                    "removed-route"
                } else {
                    "indirect"
                };
                return (
                    Trigger {
                        file: base_symbol.file.clone(),
                        kind: kind.to_string(),
                        change: format!("changed symbol {key} on the last witness"),
                        certainty: "probable".to_string(),
                    },
                    "probable".to_string(),
                );
            }
        }
    }
    (
        Trigger {
            file: String::new(),
            kind: "indirect".to_string(),
            change: "no single edge explains the lost witness".to_string(),
            certainty: "probable".to_string(),
        },
        "probable".to_string(),
    )
}

fn cascade_closure(head: &GraphSnapshot, seeds: &BTreeSet<String>) -> BTreeSet<String> {
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

fn partition(head: &GraphSnapshot, symbols: &BTreeSet<String>) -> Vec<BatchSummary> {
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
    let mut current_symbols = 0usize;
    let flush = |files: &mut Vec<String>, count: &mut usize, batches: &mut Vec<BatchSummary>| {
        if files.is_empty() {
            return;
        }
        files.sort();
        let lines: u32 = files
            .iter()
            .filter_map(|file| head.files.iter().find(|record| &record.path == file))
            .map(|file| file.lines)
            .sum();
        let id = format!("batch_{}", batches.len() + 1);
        batches.push(BatchSummary {
            id,
            symbols: *count,
            files: files.len(),
            lines,
        });
        files.clear();
        *count = 0;
    };
    for (file, keys) in by_file {
        if current_symbols + keys.len() > 50 || current_files.len() >= 10 {
            flush(&mut current_files, &mut current_symbols, &mut batches);
        }
        current_symbols += keys.len();
        current_files.push(file);
    }
    flush(&mut current_files, &mut current_symbols, &mut batches);
    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshots_are_comparable_and_empty() {
        let snapshot = shrike_graph::build_snapshot(
            std::path::Path::new("."),
            Vec::new(),
            &shrike_core::EngineConfig::default(),
        );
        let delta = compute_delta(&snapshot, &snapshot, None, "a".into(), "b".into(), false);
        assert!(delta.is_empty());
        assert!(delta.incomparable.is_none());
    }
}
