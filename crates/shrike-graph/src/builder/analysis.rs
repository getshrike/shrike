use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use super::Builder;
use crate::types::{EdgeRecord, ProofRecord, WitnessRecord};

impl<'a> Builder<'a> {
    pub(super) fn reachability(&self) -> (BTreeSet<String>, Vec<WitnessRecord>, Vec<ProofRecord>) {
        let mut outgoing: HashMap<&str, Vec<&EdgeRecord>> = HashMap::new();
        let mut incoming: HashMap<&str, Vec<&EdgeRecord>> = HashMap::new();
        for edge in &self.edges {
            outgoing.entry(&edge.source).or_default().push(edge);
            incoming.entry(&edge.target).or_default().push(edge);
        }
        // Every live symbol expands once, so sort each adjacency list once
        // rather than cloning and sorting it on every visit.
        for edges in outgoing.values_mut() {
            edges.sort_by(|a, b| a.target.cmp(&b.target).then_with(|| a.kind.cmp(&b.kind)));
        }
        // BFS membership is a hash set: every edge target probes the set, so
        // hashing is cheaper than a tree search. The returned set is rebuilt in
        // sorted order once, after the walk.
        let mut live: HashSet<String> = HashSet::new();
        // Parent links borrow from `self.edges`: a witness path only needs the
        // source and kind of the edge that first reached the symbol. Cloning
        // them here allocated three strings per live symbol.
        let mut parent: HashMap<&str, (&str, &str)> = HashMap::new();
        // The worklist borrows keys too; the `live` set owns the one copy each
        // symbol needs for the output.
        let mut queue: VecDeque<&str> = VecDeque::new();
        let mut entries: Vec<&crate::types::EntryRecord> = self.entries.iter().collect();
        entries.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        for entry in entries {
            if live.insert(entry.symbol.clone()) {
                queue.push_back(entry.symbol.as_str());
            }
        }
        // Unknown edges widen the live set in their bounded scope. Widened
        // symbols are seeded into the queue like entries: a symbol that is live
        // must expand its own references, or its helpers are misreported as
        // dead.
        for unknown in &self.unknowns {
            match unknown.scope.as_str() {
                "repo" => {
                    for symbol in &self.symbols {
                        if live.insert(symbol.key.clone()) {
                            queue.push_back(symbol.key.as_str());
                        }
                    }
                }
                "package" => {
                    if let Some(package) = &unknown.package {
                        for symbol in &self.symbols {
                            if self.package_of.get(&symbol.file) == Some(package)
                                && live.insert(symbol.key.clone())
                            {
                                queue.push_back(symbol.key.as_str());
                            }
                        }
                    }
                }
                "module" => {
                    if let Some(file) = self.files.iter().find(|file| file.path == unknown.file) {
                        if let Some(exports) = self.exports.get(&file.path) {
                            for key in exports.values() {
                                if live.insert(key.clone()) {
                                    queue.push_back(key.as_str());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        while let Some(source) = queue.pop_front() {
            if let Some(edges) = outgoing.get(source) {
                for edge in edges {
                    if live.insert(edge.target.clone()) {
                        parent.insert(
                            edge.target.as_str(),
                            (edge.source.as_str(), edge.kind.as_str()),
                        );
                        queue.push_back(edge.target.as_str());
                    }
                }
            }
        }
        let mut witnesses = Vec::new();
        for symbol in &live {
            let mut path = vec![symbol.clone()];
            let mut current = symbol.as_str();
            let mut guard = 0;
            while let Some(&(next, _)) = parent.get(current) {
                path.push(next.to_string());
                current = next;
                guard += 1;
                if guard > 4096 {
                    break;
                }
            }
            path.reverse();
            witnesses.push(WitnessRecord {
                symbol: symbol.clone(),
                path,
            });
        }
        witnesses.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        let mut proofs = Vec::new();
        for symbol in &self.symbols {
            if live.contains(&symbol.key) {
                continue;
            }
            let incoming_edges = incoming.get(symbol.key.as_str()).map_or(0, Vec::len);
            let outgoing_edges = outgoing.get(symbol.key.as_str()).map_or(0, Vec::len);
            proofs.push(ProofRecord {
                symbol: symbol.key.clone(),
                reason: if symbol.public_api {
                    "blocked by public API".to_string()
                } else if symbol.generated {
                    "blocked by generated file".to_string()
                } else if symbol.entry {
                    "entry".to_string()
                } else if incoming_edges == 0 && outgoing_edges == 0 {
                    "no witness; isolated symbol".to_string()
                } else {
                    "no witness from any entry".to_string()
                },
                incoming: incoming
                    .get(symbol.key.as_str())
                    .map_or_else(Vec::new, |edges| {
                        edges.iter().map(|edge| edge.source.clone()).collect()
                    }),
                outgoing: outgoing
                    .get(symbol.key.as_str())
                    .map_or_else(Vec::new, |edges| {
                        edges.iter().map(|edge| edge.target.clone()).collect()
                    }),
            });
        }
        (live.into_iter().collect(), witnesses, proofs)
    }

    /// Exported symbols that are live only because their own file references
    /// them. A cross-file edge proves external use; same-file `re-export`
    /// aliases propagate that proof to the local symbol they expose. A type
    /// imported externally also makes the types in its shape externally
    /// visible, so type-to-type references propagate too. Value references do
    /// not: a helper used by an externally called function stays internal and
    /// can be de-exported.
    pub(super) fn internal_only(&self, live: &BTreeSet<String>) -> Vec<String> {
        // A cross-file use only proves external visibility when the using
        // symbol is live. Imports and re-exports from dead modules cannot
        // reach the symbol today, so they must not keep a same-file-only
        // export out of `internalOnly`. Unknown source keys fail closed and
        // count as external.
        let known: HashSet<&str> = self
            .symbols
            .iter()
            .map(|symbol| symbol.key.as_str())
            .collect();
        let counts_as_external = |key: &str| !known.contains(key) || live.contains(key);
        let type_only: BTreeSet<&str> = self
            .symbols
            .iter()
            .filter(|symbol| symbol.type_only)
            .map(|symbol| symbol.key.as_str())
            .collect();
        // Same-file re-export and type-to-type reference edges propagate
        // external visibility. Build that adjacency and the external set in
        // one pass over the edges, borrowing keys instead of cloning them:
        // this set and its worklist can hold most of the graph.
        let mut external: HashSet<&str> = HashSet::new();
        let mut propagates: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &self.edges {
            if file_of(&edge.source) != file_of(&edge.target) {
                if counts_as_external(&edge.source) {
                    external.insert(edge.target.as_str());
                }
                continue;
            }
            let propagates_edge = edge.kind == "re-export"
                || (edge.kind == "reference"
                    && type_only.contains(edge.source.as_str())
                    && type_only.contains(edge.target.as_str()));
            if propagates_edge {
                propagates
                    .entry(edge.source.as_str())
                    .or_default()
                    .push(edge.target.as_str());
            }
        }
        let mut queue: VecDeque<&str> = external.iter().copied().collect();
        while let Some(source) = queue.pop_front() {
            if !counts_as_external(source) {
                continue;
            }
            if let Some(targets) = propagates.get(source) {
                for target in targets {
                    if external.insert(*target) {
                        queue.push_back(*target);
                    }
                }
            }
        }
        self.symbols
            .iter()
            .filter(|symbol| {
                symbol.exported
                    && !symbol.entry
                    && !symbol.public_api
                    && !symbol.generated
                    && live.contains(&symbol.key)
                    && !external.contains(symbol.key.as_str())
            })
            .map(|symbol| symbol.key.clone())
            .collect()
    }

    pub(super) fn sccs(&self) -> Vec<Vec<String>> {
        // Tarjan over symbol keys, iterative to avoid recursion limits.
        let keys: Vec<&str> = self
            .symbols
            .iter()
            .map(|symbol| symbol.key.as_str())
            .collect();
        let index_of: HashMap<&str, usize> = keys
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect();
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); keys.len()];
        for edge in &self.edges {
            if let (Some(source), Some(target)) = (
                index_of.get(edge.source.as_str()),
                index_of.get(edge.target.as_str()),
            ) {
                adjacency[*source].push(*target);
            }
        }
        for targets in &mut adjacency {
            targets.sort_unstable();
            targets.dedup();
        }
        let mut indices = vec![usize::MAX; keys.len()];
        let mut low = vec![0usize; keys.len()];
        let mut on_stack = vec![false; keys.len()];
        let mut stack = Vec::new();
        let mut next_index = 0usize;
        let mut components = Vec::new();
        for start in 0..keys.len() {
            if indices[start] != usize::MAX {
                continue;
            }
            let mut call_stack: Vec<(usize, usize)> = vec![(start, 0)];
            while let Some((node, edge_index)) = call_stack.pop() {
                if edge_index == 0 {
                    indices[node] = next_index;
                    low[node] = next_index;
                    next_index += 1;
                    stack.push(node);
                    on_stack[node] = true;
                }
                if edge_index < adjacency[node].len() {
                    let target = adjacency[node][edge_index];
                    call_stack.push((node, edge_index + 1));
                    if indices[target] == usize::MAX {
                        call_stack.push((target, 0));
                    } else if on_stack[target] {
                        low[node] = low[node].min(indices[target]);
                    }
                } else {
                    if low[node] == indices[node] {
                        let mut component = Vec::new();
                        while let Some(member) = stack.pop() {
                            on_stack[member] = false;
                            component.push(keys[member].to_string());
                            if member == node {
                                break;
                            }
                        }
                        component.sort();
                        components.push(component);
                    }
                    if let Some((parent, _)) = call_stack.last() {
                        low[*parent] = low[*parent].min(low[node]);
                    }
                }
            }
        }
        components.sort_by(|a, b| {
            a.first()
                .cmp(&b.first())
                .then_with(|| a.len().cmp(&b.len()))
        });
        components
    }
}

fn file_of(symbol_key: &str) -> &str {
    symbol_key
        .split_once('#')
        .map_or(symbol_key, |(file, _)| file)
}
