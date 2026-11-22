use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use shrike_core::{EngineConfig, PROTOCOL_VERSION, SNAPSHOT_SCHEMA_VERSION, TOOL_VERSION};
use shrike_resolve::Resolver;
use shrike_syntax::FileSummary;

use crate::types::GraphSnapshot;
use crate::util::graph_hash;

macro_rules! timed {
    ($enabled:expr, $name:expr, $body:expr) => {{
        let __start = std::time::Instant::now();
        let __value = $body;
        if $enabled {
            eprintln!(
                "shrike build: {}={}ms",
                $name,
                __start.elapsed().as_millis()
            );
        }
        __value
    }};
}

mod analysis;
mod class_members;
mod component_props;
mod edges;
mod entries;
mod enum_members;
mod files;
mod imports;
mod namespace_members;
mod page_data;

pub(crate) struct Builder<'a> {
    config: &'a EngineConfig,
    // Shared so per-pass snapshots are pointer copies instead of deep clones
    // of every file summary (declarations, references, enums, classes, ...).
    summaries: Arc<Vec<FileSummary>>,
    // file path -> index into `summaries`, so export resolution does not scan
    // the whole repository for every imported name.
    summary_index: BTreeMap<String, usize>,
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
    // file -> namespace local alias -> target files
    namespaces: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    export_alls: BTreeMap<String, Vec<String>>,
    package_of: BTreeMap<String, String>,
    // package directory -> parsed package.json. Route and framework detection
    // would otherwise re-read and re-parse the same manifest per file per pass.
    package_manifests: BTreeMap<String, Option<serde_json::Value>>,
    // Parsed `next.config.*` page extensions, computed once and reused by the
    // entry pass and four route-conflict passes.
    page_extensions: Option<BTreeMap<String, BTreeSet<String>>>,
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
        let summary_index = summaries
            .iter()
            .enumerate()
            .map(|(index, summary)| (summary.path.clone(), index))
            .collect();
        let read_manifest = |directory: &str| {
            std::fs::read_to_string(resolver.root().join(directory).join("package.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
        };
        let mut package_manifests = BTreeMap::new();
        for package in resolver.packages() {
            let manifest = read_manifest(&package.directory);
            package_manifests.insert(package.directory.clone(), manifest);
        }
        // The root can have a package.json without a `name`, which the resolver
        // does not surface as a package but dependency checks still consult.
        if !package_manifests.contains_key("") {
            if let Some(manifest) = read_manifest("") {
                package_manifests.insert(String::new(), Some(manifest));
            }
        }
        Self {
            config,
            summaries: Arc::new(summaries),
            summary_index,
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
            package_manifests,
            page_extensions: None,
            public_files: BTreeSet::new(),
            file_generated: BTreeMap::new(),
            file_public: BTreeSet::new(),
            file_lines: BTreeMap::new(),
            file_parse_ok: BTreeSet::new(),
        }
    }

    pub(crate) fn build(&mut self) {
        let timing = std::env::var_os("SHRIKE_TIMING").is_some();
        timed!(timing, "files", self.build_files());
        timed!(timing, "symbols", self.build_symbols());
        timed!(
            timing,
            "invalid_client_exports",
            self.detect_invalid_client_exports()
        );
        timed!(
            timing,
            "invalid_server_exports",
            self.detect_invalid_server_exports()
        );
        timed!(
            timing,
            "misplaced_directives",
            self.detect_misplaced_directives()
        );
        timed!(timing, "resolve_imports", self.resolve_imports());
        timed!(
            timing,
            "mixed_barrels",
            self.detect_mixed_client_server_barrels()
        );
        timed!(
            timing,
            "client_server_leaks",
            self.detect_client_server_leaks()
        );
        timed!(timing, "edges", self.build_edges());
        timed!(timing, "entries", self.detect_entries());
        timed!(
            timing,
            "route_conflicts",
            self.detect_next_route_conflicts()
        );
        timed!(
            timing,
            "segment_conflicts",
            self.detect_next_segment_config_conflicts()
        );
        timed!(
            timing,
            "pages_router_exports",
            self.detect_next_pages_router_exports()
        );
        timed!(
            timing,
            "pages_router_conflicts",
            self.detect_next_pages_router_route_conflicts()
        );
        timed!(timing, "finish_edges", self.finish_edges());
    }

    pub(crate) fn finish(mut self) -> GraphSnapshot {
        let timing = std::env::var_os("SHRIKE_TIMING").is_some();
        timed!(
            timing,
            "sort_symbols",
            self.symbols.sort_by(|a, b| {
                a.file
                    .cmp(&b.file)
                    .then_with(|| a.span.start.cmp(&b.span.start))
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.key.cmp(&b.key))
            })
        );
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
        let (live, witnesses, mut proofs) = timed!(timing, "reachability", self.reachability());
        timed!(
            timing,
            "unused_component_props",
            self.detect_unused_component_props(&live)
        );
        // These passes only read the graph and return their own diagnostics.
        // The component-prop walk is largely sequential, while class/enum
        // collection fans out through rayon, so overlapping them keeps the
        // pool busy instead of waiting for each pass to finish in turn.
        let ((absent, enum_diags), (class_diags, (internal_only, sccs))) = rayon::join(
            || {
                rayon::join(
                    || {
                        timed!(
                            timing,
                            "absent_component_props",
                            self.detect_absent_component_props(&live)
                        )
                    },
                    || {
                        timed!(
                            timing,
                            "unused_enum_members",
                            self.detect_unused_enum_members(&live)
                        )
                    },
                )
            },
            || {
                rayon::join(
                    || {
                        timed!(
                            timing,
                            "unused_class_members",
                            self.detect_unused_class_members(&live)
                        )
                    },
                    || {
                        rayon::join(
                            || timed!(timing, "internal_only", self.internal_only(&live)),
                            || timed!(timing, "sccs", self.sccs()),
                        )
                    },
                )
            },
        );
        self.diagnostics.extend(absent);
        self.diagnostics.extend(enum_diags);
        self.diagnostics.extend(class_diags);
        timed!(
            timing,
            "unused_namespace_members",
            self.detect_unused_namespace_members(&live)
        );
        timed!(
            timing,
            "unused_page_data_keys",
            self.detect_unused_page_data_keys()
        );
        // Every dead named re-export is its own deletion candidate, even when
        // its target declaration is dead too. The declaration alone is not a
        // complete patch: the barrel line still names the removed symbol, so
        // deleting only the target leaves a dangling re-export. Fallow and Knip
        // report these aliases; Shrike reports both links so the batch removes
        // the declaration and every barrel specifier that exposed it.
        //
        // `export *` does not create per-name alias symbols, so this only
        // covers named re-exports whose specifier span the patch can remove.
        let candidates: Vec<String> = timed!(timing, "candidates", {
            self.symbols
                .iter()
                .filter(|symbol| !live.contains(&symbol.key))
                .filter(|symbol| symbol.kind != "module")
                .filter(|symbol| !symbol.entry && !symbol.public_api && !symbol.generated)
                .filter(|symbol| !self.config.include_generated || !symbol.generated)
                .filter(|symbol| self.file_parse_ok.contains(&symbol.file))
                .map(|symbol| symbol.key.clone())
                .collect()
        });
        let candidate_set: BTreeSet<&str> = candidates.iter().map(String::as_str).collect();
        proofs.retain(|proof| candidate_set.contains(proof.symbol.as_str()));
        drop(candidate_set);
        // The builder is consumed here, so move the large collections into the
        // snapshot instead of cloning every file, symbol, and edge.
        let modules = std::mem::take(&mut self.modules);
        let diagnostics = std::mem::take(&mut self.diagnostics);
        let unknowns = std::mem::take(&mut self.unknowns);
        let mut snapshot = timed!(
            timing,
            "snapshot",
            GraphSnapshot {
                schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
                tool_version: TOOL_VERSION.to_string(),
                config_hash: self.config.config_hash(),
                resolver_hash: self.resolver.resolver_hash.clone(),
                files: std::mem::take(&mut self.files),
                symbols: std::mem::take(&mut self.symbols),
                modules,
                entries: std::mem::take(&mut self.entries),
                edges: std::mem::take(&mut self.edges),
                sccs,
                live: live.into_iter().collect(),
                candidates,
                unused_files: Vec::new(),
                internal_only,
                witnesses: witnesses
                    .into_iter()
                    .filter(|witness| !witness.path.is_empty())
                    .collect(),
                proofs,
                unknowns,
                diagnostics,
                graph_hash: String::new(),
            }
        );
        timed!(timing, "unused_files", {
            snapshot.unused_files = crate::types::unused_files(&snapshot);
        });
        timed!(timing, "graph_hash", {
            snapshot.graph_hash = graph_hash(&snapshot);
        });
        snapshot
    }
}
