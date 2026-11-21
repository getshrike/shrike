use std::collections::{BTreeMap, BTreeSet};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use shrike_core::ids::module_symbol_key;
use shrike_core::{Diagnostic, Span, normalize_relative};
use shrike_syntax::{DynamicKind, DynamicRecord, DynamicSpecifier, ParseStatus};

use super::Builder;
use crate::types::{ModuleRecord, UnknownRecord};
use crate::util::{edge, parent_dir};

/// Dynamic edges keep the module live; workers use their own edge kind so a
/// witness path and delta trigger explain why a file that is not imported is
/// still reachable.
fn dynamic_edge_kind(record: &DynamicRecord) -> &'static str {
    if record.kind == DynamicKind::Worker {
        "worker"
    } else {
        "dynamic"
    }
}

/// Worker and worklet loads take a URL, not a bundler module specifier. An
/// absolute or scheme URL points at a runtime asset (for example
/// `navigator.serviceWorker.register("/sw.js")`); it cannot expose another
/// analyzed module, so it must not widen the repository.
fn is_runtime_worker_url(specifier: &str) -> bool {
    specifier.starts_with('/')
        || specifier.starts_with("http://")
        || specifier.starts_with("https://")
        || specifier.starts_with("data:")
        || specifier.starts_with("blob:")
}

/// Imports the TypeScript graph does not analyze: bundler assets, framework
/// component formats with their own compilers, and generated output that only
/// exists after a build. These are informational, never actionable.
fn is_non_source_specifier(specifier: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        ".css", ".scss", ".sass", ".less", ".styl", ".svg", ".png", ".jpg", ".jpeg", ".gif",
        ".webp", ".avif", ".ico", ".bmp", ".woff", ".woff2", ".ttf", ".otf", ".eot", ".mp4",
        ".webm", ".mp3", ".wav", ".pdf", ".wasm", ".json", ".yaml", ".yml", ".toml", ".graphql",
        ".gql", ".vue", ".svelte", ".astro",
    ];
    // Bundler suffixes (`?url`, `?raw`, `#fragment`) do not change the file.
    let path = specifier.split(['?', '#']).next().unwrap_or(specifier);
    const GENERATED: &[&str] = &[
        ".next/",
        ".nuxt/",
        ".svelte-kit/",
        ".astro/",
        ".output/",
        ".vercel/",
    ];
    if GENERATED.iter().any(|directory| path.contains(directory)) {
        return true;
    }
    EXTENSIONS.iter().any(|extension| path.ends_with(extension))
}

impl<'a> Builder<'a> {
    pub(super) fn resolve_imports(&mut self) {
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
            let mut external = Vec::new();
            for import in &summary.imports {
                specifiers.push(import.source.clone());
                let resolution = self.resolver.resolve(&summary.path, &import.source);
                // Assets and framework components have no parsed symbols. Keep
                // package evidence, but do not create an edge to a module node
                // that cannot exist in the graph.
                if is_non_source_specifier(&import.source) {
                    if resolution.external {
                        external.push(import.source.clone());
                    } else if resolution.target.is_none() {
                        self.diagnostics.push(
                            Diagnostic::info(
                                "asset_import",
                                format!("`{}` is not analyzed as source", import.source),
                            )
                            .in_file(&summary.path)
                            .at(import.span),
                        );
                    }
                    continue;
                }
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
                } else {
                    external.push(import.source.clone());
                }
            }
            for dynamic in &summary.dynamic {
                match &dynamic.specifier {
                    DynamicSpecifier::Literal(specifier) => {
                        specifiers.push(specifier.clone());
                        if dynamic.kind == DynamicKind::Worker && is_runtime_worker_url(specifier) {
                            // A runtime URL is not a package or a module edge;
                            // keep it out of dependency evidence as well.
                            continue;
                        }
                        let resolution = self.resolver.resolve(&summary.path, specifier);
                        if is_non_source_specifier(specifier) {
                            if resolution.external {
                                external.push(specifier.clone());
                            }
                            continue;
                        }
                        if let Some(target) = resolution.target {
                            self.edges.push(edge(
                                &module_symbol_key(&summary.path),
                                &module_symbol_key(&target),
                                dynamic_edge_kind(dynamic),
                                "certain",
                                &summary.path,
                                dynamic.span,
                            ));
                            self.link_dynamic_exports(&summary.path, &target, dynamic);
                        } else if !resolution.external {
                            self.unknowns.push(UnknownRecord {
                                reason: format!("dynamic import `{specifier}` did not resolve"),
                                scope: "repo".to_string(),
                                package: self.package_of.get(&summary.path).cloned(),
                                file: summary.path.clone(),
                                span: dynamic.span,
                            });
                        } else {
                            external.push(specifier.clone());
                        }
                    }
                    DynamicSpecifier::TemplatePrefix(prefix) => {
                        let base = normalize_relative(&format!(
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
                                    dynamic_edge_kind(dynamic),
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
                    DynamicSpecifier::Glob { patterns, excludes } => {
                        let mut includes = Vec::new();
                        for pattern in patterns {
                            if let Some(resolved) =
                                self.resolver.glob_pattern(&summary.path, pattern)
                            {
                                includes.push(resolved);
                            } else {
                                self.diagnostics.push(
                                    Diagnostic::warning(
                                        "unresolved_glob",
                                        format!("cannot resolve glob pattern `{pattern}`"),
                                    )
                                    .in_file(&summary.path)
                                    .at(dynamic.span),
                                );
                                self.unknowns.push(UnknownRecord {
                                    reason: format!("glob pattern `{pattern}` cannot be bounded"),
                                    scope: "repo".to_string(),
                                    package: self.package_of.get(&summary.path).cloned(),
                                    file: summary.path.clone(),
                                    span: dynamic.span,
                                });
                            }
                        }
                        if includes.is_empty() {
                            continue;
                        }
                        // Vite anchors a `**` exclude at the common base of
                        // the positive patterns, not at the importing file.
                        let base = common_glob_base(&includes);
                        let mut resolved_excludes = Vec::new();
                        for exclude in excludes {
                            if exclude.starts_with("**") {
                                if let Some(base) = &base {
                                    resolved_excludes.push(if base.is_empty() {
                                        exclude.clone()
                                    } else {
                                        format!("{base}/{exclude}")
                                    });
                                }
                            } else if let Some(resolved) =
                                self.resolver.glob_pattern(&summary.path, exclude)
                            {
                                resolved_excludes.push(resolved);
                            }
                            // An exclude this resolver cannot place removes no
                            // file, which can only keep symbols live.
                        }
                        match compile_globs(&includes, &resolved_excludes) {
                            Ok(matcher) => {
                                let matched_files: Vec<String> = self
                                    .files
                                    .iter()
                                    .filter(|file| matcher.matches(&file.path))
                                    .map(|file| file.path.clone())
                                    .collect();
                                let matched = matched_files.len();
                                for file in matched_files {
                                    self.edges.push(edge(
                                        &module_symbol_key(&summary.path),
                                        &module_symbol_key(&file),
                                        "dynamic",
                                        "probable",
                                        &summary.path,
                                        dynamic.span,
                                    ));
                                    self.link_dynamic_exports(&summary.path, &file, dynamic);
                                }
                                if matched == 0 {
                                    self.diagnostics.push(
                                        Diagnostic::warning(
                                            "glob_matched_no_files",
                                            format!(
                                                "glob patterns [{}] matched no analyzed files",
                                                includes.join(", ")
                                            ),
                                        )
                                        .in_file(&summary.path)
                                        .at(dynamic.span),
                                    );
                                }
                            }
                            Err(error) => {
                                self.diagnostics.push(
                                    Diagnostic::warning(
                                        "invalid_glob",
                                        format!("dynamic glob cannot be compiled: {error}"),
                                    )
                                    .in_file(&summary.path)
                                    .at(dynamic.span),
                                );
                                self.unknowns.push(UnknownRecord {
                                    reason: "dynamic glob cannot be compiled".to_string(),
                                    scope: "repo".to_string(),
                                    package: self.package_of.get(&summary.path).cloned(),
                                    file: summary.path.clone(),
                                    span: dynamic.span,
                                });
                            }
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
            // Type-position imports (`import("./types").Config`, JSDoc
            // `@type {import("./types").Config}`) use an export without a
            // runtime import. Link the file's module symbol to that export so
            // the type remains live while the file itself is reachable.
            for type_import in &summary.type_imports {
                let Some(target_file) = self
                    .resolver
                    .resolve(&summary.path, &type_import.source)
                    .target
                else {
                    continue;
                };
                let Some(imported) = &type_import.imported else {
                    continue;
                };
                if let Some(target) =
                    self.resolve_export(&target_file, imported, &mut BTreeSet::new())
                {
                    self.edges.push(edge(
                        &module_symbol_key(&summary.path),
                        &target,
                        "type",
                        "certain",
                        &summary.path,
                        Span::default(),
                    ));
                } else {
                    self.unknowns.push(UnknownRecord {
                        reason: format!("unknown type import member `{imported}`"),
                        scope: "module".to_string(),
                        package: self.package_of.get(&summary.path).cloned(),
                        file: target_file,
                        span: Span::default(),
                    });
                }
            }
            self.modules.push(ModuleRecord {
                path: summary.path.clone(),
                file: summary.path.clone(),
                specifiers,
                resolved,
                unresolved,
                external,
                type_refs: summary.type_refs.clone(),
            });
            imports.insert(summary.path.clone(), module_imports);
            namespaces.insert(summary.path.clone(), module_namespaces);
        }
        self.imports = imports;
        self.namespaces = namespaces;
    }

    /// Link a literal dynamic import to the exports it actually consumes.
    ///
    /// A destructured import names exports precisely. Any other shape, an
    /// empty name list, or a name that does not resolve widens to every
    /// export: fail closed rather than report an export that an unknown
    /// namespace use could still reach.
    fn link_dynamic_exports(&mut self, source: &str, target: &str, dynamic: &DynamicRecord) {
        let precise = dynamic
            .imported
            .as_ref()
            .filter(|names| !names.is_empty())
            .and_then(|names| {
                let mut keys = Vec::new();
                for name in names {
                    keys.push(self.resolve_export(target, name, &mut BTreeSet::new())?);
                }
                keys.sort();
                keys.dedup();
                Some(keys)
            });
        if let Some(keys) = precise {
            for key in keys {
                self.edges.push(edge(
                    &module_symbol_key(source),
                    &key,
                    "dynamic",
                    "probable",
                    source,
                    dynamic.span,
                ));
            }
            return;
        }
        if let Some(exports) = self.exports.get(target).cloned() {
            for (_, key) in exports {
                self.edges.push(edge(
                    &module_symbol_key(source),
                    &key,
                    "dynamic",
                    "probable",
                    source,
                    dynamic.span,
                ));
            }
        }
    }

    pub(super) fn resolve_export(
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

    pub(super) fn local_or_imported(&self, file: &str, name: &str) -> Option<String> {
        if let Some(key) = self.locals.get(file).and_then(|map| map.get(name)) {
            return Some(key.clone());
        }
        if let Some((target_file, imported)) = self.imports.get(file).and_then(|map| map.get(name))
        {
            return self.resolve_export(target_file, imported, &mut BTreeSet::new());
        }
        None
    }
}

struct CompiledGlobs {
    includes: GlobSet,
    excludes: GlobSet,
}

impl CompiledGlobs {
    fn matches(&self, path: &str) -> bool {
        self.includes.is_match(path) && !self.excludes.is_match(path)
    }
}

fn compile_globs(
    includes: &[String],
    excludes: &[String],
) -> Result<CompiledGlobs, globset::Error> {
    Ok(CompiledGlobs {
        includes: compile_glob_set(includes)?,
        excludes: compile_glob_set(excludes)?,
    })
}

fn compile_glob_set(patterns: &[String]) -> Result<GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(GlobBuilder::new(pattern).literal_separator(true).build()?);
    }
    builder.build()
}

/// Static directory shared by every positive glob. Vite uses it as the base
/// for leading-`**` negative patterns. `Some("")` means the repository root.
fn common_glob_base(patterns: &[String]) -> Option<String> {
    let first = static_glob_base(patterns.first()?);
    let mut common: Vec<&str> = first.split('/').filter(|s| !s.is_empty()).collect();
    for pattern in &patterns[1..] {
        let base = static_glob_base(pattern);
        let segments: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
        let shared = common
            .iter()
            .zip(segments.iter())
            .take_while(|(left, right)| left == right)
            .count();
        common.truncate(shared);
    }
    Some(common.join("/"))
}

fn static_glob_base(pattern: &str) -> String {
    let cutoff = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
    let prefix = &pattern[..cutoff];
    if cutoff < pattern.len() && prefix.ends_with('/') {
        prefix.trim_end_matches('/').to_string()
    } else {
        prefix
            .rsplit_once('/')
            .map_or_else(String::new, |(directory, _)| directory.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shrike_core::EngineConfig;
    use tempfile::tempdir;

    use crate::discovery::analyze_paths;
    use crate::types::GraphSnapshot;

    fn snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
        let directory = tempdir().expect("tempdir");
        for (path, content) in files {
            let full = directory.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parents");
            }
            fs::write(full, content).expect("write fixture");
        }
        let paths: Vec<String> = files.iter().map(|(path, _)| (*path).to_string()).collect();
        analyze_paths(directory.path(), &EngineConfig::default(), &paths)
    }

    fn is_live(snapshot: &GraphSnapshot, key: &str) -> bool {
        assert!(snapshot.symbol(key).is_some(), "missing symbol {key}");
        snapshot.is_live(key)
    }

    fn is_candidate(snapshot: &GraphSnapshot, key: &str) -> bool {
        assert!(snapshot.symbol(key).is_some(), "missing symbol {key}");
        snapshot.is_candidate(key)
    }

    #[test]
    fn import_meta_glob_keeps_matching_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "const pages = import.meta.glob(['./pages/*.tsx', '!./pages/skip.tsx']);\n\
                 export default function Page() { return null; }\n\
                 void pages;\n",
            ),
            (
                "app/pages/dashboard.tsx",
                "export function Dashboard() { return null; }\nexport const dashboardHelper = 1;\n",
            ),
            (
                "app/pages/settings.tsx",
                "export function Settings() { return null; }\n",
            ),
            (
                "app/pages/skip.tsx",
                "export function Skipped() { return null; }\n",
            ),
            (
                "components/dead.tsx",
                "export function DeadComponent() { return null; }\n",
            ),
        ]);
        assert!(is_live(&snapshot, "app/pages/dashboard.tsx#Dashboard"));
        assert!(is_live(
            &snapshot,
            "app/pages/dashboard.tsx#dashboardHelper"
        ));
        assert!(is_live(&snapshot, "app/pages/settings.tsx#Settings"));
        assert!(is_candidate(&snapshot, "app/pages/skip.tsx#Skipped"));
        assert!(is_candidate(&snapshot, "components/dead.tsx#DeadComponent"));
    }

    #[test]
    fn require_context_respects_the_recursion_flag() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "const icons = require.context('../icons', false, /\\.tsx$/);\n\
                 export default function Page() { return null; }\n\
                 void icons;\n",
            ),
            (
                "icons/star.tsx",
                "export function StarIcon() { return null; }\n",
            ),
            (
                "icons/nested/arrow.tsx",
                "export function ArrowIcon() { return null; }\n",
            ),
        ]);
        assert!(is_live(&snapshot, "icons/star.tsx#StarIcon"));
        assert!(is_candidate(&snapshot, "icons/nested/arrow.tsx#ArrowIcon"));
    }

    #[test]
    fn glob_excludes_anchor_at_the_common_base() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "const pages = import.meta.glob(['./pages/a/*.tsx', './pages/b/*.tsx', '!**/skip.tsx']);\n\
                 export default function Page() { return null; }\n\
                 void pages;\n",
            ),
            (
                "app/pages/a/live.tsx",
                "export function ALive() { return null; }\n",
            ),
            (
                "app/pages/a/skip.tsx",
                "export function ASkip() { return null; }\n",
            ),
            (
                "app/pages/b/live.tsx",
                "export function BLive() { return null; }\n",
            ),
            (
                "app/pages/b/skip.tsx",
                "export function BSkip() { return null; }\n",
            ),
        ]);
        assert!(is_live(&snapshot, "app/pages/a/live.tsx#ALive"));
        assert!(is_live(&snapshot, "app/pages/b/live.tsx#BLive"));
        assert!(is_candidate(&snapshot, "app/pages/a/skip.tsx#ASkip"));
        assert!(is_candidate(&snapshot, "app/pages/b/skip.tsx#BSkip"));
    }

    #[test]
    fn glob_patterns_resolve_aliases_and_braces() {
        let snapshot = snapshot(&[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
            ),
            (
                "app/page.tsx",
                "const pages = import.meta.glob('@/pages/*.{ts,tsx}');\n\
                 export default function Page() { return null; }\n\
                 void pages;\n",
            ),
            (
                "src/pages/dashboard.ts",
                "export function Dashboard() { return null; }\n",
            ),
            (
                "src/pages/settings.tsx",
                "export function Settings() { return null; }\n",
            ),
        ]);
        assert!(is_live(&snapshot, "src/pages/dashboard.ts#Dashboard"));
        assert!(is_live(&snapshot, "src/pages/settings.tsx#Settings"));
    }

    #[test]
    fn dynamic_glob_patterns_fail_closed() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "const pages = import.meta.glob(pattern);\n\
                 export default function Page() { return null; }\n\
                 void pages;\n",
            ),
            (
                "app/pages/dashboard.tsx",
                "export function Dashboard() { return null; }\n",
            ),
        ]);
        assert!(
            snapshot
                .unknowns
                .iter()
                .any(|unknown| unknown.scope == "repo"),
            "{:?}",
            snapshot.unknowns
        );
        assert!(!is_candidate(
            &snapshot,
            "app/pages/dashboard.tsx#Dashboard"
        ));
    }
}
