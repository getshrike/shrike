use std::collections::{BTreeMap, BTreeSet};

use shrike_graph::GraphSnapshot;

use crate::diagnostics::diagnostics_for;
use crate::resolve::{is_project_evidence_item, normalize_build_path, resolve_build_item};
use crate::status::status_for_evidence;
use crate::types::{BuildBytes, BuildEdge, BuildEvidence, BuildJoin, BuildRouteJoin};
use crate::verdicts::{candidate_verdicts, symbol_or_module};

#[must_use]
pub fn join_build(snapshot: &GraphSnapshot, evidence: &BuildEvidence) -> BuildJoin {
    let known_files: BTreeSet<&str> = snapshot
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let known_symbols: BTreeSet<&str> = snapshot
        .symbols
        .iter()
        .map(|symbol| symbol.key.as_str())
        .collect();

    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for item in evidence
        .files
        .iter()
        .chain(evidence.symbols.iter())
        .chain(
            evidence
                .routes
                .iter()
                .flat_map(|route| route.files.iter().chain(route.symbols.iter())),
        )
        .chain(
            evidence
                .client_references
                .iter()
                .flat_map(|reference| [&reference.from, &reference.to]),
        )
    {
        if is_project_evidence_item(item) {
            wanted.insert(item.clone());
        }
    }

    let mut matched: BTreeSet<String> = BTreeSet::new();
    let mut unmatched = Vec::new();
    for item in &wanted {
        if let Some(resolved) =
            resolve_build_item(item, &known_files, &known_symbols, &evidence.module_ids)
        {
            matched.insert(resolved);
        } else {
            unmatched.push(item.clone());
        }
    }
    unmatched.sort();
    unmatched.dedup();

    let joined = matched.len();
    let total = joined + unmatched.len();
    let coverage = if total == 0 {
        0.0
    } else {
        joined as f64 / total as f64
    };

    let status = status_for_evidence(total, coverage, evidence);
    let diagnostics = diagnostics_for(status, total, coverage, &unmatched, evidence);

    // Route membership is resolved through the same normalizer as the join.
    let mut route_members: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut route_runtime: BTreeMap<String, String> = BTreeMap::new();
    for route in &evidence.routes {
        route_runtime.insert(
            route.route.clone(),
            route.runtime.clone().unwrap_or_else(|| "both".to_string()),
        );
        for item in route.files.iter().chain(route.symbols.iter()) {
            if !is_project_evidence_item(item) {
                continue;
            }
            if let Some(resolved) =
                resolve_build_item(item, &known_files, &known_symbols, &evidence.module_ids)
            {
                let routes = route_members.entry(resolved).or_default();
                if !routes.contains(&route.route) {
                    routes.push(route.route.clone());
                }
            }
        }
    }
    for routes in route_members.values_mut() {
        routes.sort();
    }

    // Sizes may be keyed by path, symbol, or module id.
    let mut size_by_item: BTreeMap<String, u64> = BTreeMap::new();
    for (key, size) in &evidence.sizes {
        if !is_project_evidence_item(key) {
            continue;
        }
        if let Some(resolved) =
            resolve_build_item(key, &known_files, &known_symbols, &evidence.module_ids)
        {
            size_by_item.insert(resolved, *size);
        } else {
            size_by_item.insert(normalize_build_path(key), *size);
        }
    }

    let mut route_count: BTreeMap<&str, usize> = BTreeMap::new();
    for (item, routes) in &route_members {
        if matched.contains(item) {
            route_count.insert(item.as_str(), routes.len());
        }
    }

    let mut bytes = BuildBytes::default();
    let mut unknown_sizes = Vec::new();
    for item in &matched {
        match size_by_item.get(item).copied() {
            Some(size) => {
                bytes.matched = bytes.matched.saturating_add(size);
                if route_count.get(item.as_str()).copied().unwrap_or(0) > 1 {
                    bytes.shared = bytes.shared.saturating_add(size);
                } else {
                    bytes.total_route = bytes.total_route.saturating_add(size);
                }
            }
            None => {
                bytes.unknown += 1;
                unknown_sizes.push(item.clone());
            }
        }
    }
    // A non-full join cannot claim complete byte attribution, even when every
    // matched module happened to carry a size.
    bytes.partial = bytes.unknown > 0 || status != "full";
    unknown_sizes.sort();

    let routes = evidence
        .routes
        .iter()
        .map(|route| {
            let mut files = 0;
            let mut symbols = 0;
            let mut route_bytes: u64 = 0;
            let mut unknown = 0;
            for item in route.files.iter().chain(route.symbols.iter()) {
                let Some(resolved) =
                    resolve_build_item(item, &known_files, &known_symbols, &evidence.module_ids)
                else {
                    continue;
                };
                if !matched.contains(&resolved) {
                    continue;
                }
                if route.files.contains(item) {
                    files += 1;
                } else {
                    symbols += 1;
                }
                match size_by_item.get(&resolved).copied() {
                    Some(size) if route_count.get(resolved.as_str()).copied().unwrap_or(0) <= 1 => {
                        route_bytes = route_bytes.saturating_add(size);
                    }
                    Some(_) => {}
                    None => unknown += 1,
                }
            }
            BuildRouteJoin {
                route: route.route.clone(),
                files,
                symbols,
                bytes: route_bytes,
                runtime: route.runtime.clone(),
                unknown,
            }
        })
        .collect();

    let mut edges = Vec::new();
    for reference in &evidence.client_references {
        let from = resolve_build_item(
            &reference.from,
            &known_files,
            &known_symbols,
            &evidence.module_ids,
        );
        let to = resolve_build_item(
            &reference.to,
            &known_files,
            &known_symbols,
            &evidence.module_ids,
        );
        if let (Some(from), Some(to)) = (from, to) {
            edges.push(BuildEdge {
                from: symbol_or_module(&from, &known_symbols),
                to: symbol_or_module(&to, &known_symbols),
                evidence: if reference.kind.is_empty() {
                    "client-reference".to_string()
                } else {
                    format!("client-reference:{}", reference.kind)
                },
            });
        }
    }
    edges.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| a.evidence.cmp(&b.evidence))
    });
    edges.dedup();

    let verdicts = candidate_verdicts(
        snapshot,
        evidence,
        status,
        &matched,
        &route_members,
        &route_runtime,
    );

    BuildJoin {
        status: status.to_string(),
        joined,
        total,
        coverage,
        unmatched,
        routes,
        bytes,
        verdicts,
        edges,
        unknown_sizes,
        diagnostics,
    }
}
