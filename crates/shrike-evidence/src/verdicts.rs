use std::collections::{BTreeMap, BTreeSet};

use shrike_graph::GraphSnapshot;

use crate::types::{BuildEvidence, BuildVerdict};

pub(crate) fn candidate_verdicts(
    snapshot: &GraphSnapshot,
    evidence: &BuildEvidence,
    status: &str,
    matched: &BTreeSet<String>,
    route_members: &BTreeMap<String, Vec<String>>,
    route_runtime: &BTreeMap<String, String>,
) -> Vec<BuildVerdict> {
    let mut verdicts = Vec::new();
    for key in &snapshot.candidates {
        let symbol = snapshot.symbol(key);
        let file = symbol
            .map(|record| record.file.clone())
            .or_else(|| key.rsplit_once('#').map(|(file, _)| file.to_string()))
            .unwrap_or_else(|| key.clone());
        let matched_item = if matched.contains(key) {
            Some(key.clone())
        } else if matched.contains(&file) {
            Some(file.clone())
        } else {
            None
        };
        let layer = layer_of(key, file.as_str(), evidence);
        let routes = route_members
            .get(key)
            .or_else(|| route_members.get(&file))
            .cloned()
            .unwrap_or_default();
        let client_route = routes.iter().any(|route| {
            matches!(
                route_runtime.get(route).map(String::as_str),
                Some("client") | Some("both") | None
            )
        });
        let (verdict, reason) = if status == "unsupported" {
            ("unsupported", "Next.js version is unsupported")
        } else if status == "stale" {
            ("stale", "build evidence is stale")
        } else if status == "build-failed" || status == "malformed" || status == "none" {
            ("unknown", "build evidence is unavailable")
        } else if matched_item.is_some() {
            if layer == "client" || client_route {
                ("present", "present in build output")
            } else if layer == "server" {
                ("server-only", "present only in server build output")
            } else {
                ("present", "matched a build module")
            }
        } else if status == "full" || status == "partial" {
            ("absent", "absent from joined build evidence")
        } else {
            (
                "unknown",
                "join coverage is below the confirmation threshold",
            )
        };
        verdicts.push(BuildVerdict {
            symbol: key.clone(),
            file,
            verdict: verdict.to_string(),
            layer,
            routes,
            reason: reason.to_string(),
        });
    }
    verdicts.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    verdicts
}

pub(crate) fn layer_of(key: &str, file: &str, evidence: &BuildEvidence) -> String {
    evidence
        .layers
        .get(key)
        .or_else(|| evidence.layers.get(file))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn symbol_or_module(item: &str, known_symbols: &BTreeSet<&str>) -> String {
    if item.contains('#') {
        return item.to_string();
    }
    let module = format!("{item}#__module__");
    if known_symbols.contains(module.as_str()) {
        module
    } else {
        item.to_string()
    }
}
