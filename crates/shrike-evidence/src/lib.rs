#![forbid(unsafe_code)]
//! Build-graph evidence join. Build evidence can only upgrade confidence; it
//! never invents reachability.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use shrike_core::{Diagnostic, Span};
use shrike_graph::{EdgeRecord, GraphSnapshot, WitnessRecord};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEvidence {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub routes: Vec<BuildRoute>,
    #[serde(default)]
    pub sizes: BTreeMap<String, u64>,
    #[serde(default)]
    pub snapshot_hash: Option<String>,
    /// File or symbol key to `client` / `server` / `edge` / `unknown`.
    #[serde(default)]
    pub layers: BTreeMap<String, String>,
    /// Build module id to normalized repository path.
    #[serde(default)]
    pub module_ids: BTreeMap<String, String>,
    /// Server-to-client and client-to-server boundary records.
    #[serde(default)]
    pub client_references: Vec<ClientReference>,
    /// Source hashes captured by the build, used to detect stale snapshots.
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>,
    /// Records the host could not classify. Additive evidence is skipped.
    #[serde(default)]
    pub unknown_records: usize,
    /// Host-detected override: `stale`, `malformed`, `build-failed`, or `unsupported`.
    #[serde(default)]
    pub status: Option<String>,
    /// Emit `client-boundary-leak` diagnostics. Defaults to true.
    #[serde(default)]
    pub client_boundary_leaks: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRoute {
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    /// `client`, `server`, `edge`, `both`, or absent when unknown.
    #[serde(default)]
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientReference {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildVerdict {
    pub symbol: String,
    pub file: String,
    /// `present`, `server-only`, `absent`, `unknown`, `unsupported`, or `stale`.
    pub verdict: String,
    pub layer: String,
    pub routes: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEdge {
    pub from: String,
    pub to: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRouteJoin {
    pub route: String,
    pub files: usize,
    pub symbols: usize,
    pub bytes: u64,
    /// `client`, `server`, `edge`, `both`, or absent when the export omitted it.
    pub runtime: Option<String>,
    /// Matched route items whose size is unknown.
    pub unknown: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildBytes {
    /// Bytes of every matched module with a known size, counted once.
    pub matched: u64,
    /// Bytes of matched modules that appear on more than one route.
    pub shared: u64,
    /// Sum of route-local bytes; shared modules are not counted here.
    pub total_route: u64,
    /// Matched modules with no size. They contribute zero to the sums above
    /// but are reported as unknown rather than silently treated as empty.
    pub unknown: usize,
    /// True when at least one matched module has no size.
    pub partial: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildJoin {
    pub status: String,
    pub joined: usize,
    pub total: usize,
    pub coverage: f64,
    pub unmatched: Vec<String>,
    pub routes: Vec<BuildRouteJoin>,
    pub bytes: BuildBytes,
    pub verdicts: Vec<BuildVerdict>,
    pub edges: Vec<BuildEdge>,
    pub unknown_sizes: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

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
    wanted.extend(evidence.files.iter().cloned());
    wanted.extend(evidence.symbols.iter().cloned());
    for route in &evidence.routes {
        wanted.extend(route.files.iter().cloned());
        wanted.extend(route.symbols.iter().cloned());
    }
    for reference in &evidence.client_references {
        if !reference.from.is_empty() {
            wanted.insert(reference.from.clone());
        }
        if !reference.to.is_empty() {
            wanted.insert(reference.to.clone());
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

fn diagnostics_for(
    status: &str,
    total: usize,
    coverage: f64,
    unmatched: &[String],
    evidence: &BuildEvidence,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if total == 0 && status == "none" {
        diagnostics.push(Diagnostic::info(
            "build_absent",
            "no build evidence supplied; static evidence only",
        ));
    } else if status == "unsupported" {
        diagnostics.push(Diagnostic::warning(
            "build_unsupported",
            format!(
                "Next.js version {} is below the evidence contract; static evidence only",
                version_label(&evidence.version)
            ),
        ));
    } else if status == "stale" {
        diagnostics.push(Diagnostic::warning(
            "build_stale",
            "build evidence does not match the current tree; static evidence only",
        ));
    } else if status == "build-failed" {
        diagnostics.push(Diagnostic::warning(
            "build_failed",
            "the requested build failed; static evidence only",
        ));
    } else if status == "malformed" {
        diagnostics.push(Diagnostic::warning(
            "build_malformed",
            "build evidence could not be parsed; static evidence only",
        ));
    } else if status == "low-coverage" {
        diagnostics.push(Diagnostic::warning(
            "build_low_coverage",
            format!(
                "build join coverage {:.2} is below 0.80; evidence is not used for confirmation",
                coverage
            ),
        ));
    } else if status == "partial" {
        diagnostics.push(Diagnostic::warning(
            "build_partial",
            format!(
                "build join coverage {:.2} is partial; {} entries did not match the snapshot",
                coverage,
                unmatched.len()
            ),
        ));
    }
    if evidence.client_boundary_leaks != Some(false) {
        for (item, layer) in &evidence.layers {
            if layer != "server" {
                continue;
            }
            let file = item.split('#').next().unwrap_or(item);
            let client_routes: Vec<&String> = evidence
                .routes
                .iter()
                .filter(|route| {
                    matches!(route.runtime.as_deref(), Some("client") | Some("both"))
                        && route.files.iter().any(|candidate| {
                            candidate == item
                                || candidate == file
                                || normalize_build_path(candidate) == normalize_build_path(file)
                        })
                })
                .map(|route| &route.route)
                .collect();
            if !client_routes.is_empty() {
                diagnostics.push(
                    Diagnostic::warning(
                        "client-boundary-leak",
                        format!(
                            "server-only module appears in a client chunk on {}",
                            client_routes
                                .iter()
                                .map(|route| route.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                    .in_file(file),
                );
            }
        }
    }
    diagnostics
}

fn candidate_verdicts(
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

fn layer_of(key: &str, file: &str, evidence: &BuildEvidence) -> String {
    evidence
        .layers
        .get(key)
        .or_else(|| evidence.layers.get(file))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

fn symbol_or_module(item: &str, known_symbols: &BTreeSet<&str>) -> String {
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

fn resolve_build_item(
    item: &str,
    known_files: &BTreeSet<&str>,
    known_symbols: &BTreeSet<&str>,
    module_ids: &BTreeMap<String, String>,
) -> Option<String> {
    if known_files.contains(item) || known_symbols.contains(item) {
        return Some(item.to_string());
    }
    if let Some(mapped) = module_ids.get(item) {
        if let Some(resolved) = resolve_build_item(mapped, known_files, known_symbols, module_ids) {
            return Some(resolved);
        }
    }
    if let Some((file, name)) = item.rsplit_once('#') {
        let file = normalize_build_path(file);
        let file = module_ids
            .get(&file)
            .map_or(file, |mapped| normalize_build_path(mapped));
        if let Some(resolved_file) = match_file(&file, known_files) {
            let key = format!("{resolved_file}#{name}");
            if known_symbols.contains(key.as_str()) {
                return Some(key);
            }
        }
        return None;
    }
    let normalized = normalize_build_path(item);
    if let Some(mapped) = module_ids.get(&normalized) {
        if let Some(resolved) = resolve_build_item(mapped, known_files, known_symbols, module_ids) {
            return Some(resolved);
        }
    }
    match_file(&normalized, known_files).map(ToString::to_string)
}

fn match_file<'a>(normalized: &str, known_files: &'a BTreeSet<&'a str>) -> Option<&'a str> {
    if let Some(exact) = known_files.get(normalized) {
        return Some(*exact);
    }
    let mut candidates: Vec<&str> = known_files
        .iter()
        .copied()
        .filter(|known| {
            normalized.ends_with(&format!("/{known}")) || known.ends_with(&format!("/{normalized}"))
        })
        .collect();
    candidates.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    candidates.into_iter().next()
}

fn normalize_build_path(input: &str) -> String {
    let mut value = input.replace('\\', "/");
    if let Some(index) = value.rfind('!') {
        value = value[index + 1..].to_string();
    }
    for prefix in ["turbopack://", "file://"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.to_string();
        }
    }
    if let Some(index) = value.find('?') {
        value.truncate(index);
    }
    while value.starts_with("./") {
        value = value[2..].to_string();
    }
    value.trim_start_matches('/').to_string()
}

fn status_for_evidence(total: usize, coverage: f64, evidence: &BuildEvidence) -> &'static str {
    if let Some(status) = evidence.status.as_deref() {
        return match status {
            "stale" => "stale",
            "build-failed" => "build-failed",
            "malformed" => "malformed",
            "unsupported" => "unsupported",
            "low-coverage" => "low-coverage",
            "partial" => "partial",
            "full" => "full",
            _ => "none",
        };
    }
    status_for(total, coverage, &evidence.version)
}

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

fn version_label(version: &str) -> &str {
    if version.is_empty() {
        "unknown"
    } else {
        version
    }
}

/// Capability level from the Next.js version. An unknown version is treated as
/// partial rather than current: absence of a version is not evidence of 16.4.
fn capability(version: &str) -> &'static str {
    let trimmed = version.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return "partial";
    }
    let mut parts = trimmed.split('.');
    let major = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
    match (major, minor) {
        (Some(16), Some(minor)) if minor >= 4 => "full",
        (Some(16), Some(_)) => "partial",
        (Some(0..=15), _) => "unsupported",
        _ => "unsupported",
    }
}

fn status_for(total: usize, coverage: f64, version: &str) -> &'static str {
    if total == 0 {
        return "none";
    }
    let capability = capability(version);
    if capability == "unsupported" {
        return "unsupported";
    }
    let coverage_status = if coverage < 0.80 {
        "low-coverage"
    } else if coverage < 0.95 {
        "partial"
    } else {
        "full"
    };
    if capability == "partial" && coverage_status == "full" {
        "partial"
    } else {
        coverage_status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shrike_core::{Span, hash_json};
    use shrike_graph::{FileRecord, GraphSnapshot, SymbolRecord};

    fn snapshot() -> GraphSnapshot {
        let span = Span {
            start: 0,
            end: 10,
            line: 1,
            column: 0,
        };
        GraphSnapshot {
            schema_version: "1.0".into(),
            protocol_version: "1.0".into(),
            tool_version: "1.0.0".into(),
            config_hash: "blake3:00".into(),
            resolver_hash: "blake3:00".into(),
            files: vec![
                FileRecord {
                    path: "app/page.tsx".into(),
                    content_hash: "blake3:00".into(),
                    module_kind: "module".into(),
                    parse_status: shrike_syntax::ParseStatus::Ok,
                    generated: false,
                    package: None,
                    public_api: false,
                    lines: 1,
                },
                FileRecord {
                    path: "lib/format.ts".into(),
                    content_hash: "blake3:01".into(),
                    module_kind: "module".into(),
                    parse_status: shrike_syntax::ParseStatus::Ok,
                    generated: false,
                    package: None,
                    public_api: false,
                    lines: 1,
                },
            ],
            symbols: vec![SymbolRecord {
                key: "lib/format.ts#formatInvoice".into(),
                name: "formatInvoice".into(),
                kind: "function".into(),
                file: "lib/format.ts".into(),
                span,
                exported: true,
                entry: false,
                public_api: false,
                generated: false,
                type_only: false,
                lines: 1,
            }],
            modules: Vec::new(),
            entries: Vec::new(),
            edges: Vec::new(),
            sccs: Vec::new(),
            live: Vec::new(),
            candidates: Vec::new(),
            witnesses: Vec::new(),
            proofs: Vec::new(),
            unknowns: Vec::new(),
            diagnostics: Vec::new(),
            graph_hash: hash_json(&"test"),
        }
    }

    #[test]
    fn joins_files_and_symbols() {
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["app/page.tsx".into()],
            symbols: vec!["lib/format.ts#formatInvoice".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &evidence);
        assert_eq!(join.status, "full");
        assert_eq!(join.joined, 2);
        assert_eq!(join.total, 2);
        assert!((join.coverage - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn low_coverage_is_not_confirmation() {
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["app/page.tsx".into(), "missing.tsx".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &evidence);
        assert_eq!(join.status, "low-coverage");
        assert_eq!(join.unmatched, vec!["missing.tsx"]);
    }

    #[test]
    fn old_next_versions_are_unsupported() {
        let evidence = BuildEvidence {
            version: "15.2.0".into(),
            files: vec!["app/page.tsx".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &evidence);
        assert_eq!(join.status, "unsupported");
    }

    #[test]
    fn shared_bytes_are_not_double_counted() {
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            routes: vec![
                BuildRoute {
                    route: "/".into(),
                    files: vec!["app/page.tsx".into()],
                    symbols: Vec::new(),
                    runtime: Some("client".into()),
                },
                BuildRoute {
                    route: "/about".into(),
                    files: vec!["app/page.tsx".into()],
                    symbols: Vec::new(),
                    runtime: Some("client".into()),
                },
            ],
            sizes: BTreeMap::from([("app/page.tsx".to_string(), 100)]),
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &evidence);
        assert_eq!(join.bytes.matched, 100);
        assert_eq!(join.bytes.shared, 100);
        assert_eq!(join.bytes.total_route, 0);
        assert_eq!(join.routes.len(), 2);
        assert_eq!(join.routes[0].bytes, 0);
    }

    #[test]
    fn missing_evidence_is_explicit() {
        let join = join_build(&snapshot(), &BuildEvidence::default());
        assert_eq!(join.status, "none");
        assert_eq!(join.total, 0);
        assert!(
            join.diagnostics
                .iter()
                .any(|item| item.code == "build_absent")
        );
    }

    #[test]
    fn build_liveness_follows_known_edges() {
        let mut snapshot = snapshot();
        snapshot.live = vec!["app/page.tsx#default".into()];
        snapshot.candidates = vec!["lib/format.ts#formatInvoice".into()];
        snapshot.symbols.push(SymbolRecord {
            key: "app/page.tsx#default".into(),
            name: "default".into(),
            kind: "function".into(),
            file: "app/page.tsx".into(),
            span: Span {
                start: 0,
                end: 10,
                line: 1,
                column: 0,
            },
            exported: true,
            entry: true,
            public_api: false,
            generated: false,
            type_only: false,
            lines: 1,
        });
        snapshot.edges.push(shrike_graph::EdgeRecord {
            source: "app/page.tsx#default".into(),
            target: "lib/format.ts#formatInvoice".into(),
            kind: "reference".into(),
            certainty: "certain".into(),
            file: "app/page.tsx".into(),
            span: Span {
                start: 0,
                end: 10,
                line: 1,
                column: 0,
            },
        });
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["app/page.tsx".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot, &evidence);
        assert_eq!(join.status, "full");
        let augmented = augment_snapshot(&snapshot, &evidence, &join);
        assert!(augmented.is_live("lib/format.ts#formatInvoice"));
        assert!(!augmented.is_candidate("lib/format.ts#formatInvoice"));
        assert!(augmented.proofs.is_empty());
        assert!(
            augmented
                .witnesses
                .iter()
                .any(|item| item.symbol == "lib/format.ts#formatInvoice")
        );
        assert_ne!(augmented.graph_hash, snapshot.graph_hash);
    }

    #[test]
    fn low_coverage_evidence_does_not_augment() {
        let mut snapshot = snapshot();
        snapshot.live = vec!["app/page.tsx#default".into()];
        snapshot.candidates = vec!["lib/format.ts#formatInvoice".into()];
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["missing.tsx".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot, &evidence);
        assert_eq!(join.status, "low-coverage");
        let augmented = augment_snapshot(&snapshot, &evidence, &join);
        assert_eq!(augmented.live, snapshot.live);
        assert_eq!(augmented.candidates, snapshot.candidates);
    }

    fn candidate_snapshot() -> GraphSnapshot {
        let mut snapshot = snapshot();
        let span = Span {
            start: 0,
            end: 10,
            line: 1,
            column: 0,
        };
        snapshot.symbols.push(SymbolRecord {
            key: "app/page.tsx#default".into(),
            name: "default".into(),
            kind: "function".into(),
            file: "app/page.tsx".into(),
            span,
            exported: true,
            entry: true,
            public_api: false,
            generated: false,
            type_only: false,
            lines: 1,
        });
        snapshot.live = vec!["app/page.tsx#default".into()];
        snapshot.candidates = vec!["lib/format.ts#formatInvoice".into()];
        snapshot
    }

    fn verdict_for<'a>(join: &'a BuildJoin, key: &str) -> &'a str {
        join.verdicts
            .iter()
            .find(|item| item.symbol == key)
            .map_or("missing", |item| item.verdict.as_str())
    }

    #[test]
    fn normalizes_loader_query_suffix_and_module_ids() {
        let snapshot = candidate_snapshot();
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec![
                "webpack://next/dist/loader.js!./lib/format.ts?ts=1".into(),
                "/repo/app/page.tsx".into(),
            ],
            module_ids: BTreeMap::from([("module-42".to_string(), "lib/format.ts".to_string())]),
            symbols: vec!["module-42#formatInvoice".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot, &evidence);
        assert_eq!(join.status, "full");
        assert!(join.unmatched.is_empty());
        assert_eq!(verdict_for(&join, "lib/format.ts#formatInvoice"), "present");
    }

    #[test]
    fn verdicts_distinguish_client_server_absent_and_unknown() {
        let mut snapshot = candidate_snapshot();
        let span = Span {
            start: 0,
            end: 10,
            line: 1,
            column: 0,
        };
        snapshot.files.push(FileRecord {
            path: "lib/server.ts".into(),
            content_hash: "blake3:02".into(),
            module_kind: "module".into(),
            parse_status: shrike_syntax::ParseStatus::Ok,
            generated: false,
            package: None,
            public_api: false,
            lines: 1,
        });
        snapshot.symbols.push(SymbolRecord {
            key: "lib/server.ts#action".into(),
            name: "action".into(),
            kind: "function".into(),
            file: "lib/server.ts".into(),
            span,
            exported: true,
            entry: false,
            public_api: false,
            generated: false,
            type_only: false,
            lines: 1,
        });
        snapshot.candidates = vec![
            "lib/format.ts#formatInvoice".into(),
            "lib/server.ts#action".into(),
            "lib/absent.ts#dead".into(),
        ];
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec![
                "app/page.tsx".into(),
                "lib/format.ts".into(),
                "lib/server.ts".into(),
            ],
            symbols: vec![
                "lib/format.ts#formatInvoice".into(),
                "lib/server.ts#action".into(),
            ],
            layers: BTreeMap::from([
                ("lib/format.ts".to_string(), "client".to_string()),
                ("lib/server.ts".to_string(), "server".to_string()),
            ]),
            routes: vec![
                BuildRoute {
                    route: "/".into(),
                    files: vec!["app/page.tsx".into(), "lib/format.ts".into()],
                    symbols: vec!["lib/format.ts#formatInvoice".into()],
                    runtime: Some("client".into()),
                },
                BuildRoute {
                    route: "/api".into(),
                    files: vec!["lib/server.ts".into()],
                    symbols: vec!["lib/server.ts#action".into()],
                    runtime: Some("server".into()),
                },
            ],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot, &evidence);
        assert_eq!(verdict_for(&join, "lib/format.ts#formatInvoice"), "present");
        assert_eq!(verdict_for(&join, "lib/server.ts#action"), "server-only");
        assert_eq!(verdict_for(&join, "lib/absent.ts#dead"), "absent");

        let low = BuildEvidence {
            files: vec!["app/page.tsx".into(), "missing.tsx".into()],
            routes: Vec::new(),
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot, &low);
        assert_eq!(join.status, "low-coverage");
        assert_eq!(verdict_for(&join, "lib/format.ts#formatInvoice"), "unknown");
    }

    #[test]
    fn unknown_sizes_are_not_zero() {
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["app/page.tsx".into()],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &evidence);
        assert_eq!(join.bytes.matched, 0);
        assert_eq!(join.bytes.unknown, 1);
        assert!(join.bytes.partial);
        assert_eq!(join.unknown_sizes, vec!["app/page.tsx"]);
    }

    #[test]
    fn client_boundary_leak_is_a_diagnostic_and_can_be_disabled() {
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["lib/server.ts".into()],
            layers: BTreeMap::from([("lib/server.ts".to_string(), "server".to_string())]),
            routes: vec![BuildRoute {
                route: "/".into(),
                files: vec!["lib/server.ts".into()],
                symbols: Vec::new(),
                runtime: Some("client".into()),
            }],
            ..BuildEvidence::default()
        };
        let snapshot = snapshot();
        let join = join_build(&snapshot, &evidence);
        assert!(
            join.diagnostics
                .iter()
                .any(|item| item.code == "client-boundary-leak")
        );
        let disabled = BuildEvidence {
            client_boundary_leaks: Some(false),
            ..evidence
        };
        let join = join_build(&snapshot, &disabled);
        assert!(
            !join
                .diagnostics
                .iter()
                .any(|item| item.code == "client-boundary-leak")
        );
    }

    #[test]
    fn client_reference_creates_a_build_edge_and_liveness() {
        let snapshot = candidate_snapshot();
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["app/page.tsx".into(), "lib/format.ts".into()],
            client_references: vec![ClientReference {
                from: "app/page.tsx".into(),
                to: "lib/format.ts#formatInvoice".into(),
                kind: "server-action".into(),
            }],
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot, &evidence);
        assert_eq!(join.status, "full");
        assert!(
            join.edges
                .iter()
                .any(|edge| edge.to == "lib/format.ts#formatInvoice")
        );
        let augmented = augment_snapshot(&snapshot, &evidence, &join);
        assert!(augmented.is_live("lib/format.ts#formatInvoice"));
        assert!(!augmented.is_candidate("lib/format.ts#formatInvoice"));
        assert!(
            augmented.edges.iter().any(|edge| {
                edge.kind == "build" && edge.target == "lib/format.ts#formatInvoice"
            })
        );
    }

    #[test]
    fn stale_and_build_failed_statuses_are_explicit() {
        let evidence = BuildEvidence {
            version: "16.4.0".into(),
            files: vec!["app/page.tsx".into()],
            status: Some("stale".into()),
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &evidence);
        assert_eq!(join.status, "stale");
        assert!(
            join.diagnostics
                .iter()
                .any(|item| item.code == "build_stale")
        );

        let failed = BuildEvidence {
            status: Some("build-failed".into()),
            ..BuildEvidence::default()
        };
        let join = join_build(&snapshot(), &failed);
        assert_eq!(join.status, "build-failed");
        assert!(
            join.diagnostics
                .iter()
                .any(|item| item.code == "build_failed")
        );
    }
}
