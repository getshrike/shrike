use std::collections::{BTreeMap, BTreeSet};

use shrike_core::{PROTOCOL_VERSION, TOOL_VERSION};
use shrike_graph::{GraphSnapshot, SymbolRecord};

use crate::types::Trigger;

pub(crate) fn comparability(base: &GraphSnapshot, head: &GraphSnapshot) -> Option<String> {
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

pub(crate) fn attribute(
    witness: &[String],
    base: &GraphSnapshot,
    head: &GraphSnapshot,
    base_edges: &BTreeSet<(&str, &str, &str)>,
    head_edges: &BTreeSet<(&str, &str, &str)>,
    base_symbols: &BTreeMap<&str, &SymbolRecord>,
    head_symbols: &BTreeMap<&str, &SymbolRecord>,
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
                    "dynamic" | "worker" => "changed-dynamic",
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
