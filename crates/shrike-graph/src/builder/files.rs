use std::collections::{BTreeMap, BTreeSet, VecDeque};

use shrike_core::glob::glob_match;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_core::{Diagnostic, Span};
use shrike_syntax::{
    BindingKind, DeclarationKind, DynamicKind, DynamicSpecifier, ExportedConstValue, FileSummary,
    ImportRecord, ParseStatus,
};

use super::Builder;
use crate::types::{FileRecord, SymbolRecord};
use crate::util::{declaration_kind, edge};

/// Server-only exports that Next.js rejects from a `"use client"` page or
/// layout at build time.
///
/// Verified against Next.js 16.4.6 with real `next build` probes. A client
/// page or layout fails for all five names. A client route handler fails for
/// `metadata`, `generateMetadata`, and `revalidate`, but ignores `viewport`
/// and `generateViewport`, so [`NEXT_ROUTE_HANDLER_SERVER_ONLY_EXPORTS`] is the
/// narrower set used for `route` files.
///
/// Fallow 3.45.0's `ILLEGAL_CLIENT_EXPORTS` also lists `dynamic`,
/// `dynamicParams`, `fetchCache`, `runtime`, `preferredRegion`, `maxDuration`,
/// the HTTP method handlers, and the legacy Pages Router data functions. Those
/// builds succeed in Next 16.4.6, so reporting them would be a false positive.
/// Next also ignores these exports in `template`, `not-found`, `error`, and
/// `loading` files, and in client modules that are not route files.
const NEXT_SERVER_ONLY_EXPORTS: &[&str] = &[
    "metadata",
    "generateMetadata",
    "viewport",
    "generateViewport",
    "revalidate",
];

/// Server-only exports Next.js rejects from a `"use client"` route handler.
/// `viewport` and `generateViewport` are not part of a route handler's config
/// surface and a client route handler exporting them builds successfully.
const NEXT_ROUTE_HANDLER_SERVER_ONLY_EXPORTS: &[&str] =
    &["metadata", "generateMetadata", "revalidate"];

/// Bundlers that honor `"use client"` / `"use server"` directives. This is
/// fallow 3.45.0's `RSC_BUNDLER_DEPS` predicate: Next.js plus the
/// framework-agnostic React Server Components toolchains.
const RSC_BUNDLER_PACKAGES: &[&str] = &[
    "next",
    "waku",
    "@lazarv/react-server",
    "react-server-dom-webpack",
    "react-server-dom-vite",
    "react-server-dom-parcel",
    "@vitejs/plugin-rsc",
];

/// Packages whose import makes a module server-only. This is fallow 3.45.0's
/// `SERVER_ONLY_PACKAGES` set (`crates/core/src/analyze/server_only.rs`). The
/// `node:` and bare forms are both listed because the check matches the
/// specifier as written.
const SERVER_ONLY_PACKAGES: &[&str] = &[
    "server-only",
    "next/server",
    "node:fs",
    "fs",
    "node:fs/promises",
    "fs/promises",
    "node:child_process",
    "child_process",
];

/// Named `next/headers` APIs that only run on the server. A namespace import
/// counts conservatively because any member access could reach one of these;
/// a side-effect or default import does not.
const NEXT_HEADERS_SERVER_NAMES: &[&str] = &["cookies", "headers", "draftMode"];

/// Whether a module is a server-only origin for the mixed-barrel rule: it
/// carries `"use server"`, imports the `server-only` poison package or another
/// known server runtime, or imports a named server API from `next/headers`.
///
/// Type-only imports are skipped: they are erased before bundling, so they do
/// not drag server code into a client bundle. Fallow 3.45.0 does not make this
/// distinction; Shrike deliberately prefers the runtime-accurate reading.
fn summary_is_server_only(summary: &FileSummary) -> bool {
    summary.has_directive("use server") || summary.imports.iter().any(import_is_server_only)
}

fn import_is_server_only(import: &ImportRecord) -> bool {
    server_only_import_reason(import).is_some()
}

/// Whether an import has any binding that survives type erasure. A side-effect
/// import always runs. An import whose every binding is `type` is erased and
/// cannot carry server code into the client bundle.
fn import_has_runtime_binding(import: &ImportRecord) -> bool {
    if import.type_only {
        return false;
    }
    import.side_effect_only || import.bindings.iter().any(|binding| !binding.type_only)
}

/// The human-readable reason an import reaches server-only code, or `None`.
/// A bare `next/headers` side-effect import is deliberately not a sink: only a
/// namespace or one of the named server APIs is.
fn server_only_import_reason(import: &ImportRecord) -> Option<String> {
    if !import_has_runtime_binding(import) {
        return None;
    }
    if SERVER_ONLY_PACKAGES.contains(&import.source.as_str()) {
        return Some(format!("imports `{}`", import.source));
    }
    if import.source != "next/headers" {
        return None;
    }
    import.bindings.iter().find_map(|binding| {
        if binding.type_only {
            return None;
        }
        match binding.kind {
            BindingKind::Namespace => Some("imports the `next/headers` namespace".to_string()),
            BindingKind::Named
                if NEXT_HEADERS_SERVER_NAMES.contains(&binding.imported.as_str()) =>
            {
                Some(format!(
                    "imports `{}` from `next/headers`",
                    binding.imported
                ))
            }
            BindingKind::Named | BindingKind::Default => None,
        }
    })
}

/// The first import in a module that reaches server-only code, with the reason
/// and the import's source span.
fn summary_server_only_import_reason(summary: &FileSummary) -> Option<(String, Span)> {
    summary
        .imports
        .iter()
        .find_map(|import| server_only_import_reason(import).map(|reason| (reason, import.span)))
}

/// Whether a `"use server"` module contains only Server Action exports.
///
/// The parser classifies the module from the AST, including async function
/// declarations, async `const` arrow/function expressions, overload and
/// ambient declarations, type exports, and local/default export aliases. Such
/// a module is replaced by action references at the client boundary, so the
/// leak cone stops there. A non-action export (a top-level const, a default
/// value, or a re-export) keeps the cone walking.
fn summary_is_server_action_module(summary: &FileSummary) -> bool {
    summary.server_action_module
}

/// Whether a module has a runtime static edge to `specifier`. When it does, a
/// `next/dynamic(..., { ssr: false })` import of the same specifier is not the
/// only way the target is reached, so skipping the dynamic edge cannot hide a
/// leak.
fn summary_has_static_specifier(summary: &FileSummary, specifier: &str) -> bool {
    summary
        .imports
        .iter()
        .any(|import| import.source == specifier && import_has_runtime_binding(import))
        || summary
            .re_exports
            .iter()
            .any(|re_export| re_export.source == specifier && !re_export.type_only)
        || summary
            .export_alls
            .iter()
            .any(|export_all| export_all.source == specifier && !export_all.type_only)
}

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

/// Whether a statically extracted value satisfies Next.js's
/// `unstable_dynamicStaleTime` schema: a non-negative integer number.
fn valid_stale_time(value: &ExportedConstValue) -> bool {
    let ExportedConstValue::Number(value) = value else {
        return false;
    };
    value
        .parse::<f64>()
        .is_ok_and(|number| number.is_finite() && number >= 0.0 && number.fract() == 0.0)
}

/// A file under `<package>/pages` or `<package>/src/pages` that Next.js
/// treats as a Pages Router page.
struct PagesRouterPage {
    /// Whether a `[param]` segment makes the route dynamic.
    dynamic: bool,
    /// Whether the file is an API route under `pages/api`.
    api: bool,
    /// Whether the file is a reserved `_app`, `_document`, or `_error` page.
    reserved: bool,
}

/// Classify a file as a Pages Router page anchored at its package root.
///
/// Next.js routes only files whose extension is in `pageExtensions`
/// (`tsx`, `ts`, `jsx`, and `js` by default) and reserves `_app`,
/// `_document`, `_error`, and the `api/` subtree. A nested `_app` is an
/// ordinary page, matching Next.js's root-anchored `RESERVED_PAGE` regex.
fn classify_pages_router_page(
    path: &str,
    package_directory: &str,
    extensions: Option<&BTreeSet<String>>,
) -> Option<PagesRouterPage> {
    let package_path = if package_directory.is_empty() {
        path
    } else {
        path.strip_prefix(&format!("{package_directory}/"))?
    };
    let rest = if let Some(rest) = package_path.strip_prefix("pages/") {
        rest
    } else {
        package_path.strip_prefix("src/pages/")?
    };
    let file_name = rest.rsplit('/').next()?;
    if !pages_router_page_extension(file_name, extensions) {
        return None;
    }
    let mut parts: Vec<&str> = rest.split('/').collect();
    let file = parts.pop()?;
    let stem = super::entries::next_convention_stem(file, extensions);
    if stem != "index" {
        parts.push(stem);
    }
    let route = if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    };
    let dynamic = parts
        .iter()
        .any(|segment| segment.starts_with('[') && segment.ends_with(']'));
    let api = parts.first().is_some_and(|segment| *segment == "api");
    let reserved = matches!(route.as_str(), "/_app" | "/_document" | "/_error");
    Some(PagesRouterPage {
        dynamic,
        api,
        reserved,
    })
}

/// Whether a Pages Router file uses a page extension. A configured
/// `pageExtensions` list replaces Next.js's default entirely.
fn pages_router_page_extension(file_name: &str, extensions: Option<&BTreeSet<String>>) -> bool {
    match extensions {
        Some(extensions) => extensions
            .iter()
            .any(|extension| file_name.ends_with(&format!(".{extension}"))),
        None => ["tsx", "ts", "jsx", "js"]
            .iter()
            .any(|extension| file_name.ends_with(&format!(".{extension}"))),
    }
}

/// The span of a value export Next.js sees on the page module namespace.
///
/// Type-only exports are erased before Next.js imports the page, and a
/// default-exported function or class named `getStaticProps` is a component,
/// not a data function. `export * as getStaticProps` is deliberately not
/// recognized: Next.js's analyzer only reads named export specifiers.
fn pages_router_export_span(summary: &FileSummary, name: &str) -> Option<Span> {
    for declaration in &summary.declarations {
        if declaration.exported
            && !declaration.default
            && !declaration.type_only
            && declaration.names.iter().any(|candidate| candidate == name)
        {
            return Some(declaration.span);
        }
    }
    for local in &summary.local_exports {
        if !local.type_only && local.exported == name {
            return Some(local.span);
        }
    }
    for re_export in &summary.re_exports {
        if !re_export.type_only && re_export.exported == name {
            return Some(re_export.span);
        }
    }
    None
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

    /// Report `"use client"` route files that export a Next.js server-only API.
    ///
    /// Verified against Next.js 16.4.6 with real `next build` probes: the app
    /// loader rejects `metadata`, `generateMetadata`, `viewport`,
    /// `generateViewport`, and `revalidate` from a client page or layout, and
    /// `metadata`, `generateMetadata`, and `revalidate` from a client route
    /// handler. The same names are ignored in `template`, `not-found`, `error`,
    /// and `loading` files and in client modules that are not route files, so
    /// those are not reported. `dynamic`, `dynamicParams`, `fetchCache`,
    /// `runtime`, `preferredRegion`, `maxDuration`, and the HTTP method handlers
    /// build successfully even though fallow 3.45.0 lists them.
    ///
    /// The rule is per package: only an app route owned by a package that
    /// declares `next` is a Next.js route. `directly_exported` distinguishes
    /// `export const metadata` from a local alias that renames the declaration
    /// away (`const metadata; export { metadata as config }`). Explicit export
    /// specifiers are included even when type-only because Next.js flags the
    /// exported name; local `interface` and `type` declarations are erased and
    /// are not reported.
    pub(super) fn detect_invalid_client_exports(&mut self) {
        let summaries = self.summaries.clone();
        let page_extensions = self.configured_page_extensions();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok || !summary.has_directive("use client") {
                continue;
            }
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or_else(String::new, |package| package.directory.clone());
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
            let names: &[&str] = match route.stem {
                "page" | "layout" => NEXT_SERVER_ONLY_EXPORTS,
                "route" => NEXT_ROUTE_HANDLER_SERVER_ONLY_EXPORTS,
                _ => continue,
            };
            let mut invalid: Vec<(&str, Span)> = Vec::new();
            for declaration in &summary.declarations {
                if !declaration.directly_exported || declaration.default || declaration.type_only {
                    continue;
                }
                if matches!(
                    declaration.kind,
                    DeclarationKind::Type | DeclarationKind::Interface
                ) {
                    continue;
                }
                for name in &declaration.names {
                    if names.contains(&name.as_str()) {
                        invalid.push((name.as_str(), declaration.span));
                    }
                }
            }
            for local_export in &summary.local_exports {
                let name = local_export.exported.as_str();
                if !names.contains(&name) {
                    continue;
                }
                // One finding per exported name. Prefer the export specifier's
                // span so `const metadata; export { metadata }` points at the
                // line a developer must change.
                if let Some(existing) = invalid.iter_mut().find(|(existing, _)| *existing == name) {
                    existing.1 = local_export.span;
                } else {
                    invalid.push((name, local_export.span));
                }
            }
            for re_export in &summary.re_exports {
                let name = re_export.exported.as_str();
                if !names.contains(&name) || invalid.iter().any(|(existing, _)| *existing == name) {
                    continue;
                }
                invalid.push((name, re_export.span));
            }
            for export_all in &summary.export_alls {
                let Some(name) = export_all.exported.as_deref() else {
                    continue;
                };
                if !names.contains(&name) || invalid.iter().any(|(existing, _)| *existing == name) {
                    continue;
                }
                invalid.push((name, export_all.span));
            }
            for (name, span) in invalid {
                self.diagnostics.push(
                    Diagnostic::error(
                        "invalid_client_export",
                        format!(
                            "`{name}` cannot be exported from a `\"use client\"` Next.js route; Next.js resolves it on the server and fails the build"
                        ),
                    )
                    .in_file(&summary.path)
                    .at(span),
                );
            }
        }
    }

    /// Value exports of a `"use server"` module that Next.js rejects as
    /// actions. The runtime validator throws when an export is not a function,
    /// and the TypeScript plugin rejects variable initializers that cannot be
    /// async functions, so this is a build-failure diagnostic. The parser only
    /// records the definite union; arrows, function expressions, calls, and
    /// identifiers are skipped because they may be Promise-returning functions.
    pub(super) fn detect_invalid_server_exports(&mut self) {
        if !self.project_uses_rsc_directives() {
            return;
        }
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            for export in &summary.invalid_server_exports {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "invalid_server_export",
                        format!(
                            "`{}` cannot be exported from a \"use server\" module; Next.js requires every action export to be an async function",
                            export.name
                        ),
                    )
                    .in_file(&summary.path)
                    .at(export.span),
                );
            }
        }
    }

    /// Whether the project uses a React Server Components bundler. The gate is
    /// project-wide, matching fallow 3.45.0's `project_uses_rsc_directives`:
    /// a shared module may be compiled by a sibling RSC app even when its own
    /// package does not declare the bundler. The root manifest is checked
    /// directly because the resolver only records packages that have a name.
    pub(super) fn project_uses_rsc_directives(&self) -> bool {
        super::entries::package_declares_any(self.resolver.root(), "", RSC_BUNDLER_PACKAGES)
            == Some(true)
            || self.resolver.packages().iter().any(|package| {
                super::entries::package_declares_any(
                    self.resolver.root(),
                    &package.directory,
                    RSC_BUNDLER_PACKAGES,
                ) == Some(true)
            })
    }

    /// Warn when a `"use client"` / `"use server"` string is not in the leading
    /// prologue. RSC bundlers silently ignore it, so the intended boundary is
    /// never created.
    pub(super) fn detect_misplaced_directives(&mut self) {
        if !self.project_uses_rsc_directives() {
            return;
        }
        let summaries = self.summaries.clone();
        for summary in &summaries {
            for site in &summary.misplaced_directives {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "misplaced_directive",
                        format!(
                            "\"{}\" is ignored because it is not in the leading prologue; move it to the top of the file",
                            site.directive
                        ),
                    )
                    .in_file(&summary.path)
                    .at(site.span),
                );
            }
        }
    }

    /// Warn when a barrel re-exports both a `"use client"` origin and a
    /// server-only origin. Importing either name from the barrel drags the
    /// other's directive context across the React Server Components boundary.
    ///
    /// Only direct re-export origins are classified, and only when they resolve
    /// to a local module. A type-only re-export is erased and carries no
    /// runtime directive. The first client and first server origin in source
    /// order are named, and the warning is anchored on the earlier of the two.
    /// This mirrors fallow 3.45.0's `mixed_client_server_barrel` detector,
    /// except that type-only imports do not make an origin server-only.
    pub(super) fn detect_mixed_client_server_barrels(&mut self) {
        if !self.project_uses_rsc_directives() {
            return;
        }
        let summaries = self.summaries.clone();
        let by_path: BTreeMap<&str, &FileSummary> = summaries
            .iter()
            .map(|summary| (summary.path.as_str(), summary))
            .collect();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok
                || (summary.re_exports.is_empty() && summary.export_alls.is_empty())
            {
                continue;
            }
            // Explicit re-exports and `export *` are stored separately, so
            // collect both and restore source order before classifying.
            let mut origins: Vec<(u32, Span, &str, &FileSummary)> = Vec::new();
            for re_export in &summary.re_exports {
                if re_export.type_only {
                    continue;
                }
                let Some(target) = self
                    .resolver
                    .resolve(&summary.path, &re_export.source)
                    .target
                else {
                    continue;
                };
                let Some(origin) = by_path.get(target.as_str()) else {
                    continue;
                };
                origins.push((
                    re_export.span.start,
                    re_export.span,
                    re_export.source.as_str(),
                    *origin,
                ));
            }
            for export_all in &summary.export_alls {
                if export_all.type_only {
                    continue;
                }
                let Some(target) = self
                    .resolver
                    .resolve(&summary.path, &export_all.source)
                    .target
                else {
                    continue;
                };
                let Some(origin) = by_path.get(target.as_str()) else {
                    continue;
                };
                origins.push((
                    export_all.span.start,
                    export_all.span,
                    export_all.source.as_str(),
                    *origin,
                ));
            }
            origins.sort_by_key(|(start, _, _, _)| *start);
            let mut client: Option<(Span, &str)> = None;
            let mut server: Option<(Span, &str)> = None;
            for (_, span, source, origin) in origins {
                // `else if`: a module that is somehow both client and
                // server-only counts as the client origin, as in fallow.
                if client.is_none() && origin.has_directive("use client") {
                    client = Some((span, source));
                } else if server.is_none() && summary_is_server_only(origin) {
                    server = Some((span, source));
                }
            }
            let (Some((client_span, client_source)), Some((server_span, server_source))) =
                (client, server)
            else {
                continue;
            };
            let anchor = if client_span.start <= server_span.start {
                client_span
            } else {
                server_span
            };
            self.diagnostics.push(
                Diagnostic::warning(
                    "mixed_client_server_barrel",
                    format!(
                        "barrel re-exports both a \"use client\" module (`{client_source}`) and a server-only module (`{server_source}`); one import drags the other's directive across the RSC boundary"
                    ),
                )
                .in_file(&summary.path)
                .at(anchor),
            );
        }
    }

    /// Warn when a `"use client"` module transitively imports server-only code.
    ///
    /// This is the source-level counterpart to the build-evidence
    /// `client-boundary-leak` warning: Next.js fails the client build when a
    /// client component reaches `server-only`, `next/headers`, `next/server`,
    /// or a Node server runtime. The import cone follows static imports,
    /// re-exports, and literal dynamic imports. Type-only edges are erased and
    /// skipped. A `"use server"` module whose exports are all async functions
    /// is the sanctioned Server Action boundary: the bundler replaces the
    /// import with an action reference, so the module and its imports never
    /// enter the client bundle. A `next/dynamic(..., { ssr: false })` edge is
    /// the sanctioned client-only escape hatch and is skipped when the same
    /// specifier has no runtime static edge.
    ///
    /// One warning is emitted per client root, anchored on the first import of
    /// the shortest path. This mirrors fallow 3.45.0's `client-server-leak`
    /// rule with the `server-only-import` category.
    pub(super) fn detect_client_server_leaks(&mut self) {
        if !self.project_uses_rsc_directives() {
            return;
        }
        let summaries = self.summaries.clone();
        let by_path: BTreeMap<String, &FileSummary> = summaries
            .iter()
            .map(|summary| (summary.path.clone(), summary))
            .collect();
        let sinks: BTreeMap<String, String> = summaries
            .iter()
            .filter_map(|summary| {
                summary_server_only_import_reason(summary)
                    .map(|(reason, _)| (summary.path.clone(), reason))
            })
            .collect();
        let action_modules: BTreeSet<String> = summaries
            .iter()
            .filter(|summary| summary_is_server_action_module(summary))
            .map(|summary| summary.path.clone())
            .collect();
        let mut diagnostics = Vec::new();
        for client in &summaries {
            if client.parse_status != ParseStatus::Ok || !client.has_directive("use client") {
                continue;
            }
            if let Some((reason, span)) = summary_server_only_import_reason(client) {
                diagnostics.push(
                    Diagnostic::warning(
                        "client_server_leak",
                        format!(
                            "\"use client\" module directly imports server-only code: {reason}"
                        ),
                    )
                    .in_file(&client.path)
                    .at(span),
                );
                continue;
            }
            let mut visited: BTreeSet<String> = BTreeSet::new();
            let mut parent: BTreeMap<String, (String, Span)> = BTreeMap::new();
            let mut queue: VecDeque<String> = VecDeque::new();
            visited.insert(client.path.clone());
            queue.push_back(client.path.clone());
            let mut found: Option<String> = None;
            while let Some(current) = queue.pop_front() {
                let Some(summary) = by_path.get(&current) else {
                    continue;
                };
                for (target, span) in self.leak_edges(summary, &by_path) {
                    // An all-action Server Action module never enters the
                    // client bundle; its imports stay on the server.
                    if action_modules.contains(&target) {
                        continue;
                    }
                    if !visited.insert(target.clone()) {
                        continue;
                    }
                    parent.insert(target.clone(), (current.clone(), span));
                    if sinks.contains_key(&target) {
                        found = Some(target);
                        break;
                    }
                    queue.push_back(target);
                }
                if found.is_some() {
                    break;
                }
            }
            let Some(sink) = found else {
                continue;
            };
            let mut chain: Vec<String> = vec![sink.clone()];
            let mut cursor = sink;
            while let Some((previous, _)) = parent.get(&cursor) {
                chain.push(previous.clone());
                cursor = previous.clone();
            }
            chain.reverse();
            let anchor = chain
                .get(1)
                .and_then(|next| parent.get(next))
                .map(|(_, span)| *span)
                .filter(|span| span.line > 0);
            let reason = sinks
                .get(&chain[chain.len() - 1])
                .cloned()
                .unwrap_or_default();
            let mut diagnostic = Diagnostic::warning(
                "client_server_leak",
                format!(
                    "\"use client\" module transitively imports server-only code ({reason}); import path: {}",
                    chain.join(" -> ")
                ),
            )
            .in_file(&client.path);
            if let Some(span) = anchor {
                diagnostic = diagnostic.at(span);
            }
            diagnostics.push(diagnostic);
        }
        self.diagnostics.extend(diagnostics);
    }

    /// The runtime module edges of one summary that the leak cone can follow.
    fn leak_edges(
        &self,
        summary: &FileSummary,
        by_path: &BTreeMap<String, &FileSummary>,
    ) -> Vec<(String, Span)> {
        let mut edges = Vec::new();
        let mut add = |source: &str, span: Span| {
            if let Some(target) = self.resolver.resolve(&summary.path, source).target {
                if by_path.contains_key(&target) {
                    edges.push((target, span));
                }
            }
        };
        for import in &summary.imports {
            if import_has_runtime_binding(import) {
                add(&import.source, import.span);
            }
        }
        for re_export in &summary.re_exports {
            if !re_export.type_only {
                add(&re_export.source, re_export.span);
            }
        }
        for export_all in &summary.export_alls {
            if !export_all.type_only {
                add(&export_all.source, export_all.span);
            }
        }
        for dynamic in &summary.dynamic {
            // A worker runs in its own thread, not in the client component
            // bundle. A non-literal specifier cannot be bounded; the graph
            // records it as an unknown instead of guessing.
            if dynamic.kind == DynamicKind::Worker {
                continue;
            }
            let DynamicSpecifier::Literal(specifier) = &dynamic.specifier else {
                continue;
            };
            if summary.client_only_dynamic_specifiers.contains(specifier)
                && !summary_has_static_specifier(summary, specifier)
            {
                continue;
            }
            add(specifier, dynamic.span);
        }
        edges
    }

    /// Report App Router files whose route-segment configuration Next.js
    /// rejects at build time.
    ///
    /// Next.js's `getAppPageStaticInfo` throws for:
    /// * both `unstable_paramMatching` and `unstable_generateParamMatching`
    /// * parameter matching in a `route` handler
    /// * parameter matching with `export const runtime = 'edge'` or `"use client"`
    /// * `generateStaticParams` with `export const runtime = 'edge'` or
    ///   `"use client"`
    /// * `instant`, `prefetch`, or `ensureStatic` in a `"use client"` module
    /// * `unstable_dynamicStaleTime` in a layout or next to `instant`
    ///
    /// The parser mirrors Next.js's recognized export shapes, so a function
    /// named `instant` or a re-exported `runtime` is not reported. Only the
    /// definite value forms Next.js extracts are considered.
    pub(super) fn detect_next_segment_config_conflicts(&mut self) {
        let summaries = self.summaries.clone();
        let page_extensions = self.configured_page_extensions();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or_else(String::new, |package| package.directory.clone());
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
            let client = summary.has_directive("use client");
            let special = |name: &str| {
                summary
                    .next_segment_exports
                    .iter()
                    .find(|record| record.name == name)
            };
            let config = |name: &str| {
                summary
                    .exported_const_values
                    .iter()
                    .find(|record| record.name == name)
            };
            let generate_static_params = special("generateStaticParams");
            let param_matching = special("unstable_paramMatching");
            let generate_param_matching = special("unstable_generateParamMatching");
            let edge = config("runtime").is_some_and(|record| {
                matches!(&record.value, ExportedConstValue::String(value) if value == "edge")
            });
            let dynamic_stale_time = config("unstable_dynamicStaleTime")
                .filter(|record| valid_stale_time(&record.value));
            let instant = config("instant").filter(|record| {
                matches!(
                    &record.value,
                    ExportedConstValue::Boolean(_) | ExportedConstValue::Object
                )
            });
            let prefetch = config("prefetch").filter(|record| {
                matches!(&record.value, ExportedConstValue::String(value)
                    if matches!(value.as_str(), "auto" | "partial" | "force-disabled"))
            });
            let ensure_static = config("ensureStatic").filter(|record| {
                matches!(&record.value, ExportedConstValue::String(value)
                    if matches!(value.as_str(), "auto" | "shell" | "prefetch" | "navigation"))
                    || matches!(&record.value, ExportedConstValue::Boolean(false))
            });

            let path = &summary.path;
            let mut findings: Vec<(String, Span)> = Vec::new();
            if param_matching.is_some() && generate_param_matching.is_some() {
                findings.push((
                    format!(
                        "`{path}` cannot export both `unstable_paramMatching` and `unstable_generateParamMatching`; Next.js fails the build"
                    ),
                    param_matching
                        .or(generate_param_matching)
                        .expect("both parameter matching exports")
                        .span,
                ));
            }
            if let Some(record) = param_matching.or(generate_param_matching) {
                let param_span = record.span;
                if route.stem == "route" {
                    findings.push((
                        format!(
                            "`{path}` cannot export parameter matching from a route handler; Next.js only supports it in pages and layouts"
                        ),
                        param_span,
                    ));
                }
                if edge {
                    findings.push((
                        format!(
                            "`{path}` cannot combine `export const runtime = 'edge'` with parameter matching; Next.js fails the build"
                        ),
                        param_span,
                    ));
                }
                if client {
                    findings.push((
                        format!(
                            "`{path}` cannot combine a `\"use client\"` directive with parameter matching; Next.js fails the build"
                        ),
                        param_span,
                    ));
                }
            }
            if let Some(record) = generate_static_params {
                if edge {
                    findings.push((
                        format!(
                            "`{path}` cannot combine `export const runtime = 'edge'` with `generateStaticParams`; Next.js fails the build"
                        ),
                        record.span,
                    ));
                }
                if client {
                    findings.push((
                        format!(
                            "`{path}` cannot combine a `\"use client\"` directive with `generateStaticParams`; Next.js fails the build"
                        ),
                        record.span,
                    ));
                }
            }
            if client {
                for (name, record) in [
                    ("instant", instant),
                    ("prefetch", prefetch),
                    ("ensureStatic", ensure_static),
                ] {
                    if let Some(record) = record {
                        findings.push((
                            format!(
                                "`{name}` is a route segment config and cannot be exported from a `\"use client\"` module; Next.js fails the build"
                            ),
                            record.span,
                        ));
                    }
                }
            }
            if let Some(record) = dynamic_stale_time {
                if route.stem == "layout" {
                    findings.push((
                        format!(
                            "`{path}` cannot export `unstable_dynamicStaleTime` from a layout; Next.js only supports it in pages"
                        ),
                        record.span,
                    ));
                }
                if instant.is_some() {
                    findings.push((
                        format!(
                            "`{path}` cannot export both `unstable_dynamicStaleTime` and `instant`; Next.js fails the build"
                        ),
                        record.span,
                    ));
                }
            }
            for (message, span) in findings {
                self.diagnostics.push(
                    Diagnostic::error("invalid_route_segment_config", message)
                        .in_file(path)
                        .at(span),
                );
            }
        }
    }

    /// Report Pages Router pages whose data-fetching exports or runtime
    /// configuration Next.js rejects at build time.
    ///
    /// Verified against Next.js 16.4.6 with real `next build` probes. The
    /// Pages Router imports each page and throws for:
    /// * `getStaticProps` together with `getServerSideProps`
    /// * `getInitialProps` on the default component together with either
    /// * `getStaticPaths` together with `getServerSideProps`
    /// * `getStaticPaths` without `getStaticProps` on a prerendered page
    /// * `getStaticPaths` on a non-dynamic page with `getStaticProps`
    /// * `getStaticProps` on a dynamic page without `getStaticPaths`
    /// * `runtime: 'edge'` on a non-API page, where the render runtime is
    ///   still experimental; `experimental-edge` is the accepted spelling
    /// * a runtime value outside `nodejs`, `edge`, and `experimental-edge`
    ///
    /// API routes and `_app`/`_document`/`_error` skip the data-function
    /// checks, as they do in Next.js. Runtime config is still validated there,
    /// except that API routes accept `edge`.
    pub(super) fn detect_next_pages_router_exports(&mut self) {
        let summaries = self.summaries.clone();
        let page_extensions = self.configured_page_extensions();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or_else(String::new, |package| package.directory.clone());
            if super::entries::package_declares_next(self.resolver.root(), &package_directory)
                != Some(true)
            {
                continue;
            }
            let Some(page) = classify_pages_router_page(
                &summary.path,
                &package_directory,
                page_extensions.get(&package_directory),
            ) else {
                continue;
            };
            let path = &summary.path;
            // Next.js prefers the top-level `runtime` export and falls back to
            // `config.runtime`. Validate whichever value it would resolve.
            let runtime = summary
                .exported_const_values
                .iter()
                .find(|record| record.name == "runtime")
                .or(summary.pages_config_runtime.as_ref());
            if let Some(record) = runtime {
                let message = match &record.value {
                    ExportedConstValue::String(value)
                        if matches!(value.as_str(), "nodejs" | "experimental-edge") =>
                    {
                        None
                    }
                    ExportedConstValue::String(value) if value == "edge" && page.api => None,
                    ExportedConstValue::String(value) if value == "edge" => Some(format!(
                        "`{path}` sets `runtime: 'edge'` on a Pages Router page; Next.js fails the build because the edge render runtime is experimental. Use `'experimental-edge'` instead"
                    )),
                    ExportedConstValue::String(value) => Some(format!(
                        "`{path}` sets `runtime: '{value}'`; Next.js fails the build because only `nodejs`, `edge`, and `experimental-edge` are accepted"
                    )),
                    _ => Some(format!(
                        "`{path}` sets a non-string Pages Router `runtime`; Next.js fails the build because only `nodejs`, `edge`, and `experimental-edge` are accepted"
                    )),
                };
                if let Some(message) = message {
                    self.diagnostics.push(
                        Diagnostic::error("invalid_pages_router_export", message)
                            .in_file(path)
                            .at(record.span),
                    );
                }
            }
            if page.api || page.reserved {
                continue;
            }
            let get_static_props = pages_router_export_span(summary, "getStaticProps");
            let get_server_side_props = pages_router_export_span(summary, "getServerSideProps");
            let get_static_paths = pages_router_export_span(summary, "getStaticPaths");
            let get_initial_props = summary.default_export_has_get_initial_props;
            let mut findings: Vec<(String, Span)> = Vec::new();
            if let (Some(_), Some(server_props)) = (get_static_props, get_server_side_props) {
                findings.push((
                    format!(
                        "`{path}` exports both `getStaticProps` and `getServerSideProps`; Next.js fails the build because a page cannot combine SSG and SSR"
                    ),
                    server_props,
                ));
            }
            if get_initial_props {
                if let Some(span) = get_static_props {
                    findings.push((
                        format!(
                            "`{path}` sets `getInitialProps` on the default component and exports `getStaticProps`; Next.js fails the build because the two data-fetching APIs conflict"
                        ),
                        span,
                    ));
                }
                if let Some(span) = get_server_side_props {
                    findings.push((
                        format!(
                            "`{path}` sets `getInitialProps` on the default component and exports `getServerSideProps`; Next.js fails the build because the two data-fetching APIs conflict"
                        ),
                        span,
                    ));
                }
            }
            if let Some(paths_span) = get_static_paths {
                if let Some(server_props) = get_server_side_props {
                    findings.push((
                        format!(
                            "`{path}` exports `getStaticPaths` with `getServerSideProps`; Next.js fails the build because `getStaticPaths` only applies to static generation"
                        ),
                        server_props,
                    ));
                }
                if get_static_props.is_none()
                    && !get_initial_props
                    && get_server_side_props.is_none()
                {
                    findings.push((
                        format!(
                            "`{path}` exports `getStaticPaths` without `getStaticProps`; Next.js fails the build because the paths do nothing"
                        ),
                        paths_span,
                    ));
                } else if get_static_props.is_some() && !page.dynamic {
                    findings.push((
                        format!(
                            "`{path}` exports `getStaticPaths` on a non-dynamic route; Next.js fails the build because static paths require a `[param]` segment"
                        ),
                        paths_span,
                    ));
                }
            }
            if let Some(span) = get_static_props {
                if page.dynamic && get_static_paths.is_none() {
                    findings.push((
                        format!(
                            "`{path}` is a dynamic route with `getStaticProps` but no `getStaticPaths`; Next.js fails the build because it cannot enumerate the paths"
                        ),
                        span,
                    ));
                }
            }
            for (message, span) in findings {
                self.diagnostics.push(
                    Diagnostic::error("invalid_pages_router_export", message)
                        .in_file(path)
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
