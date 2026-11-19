#![forbid(unsafe_code)]
//! Build-graph evidence join. Build evidence can only upgrade confidence; it
//! never invents reachability.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use shrike_core::Diagnostic;
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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRouteJoin {
    pub route: String,
    pub files: usize,
    pub symbols: usize,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildBytes {
    /// Bytes of every matched module, counted once even when shared.
    pub matched: u64,
    /// Bytes of matched modules that appear on more than one route.
    pub shared: u64,
    /// Sum of route-local bytes; shared modules are not counted here.
    pub total_route: u64,
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
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn join_build(snapshot: &GraphSnapshot, evidence: &BuildEvidence) -> BuildJoin {
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    wanted.extend(evidence.files.iter().cloned());
    wanted.extend(evidence.symbols.iter().cloned());
    for route in &evidence.routes {
        wanted.extend(route.files.iter().cloned());
        wanted.extend(route.symbols.iter().cloned());
    }
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

    let mut matched: BTreeSet<String> = BTreeSet::new();
    let mut unmatched = Vec::new();
    for item in &wanted {
        if known_files.contains(item.as_str()) || known_symbols.contains(item.as_str()) {
            matched.insert(item.clone());
        } else {
            unmatched.push(item.clone());
        }
    }
    unmatched.sort();

    let total = wanted.len();
    let joined = matched.len();
    let coverage = if total == 0 {
        0.0
    } else {
        joined as f64 / total as f64
    };

    let status = status_for(total, coverage, &evidence.version);
    let mut diagnostics = Vec::new();
    if total == 0 {
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

    // Count how many routes reference each matched item. Shared items are
    // reported once in `shared`, never once per route.
    let mut route_count: BTreeMap<&str, usize> = BTreeMap::new();
    for route in &evidence.routes {
        for item in route.files.iter().chain(route.symbols.iter()) {
            if matched.contains(item) {
                *route_count.entry(item.as_str()).or_insert(0) += 1;
            }
        }
    }

    let size_of = |item: &str| evidence.sizes.get(item).copied().unwrap_or(0);
    let mut bytes = BuildBytes {
        matched: matched.iter().map(|item| size_of(item)).sum(),
        ..BuildBytes::default()
    };
    for item in &matched {
        let size = size_of(item);
        if route_count.get(item.as_str()).copied().unwrap_or(0) > 1 {
            bytes.shared = bytes.shared.saturating_add(size);
        } else {
            bytes.total_route = bytes.total_route.saturating_add(size);
        }
    }

    let routes = evidence
        .routes
        .iter()
        .map(|route| {
            let files = route
                .files
                .iter()
                .filter(|item| matched.contains(*item))
                .count();
            let symbols = route
                .symbols
                .iter()
                .filter(|item| matched.contains(*item))
                .count();
            let route_bytes = route
                .files
                .iter()
                .chain(route.symbols.iter())
                .filter(|item| matched.contains(*item))
                .filter(|item| route_count.get(item.as_str()).copied().unwrap_or(0) <= 1)
                .map(|item| size_of(item))
                .sum();
            BuildRouteJoin {
                route: route.route.clone(),
                files,
                symbols,
                bytes: route_bytes,
            }
        })
        .collect();

    BuildJoin {
        status: status.to_string(),
        joined,
        total,
        coverage,
        unmatched,
        routes,
        bytes,
        diagnostics,
    }
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

    let mut outgoing: BTreeMap<&str, Vec<&EdgeRecord>> = BTreeMap::new();
    for edge in &snapshot.edges {
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
                },
                BuildRoute {
                    route: "/about".into(),
                    files: vec!["app/page.tsx".into()],
                    symbols: Vec::new(),
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
}
