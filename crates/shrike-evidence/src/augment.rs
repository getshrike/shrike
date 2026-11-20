use std::collections::{BTreeMap, BTreeSet, VecDeque};

use shrike_core::Span;
use shrike_graph::{EdgeRecord, GraphSnapshot, WitnessRecord};

use crate::types::{BuildEvidence, BuildJoin};

/// Apply build evidence to a snapshot's liveness.
///
/// Presence in the build graph is a certain observation: the module shipped.
/// We seed every symbol in an evidenced file plus explicitly named symbols, then
/// follow known edges so exclusive dependencies of a build-live symbol are not
/// reported dead. Unknown edges are not traversed here; static analysis already
/// widens liveness for their scopes, and traversing them again would only hide
/// real findings.
#[must_use]
pub fn augment_snapshot(
    snapshot: &GraphSnapshot,
    evidence: &BuildEvidence,
    join: &BuildJoin,
) -> GraphSnapshot {
    if join.status != "full" && join.status != "partial" {
        return snapshot.clone();
    }

    let mut files: BTreeSet<&str> = evidence.files.iter().map(String::as_str).collect();
    let mut symbols: BTreeSet<&str> = evidence.symbols.iter().map(String::as_str).collect();
    for route in &evidence.routes {
        files.extend(route.files.iter().map(String::as_str));
        symbols.extend(route.symbols.iter().map(String::as_str));
    }

    let mut live: BTreeSet<String> = snapshot.live.iter().cloned().collect();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut queued: BTreeSet<String> = BTreeSet::new();
    for symbol in &snapshot.symbols {
        if files.contains(symbol.file.as_str()) || symbols.contains(symbol.key.as_str()) {
            live.insert(symbol.key.clone());
            if queued.insert(symbol.key.clone()) {
                queue.push_back(symbol.key.clone());
            }
        }
    }

    // A build-evidenced file is a module root: its imports are part of the
    // shipped graph even when the static graph had no entry path to it.
    for file in &files {
        let module = format!("{file}#__module__");
        live.insert(module.clone());
        if queued.insert(module.clone()) {
            queue.push_back(module);
        }
    }
    for symbol in &snapshot.symbols {
        if symbols.contains(symbol.key.as_str()) {
            let module = format!("{}#__module__", symbol.file);
            live.insert(module.clone());
            if queued.insert(module.clone()) {
                queue.push_back(module);
            }
        }
    }

    // Client-reference records are certain build edges: the client bundle
    // references the server action or client component. They can only add
    // liveness, never remove it.
    let mut edges = snapshot.edges.clone();
    for build_edge in &join.edges {
        if !edges.iter().any(|edge| {
            edge.source == build_edge.from && edge.target == build_edge.to && edge.kind == "build"
        }) {
            edges.push(EdgeRecord {
                source: build_edge.from.clone(),
                target: build_edge.to.clone(),
                kind: "build".to_string(),
                certainty: "certain".to_string(),
                file: build_edge
                    .from
                    .split('#')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                span: Span::default(),
            });
        }
        for endpoint in [&build_edge.from, &build_edge.to] {
            live.insert(endpoint.clone());
            if queued.insert(endpoint.clone()) {
                queue.push_back(endpoint.clone());
            }
        }
    }
    edges.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.kind.cmp(&b.kind))
    });

    let mut outgoing: BTreeMap<&str, Vec<&EdgeRecord>> = BTreeMap::new();
    for edge in &edges {
        outgoing.entry(edge.source.as_str()).or_default().push(edge);
    }
    while let Some(source) = queue.pop_front() {
        let Some(edges) = outgoing.get(source.as_str()) else {
            continue;
        };
        let mut targets = edges.clone();
        targets.sort_by(|a, b| a.target.cmp(&b.target).then_with(|| a.kind.cmp(&b.kind)));
        for edge in targets {
            if edge.certainty == "unknown" {
                continue;
            }
            if live.insert(edge.target.clone()) {
                queue.push_back(edge.target.clone());
            }
        }
    }

    let mut candidates = snapshot.candidates.clone();
    candidates.retain(|key| !live.contains(key));

    let mut witnesses = snapshot.witnesses.clone();
    let existing: BTreeSet<String> = witnesses.iter().map(|item| item.symbol.clone()).collect();
    for key in &live {
        if !existing.contains(key) {
            witnesses.push(WitnessRecord {
                symbol: key.clone(),
                path: vec![key.clone()],
            });
        }
    }
    witnesses.sort_by(|a, b| a.symbol.cmp(&b.symbol));

    let mut proofs = snapshot.proofs.clone();
    proofs.retain(|proof| !live.contains(&proof.symbol));

    let mut augmented = snapshot.clone();
    augmented.edges = edges;
    augmented.live = live.into_iter().collect();
    augmented.candidates = candidates;
    augmented.witnesses = witnesses;
    augmented.proofs = proofs;
    shrike_graph::rehash_snapshot(&mut augmented);
    augmented
}
