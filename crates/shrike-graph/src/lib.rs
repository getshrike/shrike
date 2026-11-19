#![forbid(unsafe_code)]
//! Deterministic whole-graph construction, entry detection, reachability with
//! witnesses, SCCs, and candidate proofs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};
use shrike_core::glob::glob_match;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_core::{
    Diagnostic, EngineConfig, EntryPolicy, PROTOCOL_VERSION, SNAPSHOT_SCHEMA_VERSION, Span,
    TOOL_VERSION, content_hash, hash_json,
};
use shrike_resolve::Resolver;
use shrike_syntax::{DeclarationKind, DynamicSpecifier, FileSummary, ParseStatus};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub path: String,
    pub content_hash: String,
    pub module_kind: String,
    pub parse_status: ParseStatus,
    pub generated: bool,
    pub package: Option<String>,
    pub public_api: bool,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRecord {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub span: Span,
    pub exported: bool,
    pub entry: bool,
    pub public_api: bool,
    pub generated: bool,
    pub type_only: bool,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRecord {
    pub symbol: String,
    pub framework: String,
    pub kind: String,
    pub route: Option<String>,
    pub runtime: String,
    pub certainty: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRecord {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub certainty: String,
    pub file: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRecord {
    pub path: String,
    pub file: String,
    pub specifiers: Vec<String>,
    pub resolved: Vec<String>,
    pub unresolved: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownRecord {
    pub reason: String,
    pub scope: String,
    pub package: Option<String>,
    pub file: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WitnessRecord {
    pub symbol: String,
    pub path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofRecord {
    pub symbol: String,
    pub reason: String,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshot {
    pub schema_version: String,
    pub protocol_version: String,
    pub tool_version: String,
    pub config_hash: String,
    pub resolver_hash: String,
    pub files: Vec<FileRecord>,
    pub symbols: Vec<SymbolRecord>,
    pub modules: Vec<ModuleRecord>,
    pub entries: Vec<EntryRecord>,
    pub edges: Vec<EdgeRecord>,
    pub sccs: Vec<Vec<String>>,
    pub live: Vec<String>,
    pub candidates: Vec<String>,
    pub witnesses: Vec<WitnessRecord>,
    pub proofs: Vec<ProofRecord>,
    pub unknowns: Vec<UnknownRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub graph_hash: String,
}

impl GraphSnapshot {
    #[must_use]
    pub fn symbol(&self, key: &str) -> Option<&SymbolRecord> {
        self.symbols.iter().find(|symbol| symbol.key == key)
    }

    #[must_use]
    pub fn is_live(&self, key: &str) -> bool {
        self.live.iter().any(|value| value == key)
    }

    #[must_use]
    pub fn is_candidate(&self, key: &str) -> bool {
        self.candidates.iter().any(|value| value == key)
    }
}

struct Builder<'a> {
    config: &'a EngineConfig,
    summaries: Vec<FileSummary>,
    resolver: Resolver,
    symbols: Vec<SymbolRecord>,
    symbol_index: BTreeMap<String, usize>,
    entries: Vec<EntryRecord>,
    edges: Vec<EdgeRecord>,
    modules: Vec<ModuleRecord>,
    unknowns: Vec<UnknownRecord>,
    diagnostics: Vec<Diagnostic>,
    files: Vec<FileRecord>,
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
    let mut builder = Builder::new(config, summaries, resolver);
    builder.build();
    builder.finish()
}

impl<'a> Builder<'a> {
    fn new(config: &'a EngineConfig, summaries: Vec<FileSummary>, resolver: Resolver) -> Self {
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

    fn build(&mut self) {
        self.build_files();
        self.build_symbols();
        self.resolve_imports();
        self.build_edges();
        self.detect_entries();
        self.finish_edges();
    }

    fn build_files(&mut self) {
        let mut diagnostics = Vec::new();
        let summaries = self.summaries.clone();
        for summary in &summaries {
            let package = self
                .resolver
                .package_for(&summary.path)
                .map(|p| p.name.clone());
            if let Some(package) = &package {
                self.package_of
                    .insert(summary.path.clone(), package.clone());
            }
            let generated = summary.generated
                || self
                    .config
                    .generated
                    .iter()
                    .any(|glob| glob_match(glob, &summary.path))
                || summary.path.ends_with(".min.js")
                || summary.path.contains(".generated.");
            let public_api = self
                .config
                .public_api
                .iter()
                .any(|glob| glob_match(glob, &summary.path))
                || summary.path.ends_with(".d.ts")
                || self
                    .resolver
                    .package_for(&summary.path)
                    .and_then(|package| package.entry.as_deref())
                    == Some(summary.path.as_str());
            if public_api {
                self.public_files.insert(summary.path.clone());
                self.file_public.insert(summary.path.clone());
            }
            self.file_generated.insert(summary.path.clone(), generated);
            self.file_lines
                .insert(summary.path.clone(), summary.line_count);
            if summary.parse_status == ParseStatus::Ok {
                self.file_parse_ok.insert(summary.path.clone());
            }
            self.files.push(FileRecord {
                path: summary.path.clone(),
                content_hash: summary.content_hash.clone(),
                module_kind: summary.module_kind.clone(),
                parse_status: summary.parse_status,
                generated,
                package,
                public_api,
                lines: summary.line_count,
            });
            if summary.parse_status != ParseStatus::Ok {
                diagnostics.extend(summary.diagnostics.clone());
                diagnostics.push(
                    Diagnostic::warning(
                        "unparsed_file",
                        "file could not be parsed; its symbols are not deletion candidates",
                    )
                    .in_file(&summary.path),
                );
            }
        }
        self.diagnostics = diagnostics;
    }

    fn add_symbol(
        &mut self,
        summary: &FileSummary,
        name: &str,
        kind: &str,
        span: Span,
        exported: bool,
        type_only: bool,
    ) -> String {
        let key = if name == "default" {
            symbol_key(&summary.path, "default")
        } else {
            symbol_key(&summary.path, name)
        };
        if let Some(index) = self.symbol_index.get(&key) {
            let record = &mut self.symbols[*index];
            record.exported |= exported;
            record.type_only &= type_only;
            return key;
        }
        let generated = self
            .file_generated
            .get(&summary.path)
            .copied()
            .unwrap_or(false);
        let public_api = self.file_public.contains(&summary.path) && exported;
        let record = SymbolRecord {
            key: key.clone(),
            name: name.to_string(),
            kind: kind.to_string(),
            file: summary.path.clone(),
            span,
            exported,
            entry: false,
            public_api,
            generated,
            type_only,
            lines: 1,
        };
        self.symbol_index.insert(key.clone(), self.symbols.len());
        self.symbols.push(record);
        key
    }

    fn build_symbols(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let module_key = module_symbol_key(&summary.path);
            self.add_symbol(
                summary,
                "__module__",
                "module",
                Span::default(),
                false,
                false,
            );
            let _ = module_key;
            let mut local_map = BTreeMap::new();
            let mut export_map = BTreeMap::new();
            for declaration in &summary.declarations {
                let kind = declaration_kind(declaration.kind);
                if declaration.default {
                    let key = self.add_symbol(
                        summary,
                        "default",
                        "default",
                        declaration.span,
                        true,
                        declaration.type_only,
                    );
                    local_map.insert(declaration.name.clone(), key.clone());
                    for name in &declaration.names {
                        local_map.insert(name.clone(), key.clone());
                    }
                    export_map.insert("default".to_string(), key);
                } else {
                    let mut first: Option<String> = None;
                    for name in &declaration.names {
                        let key = self.add_symbol(
                            summary,
                            name,
                            kind,
                            declaration.span,
                            declaration.exported,
                            declaration.type_only,
                        );
                        local_map.insert(name.clone(), key.clone());
                        if declaration.exported {
                            export_map.insert(name.clone(), key.clone());
                        }
                        if first.is_none() {
                            first = Some(key);
                        }
                    }
                    if let Some(first) = first {
                        local_map.insert(declaration.name.clone(), first);
                    }
                }
            }
            for local_export in &summary.local_exports {
                let key = symbol_key(&summary.path, &local_export.exported);
                if let Some(local) = local_map.get(&local_export.local).cloned() {
                    self.add_alias_symbol(
                        summary,
                        &local_export.exported,
                        local_export.span,
                        local_export.type_only,
                    );
                    export_map.insert(local_export.exported.clone(), key.clone());
                    self.edges.push(edge(
                        &key,
                        &local,
                        "re-export",
                        "certain",
                        &summary.path,
                        local_export.span,
                    ));
                }
            }
            for re_export in &summary.re_exports {
                let key = self.add_alias_symbol(
                    summary,
                    &re_export.exported,
                    re_export.span,
                    re_export.type_only,
                );
                export_map.insert(re_export.exported.clone(), key);
            }
            for export_all in &summary.export_alls {
                if let Some(exported) = &export_all.exported {
                    let key = self.add_alias_symbol(
                        summary,
                        exported,
                        export_all.span,
                        export_all.type_only,
                    );
                    export_map.insert(exported.clone(), key);
                }
                self.export_alls
                    .entry(summary.path.clone())
                    .or_default()
                    .push(export_all.source.clone());
            }
            self.exports.insert(summary.path.clone(), export_map);
            self.locals.insert(summary.path.clone(), local_map);
        }
    }

    fn add_alias_symbol(
        &mut self,
        summary: &FileSummary,
        name: &str,
        span: Span,
        type_only: bool,
    ) -> String {
        self.add_symbol(summary, name, "re-export", span, true, type_only)
    }

    fn resolve_imports(&mut self) {
        let mut imports: BTreeMap<String, BTreeMap<String, (String, String)>> = BTreeMap::new();
        let mut namespaces: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let mut module_imports = BTreeMap::new();
            let mut module_namespaces = BTreeMap::new();
            let mut specifiers = Vec::new();
            let mut resolved = Vec::new();
            let mut unresolved = Vec::new();
            for import in &summary.imports {
                specifiers.push(import.source.clone());
                let resolution = self.resolver.resolve(&summary.path, &import.source);
                if let Some(target) = &resolution.target {
                    resolved.push(target.clone());
                    let target_module = module_symbol_key(target);
                    self.edges.push(edge(
                        &module_symbol_key(&summary.path),
                        &target_module,
                        "import",
                        "certain",
                        &summary.path,
                        import.span,
                    ));
                    for binding in &import.bindings {
                        match binding.kind {
                            shrike_syntax::BindingKind::Namespace => {
                                module_namespaces.insert(binding.local.clone(), target.clone());
                            }
                            _ => {
                                let target_symbol = self.resolve_export(
                                    target,
                                    &binding.imported,
                                    &mut BTreeSet::new(),
                                );
                                if let Some(target_symbol) = target_symbol {
                                    module_imports.insert(
                                        binding.local.clone(),
                                        (target.clone(), binding.imported.clone()),
                                    );
                                    self.edges.push(edge(
                                        &module_symbol_key(&summary.path),
                                        &target_symbol,
                                        "import",
                                        if binding.type_only { "type" } else { "certain" },
                                        &summary.path,
                                        binding.span,
                                    ));
                                }
                            }
                        }
                    }
                } else if !resolution.external {
                    unresolved.push(import.source.clone());
                    self.diagnostics.push(
                        Diagnostic::warning(
                            "unresolved_import",
                            format!("cannot resolve `{}`", import.source),
                        )
                        .in_file(&summary.path)
                        .at(import.span),
                    );
                }
            }
            for dynamic in &summary.dynamic {
                match &dynamic.specifier {
                    DynamicSpecifier::Literal(specifier) => {
                        let resolution = self.resolver.resolve(&summary.path, specifier);
                        if let Some(target) = resolution.target {
                            self.edges.push(edge(
                                &module_symbol_key(&summary.path),
                                &module_symbol_key(&target),
                                "dynamic",
                                "certain",
                                &summary.path,
                                dynamic.span,
                            ));
                            if let Some(exports) = self.exports.get(&target).cloned() {
                                for (_, key) in exports {
                                    self.edges.push(edge(
                                        &module_symbol_key(&summary.path),
                                        &key,
                                        "dynamic",
                                        "probable",
                                        &summary.path,
                                        dynamic.span,
                                    ));
                                }
                            }
                        } else if !resolution.external {
                            self.unknowns.push(UnknownRecord {
                                reason: format!("dynamic import `{specifier}` did not resolve"),
                                scope: "repo".to_string(),
                                package: self.package_of.get(&summary.path).cloned(),
                                file: summary.path.clone(),
                                span: dynamic.span,
                            });
                        }
                    }
                    DynamicSpecifier::TemplatePrefix(prefix) => {
                        let base = shrike_core::normalize_relative(&format!(
                            "{}/{}",
                            parent_dir(&summary.path),
                            prefix
                        ));
                        let mut matched = 0;
                        for file in &self.files {
                            if file.path.starts_with(&base) {
                                matched += 1;
                                self.edges.push(edge(
                                    &module_symbol_key(&summary.path),
                                    &module_symbol_key(&file.path),
                                    "dynamic",
                                    "probable",
                                    &summary.path,
                                    dynamic.span,
                                ));
                            }
                        }
                        if matched == 0 {
                            self.unknowns.push(UnknownRecord {
                                reason: format!("dynamic prefix `{prefix}` matched no files"),
                                scope: "repo".to_string(),
                                package: self.package_of.get(&summary.path).cloned(),
                                file: summary.path.clone(),
                                span: dynamic.span,
                            });
                        }
                    }
                    DynamicSpecifier::Unknown => {
                        let package = self.package_of.get(&summary.path).cloned();
                        self.unknowns.push(UnknownRecord {
                            reason: "dynamic specifier cannot be bounded".to_string(),
                            scope: if package.is_some() {
                                "package".to_string()
                            } else {
                                "repo".to_string()
                            },
                            package,
                            file: summary.path.clone(),
                            span: dynamic.span,
                        });
                    }
                }
            }
            for unknown in &summary.unknown {
                self.unknowns.push(UnknownRecord {
                    reason: unknown.reason.clone(),
                    scope: match unknown.scope {
                        shrike_syntax::UnknownScope::Repo => "repo".to_string(),
                        shrike_syntax::UnknownScope::Package(_) => "package".to_string(),
                        shrike_syntax::UnknownScope::Module(_) => "module".to_string(),
                    },
                    package: self.package_of.get(&summary.path).cloned(),
                    file: summary.path.clone(),
                    span: unknown.span,
                });
            }
            self.modules.push(ModuleRecord {
                path: summary.path.clone(),
                file: summary.path.clone(),
                specifiers,
                resolved,
                unresolved,
            });
            imports.insert(summary.path.clone(), module_imports);
            namespaces.insert(summary.path.clone(), module_namespaces);
        }
        self.imports = imports;
        self.namespaces = namespaces;
    }

    fn resolve_export(
        &self,
        file: &str,
        name: &str,
        seen: &mut BTreeSet<(String, String)>,
    ) -> Option<String> {
        if !seen.insert((file.to_string(), name.to_string())) {
            return None;
        }
        if let Some(map) = self.exports.get(file) {
            if let Some(key) = map.get(name) {
                return Some(key.clone());
            }
        }
        // Explicit re-exports and aliases are resolved by their edge targets.
        if let Some(summary) = self.summaries.iter().find(|summary| summary.path == file) {
            for re_export in &summary.re_exports {
                if re_export.exported == name {
                    if let Some(target_file) = self.resolver.resolve(file, &re_export.source).target
                    {
                        return self.resolve_export(&target_file, &re_export.imported, seen);
                    }
                }
            }
            if let Some(alls) = self.export_alls.get(file) {
                for source in alls {
                    if let Some(target_file) = self.resolver.resolve(file, source).target {
                        if let Some(key) = self.resolve_export(&target_file, name, seen) {
                            return Some(key);
                        }
                    }
                }
            }
        }
        None
    }

    fn local_or_imported(&self, file: &str, name: &str) -> Option<String> {
        if let Some(key) = self.locals.get(file).and_then(|map| map.get(name)) {
            return Some(key.clone());
        }
        if let Some((target_file, imported)) = self.imports.get(file).and_then(|map| map.get(name))
        {
            return self.resolve_export(target_file, imported, &mut BTreeSet::new());
        }
        None
    }

    fn build_edges(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let module = module_symbol_key(&summary.path);
            for declaration in &summary.declarations {
                let source = if declaration.default {
                    symbol_key(&summary.path, "default")
                } else {
                    self.locals
                        .get(&summary.path)
                        .and_then(|map| map.get(&declaration.name))
                        .cloned()
                        .unwrap_or_else(|| symbol_key(&summary.path, &declaration.name))
                };
                let refs = &declaration.references;
                for identifier in &refs.identifiers {
                    if identifier == &declaration.name {
                        continue;
                    }
                    if let Some(target) = self.local_or_imported(&summary.path, identifier) {
                        if target != source {
                            self.edges.push(edge(
                                &source,
                                &target,
                                "reference",
                                "certain",
                                &summary.path,
                                declaration.span,
                            ));
                        }
                    }
                }
                for jsx in &refs.jsx {
                    if let Some(target) = self.local_or_imported(&summary.path, jsx) {
                        if target != source {
                            self.edges.push(edge(
                                &source,
                                &target,
                                "jsx",
                                "certain",
                                &summary.path,
                                declaration.span,
                            ));
                        }
                    }
                }
                for member in &refs.members {
                    if let Some(target_file) = self
                        .namespaces
                        .get(&summary.path)
                        .and_then(|map| map.get(&member.object))
                    {
                        match &member.property {
                            Some(property) => {
                                if let Some(target) =
                                    self.resolve_export(target_file, property, &mut BTreeSet::new())
                                {
                                    self.edges.push(edge(
                                        &source,
                                        &target,
                                        "reference",
                                        "certain",
                                        &summary.path,
                                        declaration.span,
                                    ));
                                } else {
                                    self.unknowns.push(UnknownRecord {
                                        reason: format!("unknown member `{}`", member.object),
                                        scope: "module".to_string(),
                                        package: self.package_of.get(&summary.path).cloned(),
                                        file: summary.path.clone(),
                                        span: declaration.span,
                                    });
                                }
                            }
                            None => {
                                self.unknowns.push(UnknownRecord {
                                    reason: format!(
                                        "computed member access on `{}`",
                                        member.object
                                    ),
                                    scope: "module".to_string(),
                                    package: self.package_of.get(&summary.path).cloned(),
                                    file: summary.path.clone(),
                                    span: declaration.span,
                                });
                            }
                        }
                    }
                }
            }
            // Module-level references keep top-level side effects and roots live.
            for identifier in &summary.module_refs.identifiers {
                if let Some(target) = self.local_or_imported(&summary.path, identifier) {
                    self.edges.push(edge(
                        &module,
                        &target,
                        "reference",
                        "certain",
                        &summary.path,
                        Span::default(),
                    ));
                }
            }
            for jsx in &summary.module_refs.jsx {
                if let Some(target) = self.local_or_imported(&summary.path, jsx) {
                    self.edges.push(edge(
                        &module,
                        &target,
                        "jsx",
                        "certain",
                        &summary.path,
                        Span::default(),
                    ));
                }
            }
            for re_export in &summary.re_exports {
                let source = symbol_key(&summary.path, &re_export.exported);
                if let Some(target_file) = self
                    .resolver
                    .resolve(&summary.path, &re_export.source)
                    .target
                {
                    if let Some(target) =
                        self.resolve_export(&target_file, &re_export.imported, &mut BTreeSet::new())
                    {
                        self.edges.push(edge(
                            &source,
                            &target,
                            "re-export",
                            "certain",
                            &summary.path,
                            re_export.span,
                        ));
                    }
                }
            }
            if let Some(alls) = self.export_alls.get(&summary.path).cloned() {
                for source in alls {
                    if let Some(target_file) = self.resolver.resolve(&summary.path, &source).target
                    {
                        if let Some(target_exports) = self.exports.get(&target_file).cloned() {
                            for (_, target) in target_exports {
                                self.edges.push(edge(
                                    &module,
                                    &target,
                                    "re-export",
                                    "certain",
                                    &summary.path,
                                    Span::default(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn detect_entries(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let path = summary.path.as_str();
            let module = module_symbol_key(path);
            let add = |builder: &mut Self,
                       symbol: String,
                       framework: &str,
                       kind: &str,
                       route: Option<String>,
                       runtime: &str| {
                if let Some(record) = builder
                    .symbols
                    .iter_mut()
                    .find(|record| record.key == symbol)
                {
                    record.entry = true;
                    builder.entries.push(EntryRecord {
                        symbol: symbol.clone(),
                        framework: framework.to_string(),
                        kind: kind.to_string(),
                        route,
                        runtime: runtime.to_string(),
                        certainty: "certain".to_string(),
                    });
                }
            };
            let file_name = path.rsplit('/').next().unwrap_or(path);
            let stem = file_name
                .rsplit_once('.')
                .map_or(file_name, |(stem, _)| stem);
            let route = route_of(path);
            let is_app = path.starts_with("app/") || path.contains("/app/");
            let is_pages = path.starts_with("pages/") || path.contains("/pages/");
            let app_entry = is_app
                && matches!(
                    stem,
                    "page"
                        | "layout"
                        | "template"
                        | "loading"
                        | "error"
                        | "global-error"
                        | "not-found"
                        | "route"
                        | "default"
                        | "sitemap"
                        | "robots"
                        | "manifest"
                        | "icon"
                        | "apple-icon"
                        | "opengraph-image"
                        | "twitter-image"
                        | "middleware"
                        | "proxy"
                        | "instrumentation"
                        | "instrumentation-client"
                );
            let pages_entry = is_pages;
            if app_entry || pages_entry {
                for name in [
                    "default",
                    "page",
                    "layout",
                    "route",
                    "metadata",
                    "generateMetadata",
                    "viewport",
                    "generateViewport",
                    "generateStaticParams",
                    "generateSitemaps",
                    "getServerSideProps",
                    "getStaticProps",
                    "getStaticPaths",
                    "getInitialProps",
                    "register",
                    "dynamic",
                    "revalidate",
                    "runtime",
                    "fetchCache",
                    "dynamicParams",
                    "preferredRegion",
                    "maxDuration",
                    "GET",
                    "POST",
                    "PUT",
                    "PATCH",
                    "DELETE",
                    "HEAD",
                    "OPTIONS",
                ] {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(
                            self,
                            key,
                            if app_entry { "next-app" } else { "next-pages" },
                            if name == "default" { "route" } else { "export" },
                            route.clone(),
                            "both",
                        );
                    }
                }
                add(
                    self,
                    module.clone(),
                    if app_entry { "next-app" } else { "next-pages" },
                    "module",
                    route,
                    "both",
                );
            }
            if summary.has_directive("use server") || summary.has_directive("use client") {
                for (name, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    let _ = name;
                    add(self, key, "react", "client-boundary", None, "both");
                }
                add(self, module.clone(), "react", "module", None, "both");
            }
            if summary.has_directive("use server") {
                add(
                    self,
                    module.clone(),
                    "next-app",
                    "server-action",
                    None,
                    "server",
                );
            }
            let test = path.contains(".test.")
                || path.contains(".spec.")
                || path.contains("__tests__/")
                || path.contains("/__fixtures__/")
                || path.contains("/fixtures/")
                || path.contains("/e2e/");
            if self.config.test_entries
                && self.config.entry_policy != EntryPolicy::Production
                && test
            {
                add(self, module.clone(), "tests", "test", None, "both");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "tests", "test-export", None, "both");
                }
            }
            let config_file = matches!(
                stem,
                "next.config"
                    | "postcss.config"
                    | "tailwind.config"
                    | "eslint.config"
                    | "vite.config"
                    | "vitest.config"
                    | "jest.config"
                    | "playwright.config"
                    | "tsconfig"
                    | "instrumentation"
            );
            if config_file {
                add(self, module.clone(), "config", "config", None, "both");
            }
            if summary
                .module_refs
                .identifiers
                .iter()
                .any(|id| id == "createRoot" || id == "hydrateRoot")
            {
                add(self, module.clone(), "react", "root", None, "client");
                for identifier in &summary.module_refs.identifiers {
                    if let Some(target) = self.local_or_imported(path, identifier) {
                        add(self, target, "react", "root-reference", None, "client");
                    }
                }
            }
            if let Some(package) = self.resolver.package_for(path) {
                if package.entry.as_deref() == Some(path) {
                    for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                        add(self, key, "packages", "public-export", None, "both");
                    }
                    add(
                        self,
                        module.clone(),
                        "packages",
                        "package-entry",
                        None,
                        "both",
                    );
                }
            }
            if self.public_files.contains(path) {
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "packages", "public-api", None, "both");
                }
            }
        }
    }

    fn finish_edges(&mut self) {
        self.edges.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then_with(|| a.target.cmp(&b.target))
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.span.start.cmp(&b.span.start))
        });
        self.edges.dedup();
        self.entries.sort_by(|a, b| {
            a.symbol
                .cmp(&b.symbol)
                .then_with(|| a.framework.cmp(&b.framework))
        });
        self.entries
            .dedup_by(|a, b| a.symbol == b.symbol && a.framework == b.framework);
    }

    fn finish(mut self) -> GraphSnapshot {
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
            symbol.lines = if symbol.span.line == 0 {
                1
            } else {
                self.file_lines
                    .get(&symbol.file)
                    .map_or(1, |lines| lines.saturating_sub(symbol.span.line) + 1)
                    .max(1)
            };
        }
        let (live, witnesses, mut proofs) = self.reachability();
        let candidates: Vec<String> = self
            .symbols
            .iter()
            .filter(|symbol| !live.contains(&symbol.key))
            .filter(|symbol| symbol.kind != "module" && symbol.kind != "re-export")
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

    fn reachability(&self) -> (BTreeSet<String>, Vec<WitnessRecord>, Vec<ProofRecord>) {
        let mut outgoing: BTreeMap<&str, Vec<&EdgeRecord>> = BTreeMap::new();
        let mut incoming: BTreeMap<&str, Vec<&EdgeRecord>> = BTreeMap::new();
        for edge in &self.edges {
            outgoing.entry(&edge.source).or_default().push(edge);
            incoming.entry(&edge.target).or_default().push(edge);
        }
        let mut live = BTreeSet::new();
        let mut parent: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut queue = VecDeque::new();
        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        for entry in &entries {
            if live.insert(entry.symbol.clone()) {
                queue.push_back(entry.symbol.clone());
            }
        }
        // Unknown edges widen the live set in their bounded scope.
        for unknown in &self.unknowns {
            match unknown.scope.as_str() {
                "repo" => {
                    for symbol in &self.symbols {
                        live.insert(symbol.key.clone());
                    }
                }
                "package" => {
                    if let Some(package) = &unknown.package {
                        for symbol in &self.symbols {
                            if self.package_of.get(&symbol.file) == Some(package) {
                                live.insert(symbol.key.clone());
                            }
                        }
                    }
                }
                "module" => {
                    if let Some(file) = self.files.iter().find(|file| file.path == unknown.file) {
                        for (_, key) in self.exports.get(&file.path).cloned().unwrap_or_default() {
                            live.insert(key);
                        }
                    }
                }
                _ => {}
            }
        }
        while let Some(source) = queue.pop_front() {
            if let Some(edges) = outgoing.get(source.as_str()) {
                let mut targets: Vec<&EdgeRecord> = edges.clone();
                targets.sort_by(|a, b| a.target.cmp(&b.target).then_with(|| a.kind.cmp(&b.kind)));
                for edge in targets {
                    if live.insert(edge.target.clone()) {
                        parent.insert(edge.target.clone(), (source.clone(), edge.kind.clone()));
                        queue.push_back(edge.target.clone());
                    }
                }
            }
        }
        let mut witnesses = Vec::new();
        for symbol in &live {
            let mut path = vec![symbol.clone()];
            let mut current = symbol.clone();
            let mut guard = 0;
            while let Some((next, _)) = parent.get(&current) {
                path.push(next.clone());
                current = next.clone();
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
        (live, witnesses, proofs)
    }

    fn sccs(&self) -> Vec<Vec<String>> {
        // Tarjan over symbol keys, iterative to avoid recursion limits.
        let keys: Vec<&str> = self
            .symbols
            .iter()
            .map(|symbol| symbol.key.as_str())
            .collect();
        let index_of: BTreeMap<&str, usize> = keys
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

fn edge(
    source: &str,
    target: &str,
    kind: &str,
    certainty: &str,
    file: &str,
    span: Span,
) -> EdgeRecord {
    EdgeRecord {
        source: source.to_string(),
        target: target.to_string(),
        kind: kind.to_string(),
        certainty: certainty.to_string(),
        file: file.to_string(),
        span,
    }
}

fn declaration_kind(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Function => "function",
        DeclarationKind::Class => "class",
        DeclarationKind::Variable => "variable",
        DeclarationKind::Type => "type",
        DeclarationKind::Interface => "interface",
        DeclarationKind::Enum => "enum",
        DeclarationKind::Namespace => "namespace",
        DeclarationKind::Default => "default",
    }
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_string())
}

fn route_of(path: &str) -> Option<String> {
    let stripped = path
        .strip_prefix("app/")
        .or_else(|| path.split_once("/app/").map(|(_, rest)| rest))
        .or_else(|| path.strip_prefix("pages/"))
        .or_else(|| path.split_once("/pages/").map(|(_, rest)| rest))?;
    let mut route = stripped.to_string();
    for suffix in [".tsx", ".ts", ".jsx", ".js", ".mdx"] {
        if let Some(stem) = route.strip_suffix(suffix) {
            route = stem.to_string();
            break;
        }
    }
    for suffix in ["/page", "/route", "/layout", "/index"] {
        if let Some(stem) = route.strip_suffix(suffix) {
            route = stem.to_string();
        }
    }
    if route.is_empty() {
        route = "/".to_string();
    } else if !route.starts_with('/') {
        route = format!("/{route}");
    }
    Some(route)
}

fn graph_hash(snapshot: &GraphSnapshot) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Canonical<'a> {
        schema_version: &'a str,
        protocol_version: &'a str,
        tool_version: &'a str,
        config_hash: &'a str,
        resolver_hash: &'a str,
        files: &'a [FileRecord],
        symbols: &'a [SymbolRecord],
        modules: &'a [ModuleRecord],
        entries: &'a [EntryRecord],
        edges: &'a [EdgeRecord],
        sccs: &'a [Vec<String>],
        live: &'a [String],
        candidates: &'a [String],
        unknowns: &'a [UnknownRecord],
    }
    hash_json(&Canonical {
        schema_version: &snapshot.schema_version,
        protocol_version: &snapshot.protocol_version,
        tool_version: &snapshot.tool_version,
        config_hash: &snapshot.config_hash,
        resolver_hash: &snapshot.resolver_hash,
        files: &snapshot.files,
        symbols: &snapshot.symbols,
        modules: &snapshot.modules,
        entries: &snapshot.entries,
        edges: &snapshot.edges,
        sccs: &snapshot.sccs,
        live: &snapshot.live,
        candidates: &snapshot.candidates,
        unknowns: &snapshot.unknowns,
    })
}

/// Analyze source files from disk. Used by the engine and tests.
pub fn analyze_paths(root: &Path, config: &EngineConfig, paths: &[String]) -> GraphSnapshot {
    let summaries: Vec<FileSummary> = paths
        .iter()
        .filter_map(|path| {
            let full = root.join(path);
            std::fs::read_to_string(full)
                .ok()
                .map(|source| shrike_syntax::parse_source(path, &source))
        })
        .collect();
    build_snapshot(root, summaries, config)
}

/// Walk the configured include set. Deterministic order, ignores honoured.
pub fn discover_files(root: &Path, config: &EngineConfig) -> Vec<String> {
    let mut files = Vec::new();
    let mut stack = vec![String::new()];
    while let Some(directory) = stack.pop() {
        let full = if directory.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&directory)
        };
        let Ok(entries) = std::fs::read_dir(&full) else {
            continue;
        };
        let mut names: Vec<_> = entries.flatten().collect();
        names.sort_by_key(std::fs::DirEntry::file_name);
        for entry in names {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name != ".well-known" && name != "." && name != ".." {
                continue;
            }
            let relative = if directory.is_empty() {
                name.clone()
            } else {
                format!("{directory}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    "node_modules"
                        | ".git"
                        | ".next"
                        | "dist"
                        | "build"
                        | "out"
                        | "coverage"
                        | ".turbo"
                        | ".vercel"
                        | ".shrike"
                ) {
                    continue;
                }
                if config.ignore.iter().any(|glob| {
                    glob_match(glob, &relative) || glob_match(glob, &format!("{relative}/**"))
                }) {
                    continue;
                }
                stack.push(relative);
            } else if shrike_core::glob::is_included(&relative, &config.include, &config.ignore) {
                files.push(relative);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Compute a content hash over all file hashes (cache key).
#[must_use]
pub fn input_hash(summaries: &[FileSummary]) -> String {
    let mut parts: Vec<String> = summaries
        .iter()
        .map(|summary| format!("{}:{}", summary.path, summary.content_hash))
        .collect();
    parts.sort();
    content_hash(&parts.join("\n"))
}
