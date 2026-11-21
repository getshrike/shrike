use std::collections::{BTreeMap, BTreeSet};

use shrike_core::glob::glob_match;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_core::{Diagnostic, Span};
use shrike_syntax::{FileSummary, ParseStatus};

use super::Builder;
use crate::types::{FileRecord, SymbolRecord};
use crate::util::{declaration_kind, edge};

/// Next.js server-only, route-config, and route-handler exports. Next.js
/// rejects a `"use client"` module that exports any of these names at build
/// time.
///
/// This is fallow 3.45.0's `ILLEGAL_CLIENT_EXPORTS` set
/// (`crates/core/src/analyze/invalid_client_exports.rs`). Knip 5.88.1 does not
/// report the condition. Re-exports are deliberately not included: fallow
/// leaves `export { metadata } from "./server"` alone, and the re-exported
/// binding belongs to another module.
const NEXT_SERVER_ONLY_EXPORTS: &[&str] = &[
    // Metadata APIs.
    "metadata",
    "generateMetadata",
    "viewport",
    "generateViewport",
    // Route-segment static params.
    "generateStaticParams",
    // Route-segment config options.
    "dynamic",
    "dynamicParams",
    "revalidate",
    "fetchCache",
    "runtime",
    "preferredRegion",
    "maxDuration",
    // Legacy Pages Router data functions.
    "getServerSideProps",
    "getStaticProps",
    "getStaticPaths",
    // App Router route HTTP method handlers.
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
];

/// App Router convention stems that own a URL. A URL has at most one owner,
/// whether a `page` or a `route`, so both share the collision namespace.
const NEXT_URL_LEAF_STEMS: &[&str] = &["page", "route"];

/// App Router convention stems that decorate a segment without owning a URL.
/// They never collide, but they still prove a dynamic segment exists, so they
/// participate in the dynamic-segment-name check.
const NEXT_ROUTE_DECORATOR_STEMS: &[&str] = &[
    "layout",
    "template",
    "loading",
    "error",
    "not-found",
    "default",
    "global-error",
    "global-not-found",
    "forbidden",
    "unauthorized",
    "icon",
    "apple-icon",
    "opengraph-image",
    "twitter-image",
    "manifest",
    "sitemap",
    "robots",
];

/// One directory segment between the App Router root and a route file.
#[derive(Clone, Copy)]
enum RouteSegment<'a> {
    /// `(group)`: transparent to the URL.
    Group,
    /// `@slot`: forks the render target without contributing to the URL.
    Slot(&'a str),
    /// `[param]`, `[...param]`, or `[[...param]]`, kept as written.
    Dynamic(&'a str),
    /// An ordinary literal path segment.
    Literal(&'a str),
}

/// A file under `<package>/app` or `<package>/src/app` that follows an App
/// Router convention.
struct AppRoute<'a> {
    /// Package-relative app root, so two independent apps never share a bucket.
    app_root: String,
    /// The convention stem (`page`, `route`, `layout`, ...).
    stem: &'a str,
    /// Directory segments between the app root and the file.
    segments: Vec<RouteSegment<'a>>,
}

/// Classify a file as an App Router route file anchored at its package root.
///
/// A bare directory named `app` is never an app root: only `<package>/app` and
/// `<package>/src/app` qualify. Private `_folder` and intercepting `(.)`,
/// `(..)`, `(...)` segments opt the subtree out of routing.
fn classify_app_route<'a>(
    path: &'a str,
    package_directory: &str,
    extensions: Option<&BTreeSet<String>>,
) -> Option<AppRoute<'a>> {
    let package_path = if package_directory.is_empty() {
        path
    } else {
        path.strip_prefix(&format!("{package_directory}/"))?
    };
    let (app_root_suffix, rest) = if let Some(rest) = package_path.strip_prefix("app/") {
        ("app", rest)
    } else {
        let rest = package_path.strip_prefix("src/app/")?;
        ("src/app", rest)
    };
    let mut parts: Vec<&str> = rest.split('/').collect();
    let file_name = parts.pop()?;
    let stem = super::entries::next_convention_stem(file_name, extensions);
    if !NEXT_URL_LEAF_STEMS.contains(&stem) && !NEXT_ROUTE_DECORATOR_STEMS.contains(&stem) {
        return None;
    }
    let mut segments = Vec::with_capacity(parts.len());
    for segment in parts {
        if segment.starts_with('_')
            || segment.starts_with("(.)")
            || segment.starts_with("(..)")
            || segment.starts_with("(...)")
        {
            return None;
        }
        segments.push(if let Some(name) = segment.strip_prefix('@') {
            RouteSegment::Slot(name)
        } else if segment.starts_with('[') && segment.ends_with(']') {
            RouteSegment::Dynamic(segment)
        } else if segment.starts_with('(') && segment.ends_with(')') {
            RouteSegment::Group
        } else {
            RouteSegment::Literal(segment)
        });
    }
    let app_root = if package_directory.is_empty() {
        app_root_suffix.to_string()
    } else {
        format!("{package_directory}/{app_root_suffix}")
    };
    Some(AppRoute {
        app_root,
        stem,
        segments,
    })
}

/// The `(app root, slot, URL)` bucket. Two URL leaves in the same bucket
/// collide; route groups are stripped and parallel slots fork the bucket.
fn route_collision_bucket(route: &AppRoute<'_>) -> (String, String, String) {
    let mut slots: Vec<&str> = Vec::new();
    let mut url_parts: Vec<&str> = Vec::new();
    for segment in &route.segments {
        match segment {
            RouteSegment::Group => {}
            RouteSegment::Slot(name) => slots.push(name),
            RouteSegment::Dynamic(raw) | RouteSegment::Literal(raw) => url_parts.push(raw),
        }
    }
    let url = if url_parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", url_parts.join("/"))
    };
    (route.app_root.clone(), slots.join("/"), url)
}

/// A dynamic segment occurrence: the slot it lives in, its parent URL position
/// with route groups stripped, and its written spelling.
struct DynamicOccurrence<'a> {
    slot_key: String,
    position: String,
    spelling: &'a str,
}

fn dynamic_occurrences<'a>(route: &AppRoute<'a>) -> Vec<DynamicOccurrence<'a>> {
    let mut slots: Vec<&str> = Vec::new();
    let mut url_parts: Vec<&str> = Vec::new();
    let mut occurrences = Vec::new();
    for segment in &route.segments {
        match segment {
            RouteSegment::Group => {}
            RouteSegment::Slot(name) => slots.push(name),
            RouteSegment::Dynamic(raw) => {
                let position = if url_parts.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{}", url_parts.join("/"))
                };
                occurrences.push(DynamicOccurrence {
                    slot_key: slots.join("/"),
                    position,
                    spelling: raw,
                });
                url_parts.push(raw);
            }
            RouteSegment::Literal(raw) => url_parts.push(raw),
        }
    }
    occurrences
}

impl<'a> Builder<'a> {
    pub(super) fn build_files(&mut self) {
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
                let component = matches!(
                    summary
                        .path
                        .rsplit_once('.')
                        .map(|(_, extension)| extension),
                    Some("vue" | "svelte" | "astro")
                );
                if !component {
                    diagnostics.push(
                        Diagnostic::warning(
                            "unparsed_file",
                            "file could not be parsed; its symbols are not deletion candidates",
                        )
                        .in_file(&summary.path),
                    );
                }
            }
        }
        self.diagnostics = diagnostics;
    }

    /// Warn when a `"use client"` module exports a Next.js server-only,
    /// route-config, or route-handler name. The rule is project-wide: fallow
    /// collects the union of the root and workspace manifests, because a shared
    /// `"use client"` module is compiled in the consuming Next app's context.
    /// The root manifest is checked directly because the resolver only records
    /// packages that have a `name`.
    ///
    /// `directly_exported` distinguishes `export const metadata` from a local
    /// alias that renames the declaration away
    /// (`const metadata; export { metadata as config }`).
    pub(super) fn detect_invalid_client_exports(&mut self) {
        let next_project = super::entries::package_declares_next(self.resolver.root(), "")
            == Some(true)
            || self.resolver.packages().iter().any(|package| {
                super::entries::package_declares_next(self.resolver.root(), &package.directory)
                    == Some(true)
            });
        if !next_project {
            return;
        }
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok || !summary.has_directive("use client") {
                continue;
            }
            let mut invalid: Vec<(&str, Span)> = Vec::new();
            for declaration in &summary.declarations {
                if !declaration.directly_exported || declaration.default || declaration.type_only {
                    continue;
                }
                for name in &declaration.names {
                    if NEXT_SERVER_ONLY_EXPORTS.contains(&name.as_str()) {
                        invalid.push((name.as_str(), declaration.span));
                    }
                }
            }
            for local_export in &summary.local_exports {
                if local_export.type_only {
                    continue;
                }
                let name = local_export.exported.as_str();
                if !NEXT_SERVER_ONLY_EXPORTS.contains(&name) {
                    continue;
                }
                // One finding per exported name. Prefer the export specifier's
                // span so `const metadata; export { metadata }` points at the
                // line a developer must change, matching fallow.
                if let Some(existing) = invalid.iter_mut().find(|(existing, _)| *existing == name) {
                    existing.1 = local_export.span;
                } else {
                    invalid.push((name, local_export.span));
                }
            }
            for (name, span) in invalid {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "invalid_client_export",
                        format!(
                            "`{name}` is a Next.js server-only export and cannot be exported from a \"use client\" module"
                        ),
                    )
                    .in_file(&summary.path)
                    .at(span),
                );
            }
        }
    }

    /// Report App Router files that Next.js cannot route unambiguously.
    ///
    /// `route_collision`: two `page`/`route` leaves in one app root resolve to
    /// the same URL. Next.js fails the build with "You cannot have two parallel
    /// pages that resolve to the same path".
    ///
    /// `dynamic_segment_name_conflict`: sibling dynamic segments at one tree
    /// position use different spellings (`[id]` vs `[slug]`). `next build`
    /// succeeds, but the route throws the first time it is resolved.
    ///
    /// Both are gated on the owning package declaring `next`; a shared `app/`
    /// directory in a non-Next package is not an app root. Route groups are
    /// transparent, parallel slots fork the namespace, and private/intercepting
    /// segments are excluded. This mirrors fallow 3.45.0's `route_tree`
    /// primitive while keeping Shrike's per-package Next gate.
    pub(super) fn detect_next_route_conflicts(&mut self) {
        let summaries = self.summaries.clone();
        let page_extensions = self.configured_page_extensions();
        let mut collisions: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
        let mut positions: BTreeMap<(String, String, String), BTreeMap<String, BTreeSet<String>>> =
            BTreeMap::new();
        for summary in &summaries {
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or_else(String::new, |package| package.directory.clone());
            // A diagnostic should not claim a Next.js failure without positive
            // evidence. The entry rules fail open on a missing manifest; a
            // routing error must not.
            if super::entries::package_declares_next(self.resolver.root(), &package_directory)
                != Some(true)
            {
                continue;
            }
            let Some(route) = classify_app_route(
                &summary.path,
                &package_directory,
                page_extensions.get(&package_directory),
            ) else {
                continue;
            };
            if NEXT_URL_LEAF_STEMS.contains(&route.stem) {
                collisions
                    .entry(route_collision_bucket(&route))
                    .or_default()
                    .push(summary.path.clone());
            }
            for occurrence in dynamic_occurrences(&route) {
                positions
                    .entry((
                        route.app_root.clone(),
                        occurrence.slot_key,
                        occurrence.position,
                    ))
                    .or_default()
                    .entry(occurrence.spelling.to_string())
                    .or_default()
                    .insert(summary.path.clone());
            }
        }
        for ((_, _, url), mut files) in collisions {
            files.sort();
            files.dedup();
            if files.len() < 2 {
                continue;
            }
            for file in &files {
                let conflicting: Vec<&str> = files
                    .iter()
                    .filter(|other| *other != file)
                    .map(String::as_str)
                    .collect();
                self.diagnostics.push(
                    Diagnostic::error(
                        "route_collision",
                        format!(
                            "two or more Next.js routes resolve to `{url}`; conflicting files: {}",
                            conflicting.join(", ")
                        ),
                    )
                    .in_file(file),
                );
            }
        }
        for ((_, _, position), spellings) in positions {
            if spellings.len() < 2 {
                continue;
            }
            let names: Vec<&str> = spellings.keys().map(String::as_str).collect();
            let mut files: BTreeSet<&str> = BTreeSet::new();
            for paths in spellings.values() {
                files.extend(paths.iter().map(String::as_str));
            }
            for file in files {
                let mut conflicting: Vec<&str> = spellings
                    .values()
                    .flat_map(|paths| paths.iter().map(String::as_str))
                    .filter(|other| *other != file)
                    .collect();
                conflicting.sort();
                conflicting.dedup();
                self.diagnostics.push(
                    Diagnostic::error(
                        "dynamic_segment_name_conflict",
                        format!(
                            "sibling Next.js dynamic segments at `{position}` use different names: {}; conflicting files: {}",
                            names.join(", "),
                            conflicting.join(", ")
                        ),
                    )
                    .in_file(file),
                );
            }
        }
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

    pub(super) fn build_symbols(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                // Framework files with their own compilers (Svelte, Astro, Vue)
                // are still entries. They have no parsed symbols, so only the
                // module key exists; `detect_entries` marks it live.
                if super::entries::framework_module(&summary.path).is_some() {
                    self.add_symbol(
                        summary,
                        "__module__",
                        "module",
                        Span::default(),
                        false,
                        false,
                    );
                    self.exports.insert(summary.path.clone(), BTreeMap::new());
                    self.locals.insert(summary.path.clone(), BTreeMap::new());
                }
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
                let key = self.add_alias_symbol(
                    summary,
                    &local_export.exported,
                    local_export.span,
                    local_export.type_only,
                );
                export_map.insert(local_export.exported.clone(), key.clone());
                if let Some(local) = local_map.get(&local_export.local).cloned() {
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
}
