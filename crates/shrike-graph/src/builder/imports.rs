use std::collections::{BTreeMap, BTreeSet};

use shrike_core::ids::module_symbol_key;
use shrike_core::{Diagnostic, Span, normalize_relative};
use shrike_syntax::{DynamicRecord, DynamicSpecifier, ParseStatus};

use super::Builder;
use crate::types::{ModuleRecord, UnknownRecord};
use crate::util::{edge, parent_dir};

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
                                "dynamic",
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
