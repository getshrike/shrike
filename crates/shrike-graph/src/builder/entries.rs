use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use shrike_core::ids::module_symbol_key;
use shrike_core::{ClientBoundaryPolicy, EntryPolicy};
use shrike_syntax::ParseStatus;

use super::Builder;
use crate::types::EntryRecord;
use crate::util::route_of;

/// Relative module paths named in a `next.config.*` file. Next.js loads these
/// directly (custom image loader, cache handlers, Turbopack loaders/aliases),
/// so they are entries even though no application module imports them. The
/// scan is deliberately lexer-light but skips comments: any relative string is
/// treated as a possible config reference, which fails closed.
fn next_config_references(source: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '/' if chars.peek() == Some(&'/') => {
                for character in chars.by_ref() {
                    if character == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for character in chars.by_ref() {
                    if previous == '*' && character == '/' {
                        break;
                    }
                    previous = character;
                }
            }
            '\'' | '"' | '`' => {
                let quote = character;
                let mut value = String::new();
                let mut interpolation = false;
                while let Some(character) = chars.next() {
                    if character == '\\' {
                        if let Some(escaped) = chars.next() {
                            value.push(escaped);
                        }
                    } else if character == quote {
                        break;
                    } else {
                        if quote == '`' && character == '$' && chars.peek() == Some(&'{') {
                            interpolation = true;
                        }
                        value.push(character);
                    }
                }
                if !interpolation && (value.starts_with("./") || value.starts_with("../")) {
                    references.push(value);
                }
            }
            _ => {}
        }
    }
    references
}

/// Vercel functions live under a project's `api/` directory when that project
/// opts in with a sibling `vercel.json`. They are HTTP endpoints executed by
/// the platform, not imported by application code.
fn is_vercel_function(root: &Path, path: &str) -> bool {
    let mut search = 0;
    while let Some(offset) = path[search..].find("api/") {
        let index = search + offset;
        let boundary = index == 0 || path.as_bytes()[index - 1] == b'/';
        if boundary && root.join(&path[..index]).join("vercel.json").is_file() {
            return true;
        }
        search = index + 4;
    }
    false
}

/// Whether `path` is inside `directory`, either at the repository root or in a
/// nested workspace package. Framework conventions are package-relative.
fn in_directory(path: &str, directory: &str) -> bool {
    path.starts_with(directory) || path.contains(&format!("/{directory}"))
}

/// Whether `path` is exactly `relative` at the repository root or at the root
/// of a nested workspace package.
fn at_package_root(path: &str, relative: &str) -> bool {
    path == relative || path.ends_with(&format!("/{relative}"))
}

/// Framework files that are executed by convention even when the TypeScript
/// parser does not understand their extension. These files are never deletion
/// candidates, but their module symbol is an entry.
pub(super) fn framework_module(path: &str) -> Option<(&'static str, &'static str)> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".svelte") && in_directory(&lower, "src/routes/") {
        let name = lower.rsplit('/').next().unwrap_or("");
        if name.starts_with("+page") || name.starts_with("+layout") || name.starts_with("+error") {
            return Some(("sveltekit", "route"));
        }
    }
    if lower.ends_with(".astro") && in_directory(&lower, "src/pages/") {
        return Some(("astro", "page"));
    }
    if lower.ends_with(".vue") {
        // Nuxt 3 keeps routes at the project root; Nuxt 4 and srcDir setups
        // move them under `app/` or `src/`. Any of those locations is a route
        // component, never a deletable module.
        let name = lower.rsplit('/').next().unwrap_or("");
        if matches!(name, "app.vue" | "error.vue")
            || in_directory(&lower, "pages/")
            || in_directory(&lower, "layouts/")
        {
            return Some(("nuxt", "route"));
        }
    }
    None
}

impl<'a> Builder<'a> {
    pub(super) fn detect_entries(&mut self) {
        let summaries = self.summaries.clone();
        let mut expo_layout_packages: BTreeSet<String> = BTreeSet::new();
        let mut nuxt_config_packages: BTreeSet<String> = BTreeSet::new();
        let mut router_packages: BTreeSet<String> = BTreeSet::new();
        for summary in &summaries {
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or("", |package| package.directory.as_str());
            let relative = summary
                .path
                .strip_prefix(package_directory)
                .unwrap_or(&summary.path)
                .trim_start_matches('/');
            if matches!(
                relative,
                "app/_layout.tsx"
                    | "app/_layout.ts"
                    | "app/_layout.jsx"
                    | "app/_layout.js"
                    | "src/app/_layout.tsx"
                    | "src/app/_layout.ts"
                    | "src/app/_layout.jsx"
                    | "src/app/_layout.js"
            ) {
                expo_layout_packages.insert(package_directory.to_string());
            }
            if matches!(
                relative,
                "nuxt.config.ts"
                    | "nuxt.config.js"
                    | "nuxt.config.mjs"
                    | "nuxt.config.mts"
                    | "nuxt.config.cts"
            ) {
                nuxt_config_packages.insert(package_directory.to_string());
            }
            // React Router framework mode maps route modules from `app/routes.ts`
            // instead of importing them. A route module may import only its
            // generated `./+types/*` types, so the package-level config is the
            // reliable signal that every file under `app/routes/` is a route.
            // Remix has no route map, but its root module always imports the
            // framework; use that to protect routes that have no imports.
            let router_config = matches!(
                relative,
                "app/routes.ts"
                    | "app/routes.js"
                    | "app/routes.mts"
                    | "app/routes.cts"
                    | "app/routes.mjs"
                    | "app/routes.cjs"
                    | "react-router.config.ts"
                    | "react-router.config.js"
                    | "react-router.config.mts"
                    | "react-router.config.cts"
                    | "react-router.config.mjs"
                    | "react-router.config.cjs"
            );
            let router_root = matches!(
                relative,
                "app/root.tsx" | "app/root.ts" | "app/root.jsx" | "app/root.js"
            );
            let imports_router = summary.imports.iter().any(|import| {
                import.source == "react-router"
                    || import.source.starts_with("react-router/")
                    || import.source.starts_with("@react-router/")
            });
            let imports_remix = summary
                .imports
                .iter()
                .any(|import| import.source.starts_with("@remix-run/"));
            if (router_config && imports_router)
                || (router_root && (imports_router || imports_remix))
            {
                router_packages.insert(package_directory.to_string());
            }
        }
        // Next.js loads configured modules directly (custom image loader,
        // cache handlers, Turbopack loaders/aliases). They are roots even
        // though no application module imports them.
        let analyzed: BTreeSet<&str> = summaries
            .iter()
            .map(|summary| summary.path.as_str())
            .collect();
        let mut next_config_modules: BTreeSet<String> = BTreeSet::new();
        for summary in &summaries {
            let file_name = summary.path.rsplit('/').next().unwrap_or("");
            if !matches!(
                file_name,
                "next.config.js"
                    | "next.config.mjs"
                    | "next.config.cjs"
                    | "next.config.ts"
                    | "next.config.mts"
                    | "next.config.cts"
            ) {
                continue;
            }
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                continue;
            };
            for specifier in next_config_references(&source) {
                let resolved = self.resolver.resolve(&summary.path, &specifier);
                if let Some(target) = resolved.target {
                    if analyzed.contains(target.as_str()) {
                        next_config_modules.insert(target);
                    }
                }
            }
        }
        for summary in &summaries {
            let path = summary.path.as_str();
            let package_directory = self
                .resolver
                .package_for(path)
                .map_or("", |package| package.directory.as_str());
            let has_expo_layout = expo_layout_packages.contains(package_directory);
            let has_nuxt_config = nuxt_config_packages.contains(package_directory);
            let has_router_package = router_packages.contains(package_directory);
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
            if summary.parse_status != ParseStatus::Ok {
                if let Some((framework, kind)) = framework_module(path) {
                    add(self, module, framework, kind, route_of(path), "both");
                }
                continue;
            }
            if summary.executable {
                add(self, module.clone(), "executable", "tool", None, "both");
            }
            if next_config_modules.contains(path) {
                // Everything exported by a config-referenced module is
                // reachable through the configuration (loaders may expose
                // helper exports such as Turbopack's `raw`). Protect the whole
                // surface rather than guessing which hook the tool calls.
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(
                        self,
                        key,
                        "next-config",
                        "config-reference-export",
                        None,
                        "both",
                    );
                }
                add(
                    self,
                    module.clone(),
                    "next-config",
                    "config-reference",
                    None,
                    "both",
                );
            }
            let file_name = path.rsplit('/').next().unwrap_or(path);
            let stem = file_name
                .rsplit_once('.')
                .map_or(file_name, |(stem, _)| stem);
            let route = route_of(path);
            let is_app = path.starts_with("app/") || path.contains("/app/");
            let is_pages = path.starts_with("pages/") || path.contains("/pages/");
            // Next.js middleware/proxy and the MDX component map live at the
            // project root (or `src/`), not under `app/`, but they are still
            // framework entrypoints.
            let root_convention = matches!(
                stem,
                "middleware" | "proxy" | "mdx-components" | "instrumentation-client"
            ) && path
                .strip_suffix(".tsx")
                .or_else(|| path.strip_suffix(".ts"))
                .or_else(|| path.strip_suffix(".jsx"))
                .or_else(|| path.strip_suffix(".js"))
                .is_some_and(|base| {
                    [
                        "middleware",
                        "proxy",
                        "mdx-components",
                        "instrumentation-client",
                        "src/middleware",
                        "src/proxy",
                        "src/mdx-components",
                        "src/instrumentation-client",
                    ]
                    .iter()
                    .any(|relative| at_package_root(base, relative))
                });
            let app_entry = (is_app
                && matches!(
                    stem,
                    "page"
                        | "layout"
                        | "template"
                        | "loading"
                        | "error"
                        | "global-error"
                        | "not-found"
                        | "global-not-found"
                        | "forbidden"
                        | "unauthorized"
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
                ))
                || root_convention;
            let pages_entry = is_pages;
            if app_entry || pages_entry {
                let mut public_names: Vec<&str> = vec![
                    "default",
                    "page",
                    "layout",
                    "route",
                    "metadata",
                    "Head",
                    "pageQuery",
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
                    "onRequestError",
                    "onRequestInit",
                    "onRouterTransitionStart",
                    "proxy",
                    "middleware",
                    "config",
                    "dynamic",
                    "revalidate",
                    "runtime",
                    "fetchCache",
                    "dynamicParams",
                    "preferredRegion",
                    "maxDuration",
                    // Cache Components segment config. `instant` and `prefetch`
                    // replaced their `unstable_` spellings in Next.js 16.3;
                    // keep the older names for projects on 16.0-16.2.
                    "ensureStatic",
                    "instant",
                    "unstable_instant",
                    "prefetch",
                    "unstable_prefetch",
                    "experimental_ppr",
                    "GET",
                    "POST",
                    "PUT",
                    "PATCH",
                    "DELETE",
                    "HEAD",
                    "OPTIONS",
                ];
                // Metadata image routes export a small, fixed API. Keep it scoped
                // to those files so a `size` export on a page is still analyzed.
                if matches!(
                    stem,
                    "icon" | "apple-icon" | "opengraph-image" | "twitter-image"
                ) {
                    public_names.extend(["size", "contentType", "alt", "generateImageMetadata"]);
                }
                if stem == "mdx-components" {
                    public_names.push("useMDXComponents");
                }
                // Next.js calls `reportWebVitals` when it is exported from the
                // Pages Router `_app` file, but it is not imported by code.
                if stem == "_app" {
                    public_names.push("reportWebVitals");
                }
                for name in public_names {
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
                    route.clone(),
                    "both",
                );
            }
            // Remix and React Router route modules are called by the router,
            // not imported by application code.
            let router_import = summary.imports.iter().any(|import| {
                import.source.starts_with("@remix-run/")
                    || import.source == "react-router"
                    || import.source.starts_with("react-router/")
                    || import.source.starts_with("@react-router/")
            });
            let router_entry = [
                "app/root.tsx",
                "app/root.ts",
                "app/root.jsx",
                "app/root.js",
                "app/entry.client.tsx",
                "app/entry.client.ts",
                "app/entry.client.jsx",
                "app/entry.client.js",
                "app/entry.server.tsx",
                "app/entry.server.ts",
                "app/entry.server.jsx",
                "app/entry.server.js",
                "app/routes.ts",
                "app/routes.js",
            ]
            .iter()
            .any(|relative| at_package_root(path, relative));
            let router_root = ["app/root.tsx", "app/root.ts", "app/root.jsx", "app/root.js"]
                .iter()
                .any(|relative| at_package_root(path, relative));
            // In framework mode every `app/routes/` file is a route module,
            // including files named `page.tsx`. The per-file import check
            // remains for standalone/Remix layouts without a routes config.
            let router_route = in_directory(path, "app/routes/")
                && (has_router_package || (router_import && stem != "page"));
            if (router_import || has_router_package) && (router_entry || router_route) {
                add(
                    self,
                    module.clone(),
                    "remix",
                    if router_entry { "entry" } else { "route" },
                    route.clone(),
                    "both",
                );
                for name in [
                    "default",
                    "loader",
                    "clientLoader",
                    "action",
                    "clientAction",
                    "middleware",
                    "clientMiddleware",
                    "meta",
                    "links",
                    "headers",
                    "ErrorBoundary",
                    "HydrateFallback",
                    "shouldRevalidate",
                    "handle",
                    "Component",
                    "config",
                ] {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(self, key, "remix", "route-export", route.clone(), "both");
                    }
                }
                // The root route alone may export `Layout`, the document shell
                // React Router wraps around the default component.
                if router_root {
                    if let Some(key) = self.resolve_export(path, "Layout", &mut BTreeSet::new()) {
                        add(self, key, "remix", "route-export", route.clone(), "both");
                    }
                }
            }

            // TanStack Router file routes export a `Route` object consumed by
            // the generated route tree. The tree can point at a configured
            // `routesDirectory`, so identify route modules by their route
            // factory import rather than a fixed folder name.
            let tanstack_import = summary
                .imports
                .iter()
                .any(|import| import.source.starts_with("@tanstack/"));
            let tanstack_route_factory = summary.imports.iter().any(|import| {
                import.bindings.iter().any(|binding| {
                    matches!(
                        binding.imported.as_str(),
                        "createFileRoute"
                            | "createLazyFileRoute"
                            | "createRootRoute"
                            | "createServerFileRoute"
                    )
                })
            });
            let tanstack_route =
                tanstack_route_factory || (tanstack_import && in_directory(path, "src/routes/"));
            if tanstack_route {
                add(
                    self,
                    module.clone(),
                    "tanstack",
                    "route",
                    route.clone(),
                    "both",
                );
                for name in ["Route", "route"] {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(self, key, "tanstack", "route-export", route.clone(), "both");
                    }
                }
            }

            // Expo Router uses `app/` with `_layout` files and `+` special
            // routes. A layout is the strong signal that the tree is Expo.
            let expo_import = summary
                .imports
                .iter()
                .any(|import| import.source.starts_with("expo-router"));
            // Expo Router treats every module under `app/` as a route. The
            // `_layout` file is the strong signal that the tree is Expo rather
            // than Next; a direct `expo-router` import is the weaker one.
            let expo_app = in_directory(path, "app/") || in_directory(path, "src/app/");
            if expo_app && !app_entry && (has_expo_layout || expo_import) {
                add(self, module.clone(), "expo", "route", route.clone(), "both");
                let mut public_names = vec![
                    "default",
                    "unstable_settings",
                    "ErrorBoundary",
                    "getDerivedStateFromError",
                ];
                // `+api` routes are dispatched by HTTP method. Expo Router
                // reads these named handlers directly from the route module.
                if stem.ends_with("+api") {
                    public_names
                        .extend(["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]);
                }
                // `+native-intent` is consulted before native deep links are
                // resolved. `legacy_subscribe` is the alpha React Navigation
                // bridge; both are framework-invoked exports.
                if stem == "+native-intent" {
                    public_names.extend(["redirectSystemPath", "legacy_subscribe"]);
                }
                for name in public_names {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(self, key, "expo", "route-export", route.clone(), "both");
                    }
                }
            }

            // SvelteKit route modules export `load`, actions, and page options
            // that the framework calls directly.
            let sveltekit_route = in_directory(path, "src/routes/")
                && (stem.starts_with("+page")
                    || stem.starts_with("+layout")
                    || stem.starts_with("+server")
                    || stem.starts_with("+error"));
            if sveltekit_route {
                add(
                    self,
                    module.clone(),
                    "sveltekit",
                    "route",
                    route.clone(),
                    "both",
                );
                for name in [
                    "load",
                    "actions",
                    "entries",
                    "prerender",
                    "ssr",
                    "csr",
                    "trailingSlash",
                    "config",
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
                            "sveltekit",
                            "route-export",
                            route.clone(),
                            "both",
                        );
                    }
                }
            }

            // Astro endpoints under `src/pages` are HTTP handlers.
            let astro_endpoint = in_directory(path, "src/pages/")
                && summary
                    .imports
                    .iter()
                    .any(|import| import.source == "astro" || import.source.starts_with("astro:"));
            if astro_endpoint {
                add(
                    self,
                    module.clone(),
                    "astro",
                    "endpoint",
                    route.clone(),
                    "server",
                );
                for name in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(
                            self,
                            key,
                            "astro",
                            "endpoint-export",
                            route.clone(),
                            "server",
                        );
                    }
                }
            }

            // Nuxt/Nitro server routes and plugins are loaded by convention.
            let nuxt_entry = has_nuxt_config
                && (in_directory(path, "server/api/")
                    || in_directory(path, "server/routes/")
                    || in_directory(path, "server/middleware/")
                    || in_directory(path, "plugins/")
                    || in_directory(path, "middleware/"));
            if nuxt_entry {
                add(
                    self,
                    module.clone(),
                    "nuxt",
                    "server",
                    route.clone(),
                    "server",
                );
                for name in ["default", "handler"] {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(self, key, "nuxt", "server-export", route.clone(), "server");
                    }
                }
            }

            // Ambient declarations and declaration files describe types that
            // are visible program-wide (or are published API), not values that
            // can be proven unused from the module graph. Never propose them.
            let declaration_file = matches!(
                path.rsplit_once('.'),
                Some((_, "ts" | "mts" | "cts")) if path.contains(".d.")
            );
            for declaration in &summary.declarations {
                let ambient = summary.ambient_modules.contains(&declaration.name);
                if !ambient && !declaration_file {
                    continue;
                }
                let key = self
                    .locals
                    .get(path)
                    .and_then(|locals| locals.get(&declaration.name))
                    .cloned();
                if let Some(key) = key {
                    add(self, key, "ambient", "declaration", None, "both");
                }
            }
            if summary.has_directive("use client")
                && self.config.client_boundary_policy == ClientBoundaryPolicy::Protect
            {
                for (name, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    let _ = name;
                    add(self, key, "react", "client-boundary", None, "both");
                }
                add(self, module.clone(), "react", "module", None, "both");
            }
            if summary.has_directive("use server") {
                if self.config.client_boundary_policy == ClientBoundaryPolicy::Protect {
                    for (name, key) in self.exports.get(path).cloned().unwrap_or_default() {
                        let _ = name;
                        add(self, key, "react", "client-boundary", None, "both");
                    }
                    add(self, module.clone(), "react", "module", None, "both");
                }
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
                || path.contains(".test-d.")
                || path.contains("__tests__/")
                || path.contains("__typetest__/")
                || path.contains("__mocks__/")
                || path.contains("/__fixtures__/")
                || path.contains("/fixtures/")
                || path.contains("/e2e/")
                || path.starts_with("test/")
                || path.starts_with("tests/")
                || path.contains("/test/")
                || path.contains("/tests/");
            let story = path.contains(".stories.")
                || path.contains(".story.")
                || path.starts_with(".storybook/")
                || path.contains("/.storybook/");
            if self.config.test_entries
                && self.config.entry_policy != EntryPolicy::Production
                && (test || story)
            {
                add(self, module.clone(), "tests", "test", None, "both");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "tests", "test-export", None, "both");
                }
            }
            // Standalone examples, benchmarks, and repository scripts are run
            // directly rather than imported. Under the local policy they are
            // roots, not dead code.
            let local_root = path.starts_with("examples/")
                || path.starts_with("example/")
                || path.starts_with("demo/")
                || path.starts_with("playground/")
                || path.starts_with("playgrounds/")
                || path.starts_with("benchmarks/")
                || path.starts_with("scripts/");
            if self.config.entry_policy != EntryPolicy::Production && local_root {
                add(self, module.clone(), "local", "standalone", None, "both");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "local", "standalone-export", None, "both");
                }
            }
            // Gatsby loads lifecycle files by convention and pages under
            // `src/pages`; both are executed directly by the framework.
            let gatsby_lifecycle = matches!(
                stem,
                "gatsby-config" | "gatsby-node" | "gatsby-browser" | "gatsby-ssr" | "gatsby-shared"
            );
            if self.config.entry_policy != EntryPolicy::Production && gatsby_lifecycle {
                add(self, module.clone(), "gatsby", "lifecycle", None, "both");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "gatsby", "lifecycle-export", None, "both");
                }
            }
            // Gatsby functions under `src/api` are HTTP endpoints executed by
            // the framework, not imported by application code.
            let gatsby_function = path.starts_with("src/api/") || path.contains("/src/api/");
            if self.config.entry_policy != EntryPolicy::Production && gatsby_function {
                add(self, module.clone(), "gatsby", "function", None, "server");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "gatsby", "function-export", None, "server");
                }
            }
            if self.config.entry_policy != EntryPolicy::Production
                && is_vercel_function(self.resolver.root(), path)
            {
                add(self, module.clone(), "vercel", "function", None, "server");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "vercel", "function-export", None, "server");
                }
            }
            // Build/test tool configs are consumed directly by the tool, so
            // both the module and everything it exports are entries. Matching
            // on the `.config` suffix covers rollup, drizzle, shrike, and any
            // future tool without maintaining a name list.
            let config_file =
                stem.ends_with(".config") || matches!(stem, "tsconfig" | "instrumentation");
            if config_file {
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "config", "config-export", None, "both");
                }
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
            let package_flags = self.resolver.package_for(path).map(|package| {
                (
                    package.script_files.contains(path),
                    package.public_sources.contains(path) || package.entry.as_deref() == Some(path),
                )
            });
            if let Some((is_script, is_public_source)) = package_flags {
                if is_script {
                    add(self, module.clone(), "scripts", "script", None, "both");
                }
                if is_public_source {
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

    fn entry_symbols(snapshot: &GraphSnapshot) -> Vec<&str> {
        snapshot
            .entries
            .iter()
            .map(|entry| entry.symbol.as_str())
            .collect()
    }

    #[test]
    fn symbol_line_counts_use_the_declaration_span() {
        let snapshot = snapshot(&[(
            "source/math.ts",
            "export const add = (a: number, b: number) => {\n  return a + b;\n};\n\nexport const one = 1;\n",
        )]);
        let lines = |name: &str| {
            snapshot
                .symbols
                .iter()
                .find(|symbol| symbol.key == format!("source/math.ts#{name}"))
                .expect("symbol")
                .lines
        };
        assert_eq!(lines("add"), 3);
        assert_eq!(lines("one"), 1);
    }

    #[test]
    fn plain_test_directories_are_entries() {
        let snapshot = snapshot(&[
            ("source/index.ts", "export const api = 1;\n"),
            (
                "test/test.ts",
                "import { api } from '../source/index.ts';\nconsole.log(api);\n",
            ),
            ("tests/unit/helper.ts", "export const helper = 1;\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(entries.contains(&"test/test.ts#__module__"), "{entries:?}");
        assert!(
            entries.contains(&"tests/unit/helper.ts#__module__"),
            "{entries:?}"
        );
    }

    #[test]
    fn config_file_exports_are_entries() {
        let snapshot = snapshot(&[
            ("src/index.ts", "export const live = 1;\n"),
            (
                "rollup.config.mjs",
                "export const external = ['react'];\nexport default { external };\n",
            ),
            ("eslint.config.mjs", "export default [];\n"),
            (
                "shrike.config.ts",
                "const config = {};\nexport default config;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"rollup.config.mjs#external"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"rollup.config.mjs#default"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"eslint.config.mjs#default"),
            "{entries:?}"
        );
        assert!(entries.contains(&"shrike.config.ts#default"), "{entries:?}");
    }

    #[test]
    fn documented_tool_scripts_are_entries() {
        let snapshot = snapshot(&[
            (
                "scripts/generate-report.ts",
                "/**\n * Report generator.\n *\n * Usage: npx tsx scripts/generate-report.ts\n */\nfunction main() {\n  console.log('report');\n}\nmain();\n",
            ),
            (
                "src/library.ts",
                "/** Usage: this is documentation, not an entry. */\nexport const value = 1;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"scripts/generate-report.ts#__module__"),
            "{entries:?}"
        );
        assert!(
            snapshot
                .live
                .iter()
                .any(|key| key == "scripts/generate-report.ts#main"),
            "tool body must stay live: {:?}",
            snapshot.live
        );
        assert!(
            !entries.contains(&"src/library.ts#__module__"),
            "{entries:?}"
        );
    }

    #[test]
    fn root_next_proxy_is_an_entry() {
        let snapshot = snapshot(&[
            (
                "proxy.ts",
                "export const config = { matcher: ['/jobs/:slug'] };\nexport async function proxy() {}\n",
            ),
            ("src/index.ts", "export const live = 1;\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(entries.contains(&"proxy.ts#proxy"), "{entries:?}");
        assert!(entries.contains(&"proxy.ts#config"), "{entries:?}");
    }

    #[test]
    fn package_script_modules_are_entries() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","scripts":{"seed":"TZ=UTC npx tsx db/seeds/seed.ts"}}"#,
            ),
            (
                "db/seeds/seed.ts",
                "function main() {\n  console.log('seed');\n}\nmain();\n",
            ),
            ("src/index.ts", "export const live = 1;\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"db/seeds/seed.ts#__module__"),
            "{entries:?}"
        );
        assert!(
            snapshot
                .live
                .iter()
                .any(|key| key == "db/seeds/seed.ts#main"),
            "the script body must stay live: {:?}",
            snapshot.live
        );
    }

    #[test]
    fn ambient_module_augmentations_and_declaration_files_are_entries() {
        let snapshot = snapshot(&[
            (
                "src/vanilla.ts",
                "export interface StoreMutators<S, A> {}\nexport type StateCreator<T> = T;\n",
            ),
            (
                "src/immer.ts",
                "import type { StateCreator } from './vanilla.ts';\n\
                 type Write<T, U> = Omit<T, keyof U> & U;\n\
                 declare module './vanilla.ts' {\n\
                   interface StoreMutators<S, A> { immer: Write<S, A> }\n\
                 }\n\
                 export const immer = 1;\n",
            ),
            (
                "src/types.d.ts",
                "declare interface ImportMeta { env?: { MODE: string } }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"src/immer.ts#./vanilla.ts"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"src/types.d.ts#ImportMeta"),
            "{entries:?}"
        );
        assert!(
            snapshot.live.iter().any(|key| key == "src/immer.ts#Write"),
            "types referenced by a module augmentation must stay live: {:?}",
            snapshot.live
        );
    }

    #[test]
    fn library_build_entry_maps_to_source_exports() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"@corpus/is","exports":{"types":"./distribution/index.d.ts","default":"./distribution/index.js"}}"#,
            ),
            (
                "source/index.ts",
                "export function isString(value: unknown) { return typeof value === 'string'; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(entries.contains(&"source/index.ts#isString"), "{entries:?}");
        assert!(
            entries.contains(&"source/index.ts#__module__"),
            "{entries:?}"
        );
    }

    #[test]
    fn next_interrupt_routes_and_image_metadata_are_entries() {
        let snapshot = snapshot(&[
            (
                "app/layout.tsx",
                "export default function Layout() { return null; }\n",
            ),
            (
                "app/page.tsx",
                "export default function Page() { return null; }\nexport const helper = 1;\n",
            ),
            (
                "app/forbidden.tsx",
                "export default function Forbidden() { return null; }\n",
            ),
            (
                "app/unauthorized.tsx",
                "export default function Unauthorized() { return null; }\n",
            ),
            (
                "app/icon.tsx",
                "export const size = { width: 32 };\nexport const contentType = 'image/png';\nexport default function Icon() { return null; }\n",
            ),
            (
                "app/opengraph-image.tsx",
                "export const size = { width: 1200 };\nexport const contentType = 'image/png';\nexport default function Image() { return null; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"app/forbidden.tsx#default"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"app/unauthorized.tsx#default"),
            "{entries:?}"
        );
        assert!(entries.contains(&"app/icon.tsx#size"), "{entries:?}");
        assert!(entries.contains(&"app/icon.tsx#contentType"), "{entries:?}");
        assert!(
            entries.contains(&"app/opengraph-image.tsx#size"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"app/opengraph-image.tsx#contentType"),
            "{entries:?}"
        );
        assert!(
            !entries.contains(&"app/page.tsx#helper"),
            "a page's arbitrary export must not become public: {entries:?}"
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "app/page.tsx#helper"),
            "the page helper should remain a candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn next_cache_components_segment_config_is_public() {
        let snapshot = snapshot(&[
            (
                "app/layout.tsx",
                "export const ensureStatic = 'navigation';\nexport const instant = true;\nexport const prefetch = 'partial';\nexport const unstable_instant = true;\nexport const unstable_prefetch = 'force-disabled';\nexport function unusedLayoutHelper() { return 1; }\nexport default function Layout() { return null; }\n",
            ),
            (
                "app/page.tsx",
                "export const instant = { level: 'warning' };\nexport function unusedPageHelper() { return 1; }\nexport default function Page() { return null; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for name in [
            "ensureStatic",
            "instant",
            "unstable_instant",
            "prefetch",
            "unstable_prefetch",
        ] {
            let key = format!("app/layout.tsx#{name}");
            assert!(
                entries.contains(&key.as_str()),
                "segment config {name} must be public: {entries:?}"
            );
        }
        assert!(
            entries.contains(&"app/page.tsx#instant"),
            "page segment config must be public: {entries:?}"
        );
        assert!(
            !entries.contains(&"app/layout.tsx#unusedLayoutHelper"),
            "an arbitrary layout export must not become public: {entries:?}"
        );
        assert!(
            !entries.contains(&"app/page.tsx#unusedPageHelper"),
            "an arbitrary page export must not become public: {entries:?}"
        );
        for key in [
            "app/layout.tsx#unusedLayoutHelper",
            "app/page.tsx#unusedPageHelper",
        ] {
            assert!(
                snapshot.candidates.iter().any(|candidate| candidate == key),
                "{key} should remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn expo_api_routes_and_native_intent_exports_are_entries() {
        let snapshot = snapshot(&[
            (
                "app/_layout.tsx",
                "export default function Layout() { return null; }\n",
            ),
            (
                "app/hello+api.ts",
                "export function GET() { return new Response('ok'); }\nexport function POST() { return new Response('ok'); }\nexport function unusedApiHelper() { return 1; }\n",
            ),
            (
                "app/+native-intent.tsx",
                "export function redirectSystemPath({ path }: { path: string }) { return path; }\nexport function legacy_subscribe() {}\nexport function unusedIntentHelper() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for name in ["GET", "POST"] {
            let key = format!("app/hello+api.ts#{name}");
            assert!(
                entries.contains(&key.as_str()),
                "Expo API method {name} must be public: {entries:?}"
            );
        }
        for name in ["redirectSystemPath", "legacy_subscribe"] {
            let key = format!("app/+native-intent.tsx#{name}");
            assert!(
                entries.contains(&key.as_str()),
                "Expo native intent export {name} must be public: {entries:?}"
            );
        }
        for key in [
            "app/hello+api.ts#unusedApiHelper",
            "app/+native-intent.tsx#unusedIntentHelper",
        ] {
            assert!(
                !entries.contains(&key),
                "an arbitrary Expo route export must not become public: {entries:?}"
            );
            assert!(
                snapshot.candidates.iter().any(|candidate| candidate == key),
                "{key} should remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn next_config_referenced_modules_are_entries() {
        let snapshot = snapshot(&[
            (
                "next.config.ts",
                "// './lib/commented.ts' is documentation, not a loaded module\nconst config = {\n  images: { loaderFile: './image-loader.ts' },\n  cacheHandler: './cache-handler.ts',\n  turbopack: { rules: { '*.svg': { loaders: ['./loaders/svg.ts'] } } },\n};\nexport default config;\n",
            ),
            (
                "image-loader.ts",
                "export default function imageLoader() { return ''; }\n",
            ),
            ("cache-handler.ts", "export default class CacheHandler {}\n"),
            (
                "loaders/svg.ts",
                "export default function svgLoader() {}\nexport const raw = true;\n",
            ),
            (
                "lib/commented.ts",
                "export function commentedReference() { return 1; }\n",
            ),
            ("lib/dead.ts", "export function deadThing() { return 1; }\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "image-loader.ts#default",
            "cache-handler.ts#default",
            "loaders/svg.ts#default",
            "loaders/svg.ts#raw",
        ] {
            assert!(
                entries.contains(&key),
                "next.config reference {key} must be public: {entries:?}"
            );
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "next.config reference {key} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "lib/commented.ts#commentedReference",
            "lib/dead.ts#deadThing",
        ] {
            assert!(
                snapshot.candidates.iter().any(|candidate| candidate == key),
                "{key} must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn jsx_namespace_members_keep_the_used_export_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import * as UI from '../components/ui';\nexport default function Page() { return <UI.Button />; }\n",
            ),
            (
                "components/ui.tsx",
                "export function Button() { return null; }\nexport function DeadButton() { return null; }\n",
            ),
        ]);
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "components/ui.tsx#Button"),
            "the JSX namespace member must stay live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "components/ui.tsx#DeadButton"),
            "an unused namespace export must remain a candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn namespace_values_widen_the_target_module() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import * as UI from '../components/ui';\nconst all = { ...UI };\nexport default function Page() { return <div>{Object.keys(all).length}</div>; }\n",
            ),
            (
                "components/ui.tsx",
                "export function Button() { return null; }\nexport function DeadButton() { return null; }\n",
            ),
        ]);
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "components/ui.tsx#DeadButton"),
            "a namespace value use must conservatively protect every export: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn namespace_widening_propagates_through_exported_dependencies() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import * as UI from '../components/ui';\nconst all = { ...UI };\nexport default function Page() { return <div>{Object.keys(all).length}</div>; }\n",
            ),
            (
                "components/ui.tsx",
                "function leaf() { return 1; }\n\
                 function helper() { return leaf(); }\n\
                 export function Button() { return helper(); }\n\
                 export function DeadButton() { return null; }\n",
            ),
        ]);
        for key in ["components/ui.tsx#helper", "components/ui.tsx#leaf"] {
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "a live symbol reached through namespace widening must keep its own \
                 dependencies live: {key} in {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn type_position_imports_keep_the_named_type_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { JsDocValue } from '../src/jsdoc.js';\nexport const inline: import('../src/types').InlineType = { id: 'x' };\nexport default function Page() { return <div>{JsDocValue.name}</div>; }\n",
            ),
            (
                "src/jsdoc.js",
                "/** @type {import('./types').JSDocType} */\nexport const JsDocValue = { name: 'x' };\n",
            ),
            (
                "src/types.ts",
                "export interface JSDocType { name: string }\nexport interface InlineType { id: string }\nexport interface DeadType { dead: true }\n",
            ),
        ]);
        for live in ["src/types.ts#JSDocType", "src/types.ts#InlineType"] {
            assert!(
                !snapshot.candidates.iter().any(|key| key == live),
                "{live} is used from a type position: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/types.ts#DeadType"),
            "an unused type must remain a candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn destructured_dynamic_imports_keep_only_named_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const { used } = await import('./mod');\n  return used;\n}\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
        ]);
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#used"),
            "the destructured export must stay live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#dead"),
            "an export the destructuring does not read must be a candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn member_dynamic_imports_keep_only_the_accessed_export_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const used = (await import('./mod')).used;\n  return used;\n}\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
        ]);
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#used"),
            "{:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#dead"),
            "{:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn unknown_dynamic_import_usage_keeps_all_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const mod = await import('./mod');\n  return mod.used;\n}\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const other = 2;\n",
            ),
        ]);
        for key in ["src/mod.ts#used", "src/mod.ts#other"] {
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "a namespace binding can be used in unknown ways: {key} {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn unresolved_dynamic_import_name_keeps_all_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const { missing } = await import('./mod');\n  return missing;\n}\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const other = 2;\n",
            ),
        ]);
        for key in ["src/mod.ts#used", "src/mod.ts#other"] {
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "an unresolved destructured name must widen instead of narrowing: {key} {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn pages_app_report_web_vitals_is_a_framework_export() {
        let snapshot = snapshot(&[(
            "pages/_app.tsx",
            "export default function App() { return null; }\nexport function reportWebVitals() {}\nexport function unusedAppExport() {}\n",
        )]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"pages/_app.tsx#reportWebVitals"),
            "{entries:?}"
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "pages/_app.tsx#unusedAppExport"),
            "an arbitrary _app export must remain a candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn mdx_imports_and_component_map_are_reachable() {
        let snapshot = snapshot(&[
            (
                "app/layout.tsx",
                "export default function Layout() { return null; }\n",
            ),
            (
                "app/page.mdx",
                "import { Widget } from '../components/widget';\n\n# Hello\n\n<Widget />\n",
            ),
            (
                "components/widget.tsx",
                "export function Widget() { return null; }\n",
            ),
            (
                "mdx-components.tsx",
                "export function useMDXComponents(components: Record<string, unknown>) { return components; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"mdx-components.tsx#useMDXComponents"),
            "{entries:?}"
        );
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "components/widget.tsx#Widget"),
            "MDX imports must keep the component live: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "mdx-components.tsx#useMDXComponents"),
            "{:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn router_and_meta_framework_routes_are_entries() {
        let snapshot = snapshot(&[
            (
                "app/root.tsx",
                "import { Outlet } from '@remix-run/react';\nexport default function Root() { return <Outlet />; }\nexport function loader() { return null; }\n",
            ),
            (
                "app/routes/users.$id.tsx",
                "import { useLoaderData } from '@remix-run/react';\nexport async function loader() { return null; }\nexport default function User() { return null; }\n",
            ),
            (
                "src/routes/posts.tsx",
                "import { createFileRoute } from '@tanstack/react-router';\nexport const Route = createFileRoute('/posts')({});\n",
            ),
            (
                "app/_layout.tsx",
                "import { Stack } from 'expo-router';\nexport default function Layout() { return <Stack />; }\n",
            ),
            (
                "app/index.tsx",
                "export default function Index() { return null; }\n",
            ),
            (
                "app/profile.tsx",
                "export default function Profile() { return null; }\n",
            ),
            (
                "src/routes/+page.ts",
                "export function load() { return {}; }\n",
            ),
            (
                "src/pages/health.ts",
                "import type { APIRoute } from 'astro';\nexport const GET: APIRoute = () => new Response('ok');\n",
            ),
            ("nuxt.config.ts", "export default defineNuxtConfig({});\n"),
            (
                "server/api/hello.ts",
                "export default defineEventHandler(() => 'hello');\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(entries.contains(&"app/root.tsx#default"), "{entries:?}");
        assert!(
            entries.contains(&"app/routes/users.$id.tsx#loader"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"src/routes/posts.tsx#Route"),
            "{entries:?}"
        );
        assert!(entries.contains(&"app/_layout.tsx#default"), "{entries:?}");
        assert!(entries.contains(&"app/index.tsx#default"), "{entries:?}");
        assert!(
            entries.contains(&"app/profile.tsx#default"),
            "every Expo app module is a route: {entries:?}"
        );
        assert!(entries.contains(&"src/routes/+page.ts#load"), "{entries:?}");
        assert!(entries.contains(&"src/pages/health.ts#GET"), "{entries:?}");
        assert!(
            entries.contains(&"server/api/hello.ts#default"),
            "{entries:?}"
        );
    }

    #[test]
    fn skipped_framework_files_are_entries() {
        let snapshot = snapshot(&[
            ("src/routes/+page.svelte", "<h1>Hello</h1>\n"),
            ("src/pages/index.astro", "---\n---\n<h1>Hello</h1>\n"),
            ("pages/index.vue", "<template><div /></template>\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"src/routes/+page.svelte#__module__"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"src/pages/index.astro#__module__"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"pages/index.vue#__module__"),
            "{entries:?}"
        );
    }

    #[test]
    fn exported_symbols_used_only_in_their_own_file_are_reported_separately() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { bootstrap } from '../src/registry';\nexport default function Page() { bootstrap(); return null; }\n",
            ),
            (
                "src/registry.ts",
                "export function helper() { return 1; }\n\
                 export function publicApi() { return helper(); }\n\
                 export function bootstrap() { return { publicApi }; }\n",
            ),
        ]);
        assert!(
            snapshot
                .internal_only
                .iter()
                .any(|key| key == "src/registry.ts#publicApi"),
            "{:?}",
            snapshot.internal_only
        );
        assert!(
            snapshot
                .internal_only
                .iter()
                .any(|key| key == "src/registry.ts#helper"),
            "{:?}",
            snapshot.internal_only
        );
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "src/registry.ts#publicApi"),
            "{:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn nuxt_app_dir_and_expo_src_app_are_entries() {
        let snapshot = snapshot(&[
            ("nuxt.config.ts", "export default defineNuxtConfig({});\n"),
            ("app/pages/index.vue", "<template><div /></template>\n"),
            (
                "src/app/_layout.tsx",
                "import { Stack } from 'expo-router';\nexport default function Layout() { return <Stack />; }\n",
            ),
            (
                "src/app/index.tsx",
                "export default function Index() { return null; }\n",
            ),
            (
                "app/routes.ts",
                "import { type RouteConfig, index } from '@react-router/dev/routes';\nexport default [index('routes/home.tsx')] satisfies RouteConfig;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"app/pages/index.vue#__module__"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"src/app/_layout.tsx#default"),
            "{entries:?}"
        );
        assert!(
            entries.contains(&"src/app/index.tsx#default"),
            "{entries:?}"
        );
        assert!(entries.contains(&"app/routes.ts#default"), "{entries:?}");
    }

    #[test]
    fn react_router_route_modules_with_generated_types_are_entries() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"rr7-app","type":"module","dependencies":{"react-router":"^7.9.0","@react-router/dev":"^7.9.0"}}"#,
            ),
            (
                "app/routes.ts",
                "import { type RouteConfig, index, route } from '@react-router/dev/routes';\n\
                 export default [index('routes/home.tsx'), route('about', 'routes/about.tsx')] satisfies RouteConfig;\n",
            ),
            (
                "app/root.tsx",
                "import { Outlet } from 'react-router';\n\
                 export function Layout({ children }: { children: React.ReactNode }) { return <html><body>{children}</body></html>; }\n\
                 export default function Root() { return <Outlet />; }\n",
            ),
            (
                "app/routes/home.tsx",
                "import type { Route } from './+types/home';\n\
                 export function loader({}: Route.LoaderArgs) { return { title: 'Home' }; }\n\
                 export const middleware = [];\n\
                 export default function Home({ loaderData }: Route.ComponentProps) { return <main>{loaderData.title}</main>; }\n",
            ),
            (
                "app/routes/about.tsx",
                "import type { Route } from './+types/about';\n\
                 export async function action({ request }: Route.ActionArgs) { return request.formData(); }\n\
                 export const clientMiddleware = [];\n\
                 export default function About() { return <main>About</main>; }\n",
            ),
            (
                "app/components/dead.tsx",
                "export function deadThing() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for symbol in [
            "app/root.tsx#Layout",
            "app/routes/home.tsx#loader",
            "app/routes/home.tsx#middleware",
            "app/routes/home.tsx#default",
            "app/routes/about.tsx#action",
            "app/routes/about.tsx#clientMiddleware",
            "app/routes/about.tsx#default",
        ] {
            assert!(
                entries.contains(&symbol),
                "missing entry {symbol}: {entries:?}"
            );
        }
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "app/components/dead.tsx#deadThing"),
            "non-route modules must still be analyzed: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn remix_route_modules_without_imports_are_entries() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"remix-app","type":"module","dependencies":{"@remix-run/node":"^2.15.0","@remix-run/react":"^2.15.0"}}"#,
            ),
            (
                "app/root.tsx",
                "import { Outlet } from '@remix-run/react';\n\
                 export default function App() { return <Outlet />; }\n",
            ),
            (
                "app/routes/about._index.tsx",
                "export default function About() { return <main>About</main>; }\n",
            ),
            (
                "app/components/dead.tsx",
                "export function deadThing() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"app/routes/about._index.tsx#default"),
            "a Remix route with no imports is still router-rendered: {entries:?}"
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "app/components/dead.tsx#deadThing"),
            "non-route modules must still be analyzed: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn tanstack_route_factories_are_entries_in_custom_directories() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"tsr-app","type":"module","dependencies":{"@tanstack/react-router":"^1.130.0"}}"#,
            ),
            (
                "tsr.config.json",
                r#"{"routesDirectory":"src/pages","generatedRouteTree":"src/routeTree.gen.ts"}"#,
            ),
            (
                "src/pages/index.tsx",
                "import { createFileRoute } from '@tanstack/react-router';\n\
                 export const Route = createFileRoute('/')({ component: Home });\n\
                 function Home() { return <main>Home</main>; }\n",
            ),
            (
                "src/components/dead.tsx",
                "export function deadThing() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"src/pages/index.tsx#Route"),
            "a route factory marks a route in a configured directory: {entries:?}"
        );
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "src/pages/index.tsx#Home"),
            "the route's component must stay live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/components/dead.tsx#deadThing"),
            "non-route modules must still be analyzed: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn framework_component_imports_do_not_create_dangling_edges() {
        let snapshot = snapshot(&[
            ("src/main.ts", "import App from './App.vue';\nApp();\n"),
            ("src/App.vue", "<template><div /></template>\n"),
        ]);
        for edge in &snapshot.edges {
            assert!(
                snapshot
                    .symbols
                    .iter()
                    .any(|symbol| symbol.key == edge.source),
                "missing source symbol for edge: {edge:?}"
            );
            assert!(
                snapshot
                    .symbols
                    .iter()
                    .any(|symbol| symbol.key == edge.target),
                "missing target symbol for edge: {edge:?}"
            );
        }
    }

    #[test]
    fn internal_only_aliases_follow_external_use() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { publicName } from '../src/registry';\nexport default function Page() { publicName(); return null; }\n",
            ),
            (
                "src/registry.ts",
                "function helper() { return 1; }\nexport { helper as publicName };\nexport function unused() { return 2; }\n",
            ),
        ]);
        assert!(
            !snapshot
                .internal_only
                .iter()
                .any(|key| key == "src/registry.ts#helper"),
            "an exported alias makes the local symbol externally visible: {:?}",
            snapshot.internal_only
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/registry.ts#unused"),
            "{:?}",
            snapshot.candidates
        );
    }
}
