#![forbid(unsafe_code)]
//! Build-graph evidence join. Build evidence can only upgrade confidence; it
//! never invents reachability.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use shrike_core::Diagnostic;
use shrike_graph::GraphSnapshot;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildJoin {
    pub status: String,
    pub joined: usize,
    pub unmatched: Vec<String>,
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
    let mut joined = 0;
    let mut unmatched = Vec::new();
    for item in &wanted {
        if known_files.contains(item.as_str()) || known_symbols.contains(item.as_str()) {
            joined += 1;
        } else {
            unmatched.push(item.clone());
        }
    }
    unmatched.sort();
    let status = if wanted.is_empty() {
        "none"
    } else if unmatched.is_empty() {
        "full"
    } else {
        "partial"
    };
    let mut diagnostics = Vec::new();
    if status == "partial" {
        diagnostics.push(Diagnostic::warning(
            "build_partial",
            format!(
                "{} build evidence entries did not match the snapshot",
                unmatched.len()
            ),
        ));
    }
    if status == "none" {
        diagnostics.push(Diagnostic::info(
            "build_absent",
            "no build evidence supplied; static evidence only",
        ));
    }
    BuildJoin {
        status: status.to_string(),
        joined,
        unmatched,
        diagnostics,
    }
}
