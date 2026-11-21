use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use shrike_core::{EngineConfig, PROTOCOL_VERSION, SNAPSHOT_SCHEMA_VERSION, TOOL_VERSION};
use shrike_resolve::Resolver;
use shrike_syntax::FileSummary;

use crate::types::GraphSnapshot;
use crate::util::graph_hash;

mod analysis;
mod edges;
mod entries;
mod files;
mod imports;

pub(crate) struct Builder<'a> {
    config: &'a EngineConfig,
    summaries: Vec<FileSummary>,
    resolver: Resolver,
    symbols: Vec<crate::types::SymbolRecord>,
    symbol_index: BTreeMap<String, usize>,
    entries: Vec<crate::types::EntryRecord>,
    edges: Vec<crate::types::EdgeRecord>,
    modules: Vec<crate::types::ModuleRecord>,
    unknowns: Vec<crate::types::UnknownRecord>,
    diagnostics: Vec<shrike_core::Diagnostic>,
    files: Vec<crate::types::FileRecord>,
    // file -> exported name -> symbol key
    exports: BTreeMap<String, BTreeMap<String, String>>,
    // file -> local name -> symbol key
    locals: BTreeMap<String, BTreeMap<String, String>>,
    // file -> local import alias -> (target file, imported name)
    imports: BTreeMap<String, BTreeMap<String, (String, String)>>,
    // file -> namespace local alias -> target file
    namespaces: BTreeMap<String, BTreeMap<String, String>>,
    export_alls: BTreeMap<String, Vec<String>>,
    package_of: BTreeMap<String, String>,
    public_files: BTreeSet<String>,
    file_generated: BTreeMap<String, bool>,
    file_public: BTreeSet<String>,
    file_lines: BTreeMap<String, u32>,
    file_parse_ok: BTreeSet<String>,
}

/// Build a snapshot from already parsed summaries.
#[must_use]
pub fn build_snapshot(
    root: &Path,
    summaries: Vec<FileSummary>,
    config: &EngineConfig,
) -> GraphSnapshot {
    let mut summaries = summaries;
    summaries.sort_by(|a, b| a.path.cmp(&b.path));
    summaries.dedup_by(|a, b| a.path == b.path);
    let paths: Vec<String> = summaries
        .iter()
        .map(|summary| summary.path.clone())
        .collect();
    let resolver = Resolver::new(root, &paths, config);
    build_snapshot_with_resolver(summaries, config, resolver)
}

/// Build a snapshot using a resolver the caller already constructed. Callers
/// that key caches on the resolver hash use this to avoid resolving twice.
#[must_use]
pub fn build_snapshot_with_resolver(
    mut summaries: Vec<FileSummary>,
    config: &EngineConfig,
    resolver: Resolver,
) -> GraphSnapshot {
    summaries.sort_by(|a, b| a.path.cmp(&b.path));
    summaries.dedup_by(|a, b| a.path == b.path);
    let mut builder = Builder::new(config, summaries, resolver);
    builder.build();
    builder.finish()
}

impl<'a> Builder<'a> {
    pub(crate) fn new(
        config: &'a EngineConfig,
        summaries: Vec<FileSummary>,
        resolver: Resolver,
    ) -> Self {
        Self {
            config,
            summaries,
            resolver,
            symbols: Vec::new(),
            symbol_index: BTreeMap::new(),
            entries: Vec::new(),
            edges: Vec::new(),
            modules: Vec::new(),
            unknowns: Vec::new(),
            diagnostics: Vec::new(),
            files: Vec::new(),
            exports: BTreeMap::new(),
            locals: BTreeMap::new(),
            imports: BTreeMap::new(),
            namespaces: BTreeMap::new(),
            export_alls: BTreeMap::new(),
            package_of: BTreeMap::new(),
            public_files: BTreeSet::new(),
            file_generated: BTreeMap::new(),
            file_public: BTreeSet::new(),
            file_lines: BTreeMap::new(),
            file_parse_ok: BTreeSet::new(),
        }
    }

    pub(crate) fn build(&mut self) {
        self.build_files();
        self.build_symbols();
        self.resolve_imports();
        self.build_edges();
        self.detect_entries();
        self.finish_edges();
    }

    pub(crate) fn finish(mut self) -> GraphSnapshot {
        self.symbols.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then_with(|| a.span.start.cmp(&b.span.start))
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.key.cmp(&b.key))
        });
        // Recompute indices after sorting.
        self.symbol_index = self
            .symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| (symbol.key.clone(), index))
            .collect();
        for symbol in &mut self.symbols {
            // Use the declaration's own span. Counting from its first line to
            // the end of the file overstates impact for everything that is not
            // the final declaration (and the receipt is the product).
            symbol.lines = if symbol.span.line == 0 {
                1
            } else if symbol.span.end_line >= symbol.span.line {
                symbol.span.end_line - symbol.span.line + 1
            } else {
                self.file_lines
                    .get(&symbol.file)
                    .map_or(1, |lines| lines.saturating_sub(symbol.span.line) + 1)
                    .max(1)
            };
        }
        let (live, witnesses, mut proofs) = self.reachability();
        let internal_only = self.internal_only(&live);
        // A barrel alias is actionable only when its chain reaches a live
        // symbol. Report every dead link in that chain: removing an inner
        // alias would otherwise leave an outer alias dangling. When the chain
        // ends at a dead declaration, report the declaration instead so the
        // alias does not duplicate it.
        let mut re_export_sources: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for edge in &self.edges {
            if edge.kind == "re-export" {
                re_export_sources
                    .entry(edge.target.as_str())
                    .or_default()
                    .push(edge.source.as_str());
            }
        }
        let mut actionable_re_exports: BTreeSet<&str> = BTreeSet::new();
        let mut queue: Vec<&str> = live.iter().map(String::as_str).collect();
        while let Some(target) = queue.pop() {
            if let Some(sources) = re_export_sources.get(target) {
                for source in sources {
                    if actionable_re_exports.insert(source) {
                        queue.push(source);
                    }
                }
            }
        }
        let candidates: Vec<String> = self
            .symbols
            .iter()
            .filter(|symbol| !live.contains(&symbol.key))
            .filter(|symbol| symbol.kind != "module")
            .filter(|symbol| {
                symbol.kind != "re-export" || actionable_re_exports.contains(symbol.key.as_str())
            })
            .filter(|symbol| !symbol.entry && !symbol.public_api && !symbol.generated)
            .filter(|symbol| !self.config.include_generated || !symbol.generated)
            .filter(|symbol| self.file_parse_ok.contains(&symbol.file))
            .map(|symbol| symbol.key.clone())
            .collect();
        let candidate_set: BTreeSet<&str> = candidates.iter().map(String::as_str).collect();
        proofs.retain(|proof| candidate_set.contains(proof.symbol.as_str()));
        let sccs = self.sccs();
        let modules = self.modules.clone();
        let diagnostics = self.diagnostics.clone();
        let unknowns = self.unknowns.clone();
        let mut snapshot = GraphSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            tool_version: TOOL_VERSION.to_string(),
            config_hash: self.config.config_hash(),
            resolver_hash: self.resolver.resolver_hash.clone(),
            files: self.files.clone(),
            symbols: self.symbols.clone(),
            modules,
            entries: self.entries.clone(),
            edges: self.edges.clone(),
            sccs,
            live: live.into_iter().collect(),
            candidates: candidates.clone(),
            internal_only,
            witnesses: witnesses
                .into_iter()
                .filter(|witness| !witness.path.is_empty())
                .collect(),
            proofs,
            unknowns,
            diagnostics,
            graph_hash: String::new(),
        };
        snapshot.graph_hash = graph_hash(&snapshot);
        snapshot
    }
}
