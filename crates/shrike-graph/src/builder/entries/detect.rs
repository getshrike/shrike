//! Per-file entry classification, split by framework.

use std::collections::BTreeSet;

use shrike_core::ids::module_symbol_key;
use shrike_core::{ClientBoundaryPolicy, EntryPolicy, ServerActionPolicy};
use shrike_syntax::ParseStatus;

use super::context::EntryContext;
use super::helpers::*;
use super::{Builder, add};
use crate::util::route_of;

pub(super) fn run(ctx: &EntryContext, builder: &mut Builder<'_>) {
    let summaries = &ctx.summaries;
    let analyzed: BTreeSet<&str> = summaries.iter().map(|s| s.path.as_str()).collect();
    let rsc_directives = ctx.rsc_directives;
    let page_extensions = &ctx.page_extensions;
    let vite_packages = &ctx.vite_packages;
    let storybook_packages = &ctx.storybook_packages;
    let gatsby_packages = &ctx.gatsby_packages;
    let expo_layout_packages = &ctx.expo_layout_packages;
    let expo_config_modules = &ctx.expo_config_modules;
    let nuxt_config_packages = &ctx.nuxt_config_packages;
    let next_config_modules = &ctx.next_config_modules;
    let webpack_entry_modules = &ctx.webpack_entry_modules;
    let storybook_story_files = &ctx.storybook_story_files;
    let ladle_story_files = &ctx.ladle_story_files;
    let ladle_entry_files = &ctx.ladle_entry_files;
    let cosmos_fixture_files = &ctx.cosmos_fixture_files;
    let cosmos_decorator_files = &ctx.cosmos_decorator_files;
    let docusaurus_page_files = &ctx.docusaurus_page_files;
    let gatsby_template_files = &ctx.gatsby_template_files;
    let router_route_files = &ctx.router_route_files;
    let test_config_modules = &ctx.test_config_modules;
    let cypress_specs = &ctx.cypress_specs;
    let cypress_support = &ctx.cypress_support;
    let expo_roots = &ctx.expo_roots;
    let router_packages = &ctx.router_packages;

    for summary in summaries.iter() {
        let path = summary.path.as_str();
        let package_directory = builder
            .resolver
            .package_for(path)
            .map_or_else(String::new, |package| package.directory.clone());
        let relative = path
            .strip_prefix(package_directory.as_str())
            .unwrap_or(path)
            .trim_start_matches('/');
        let has_expo_layout = expo_layout_packages.contains(&package_directory);
        let has_nuxt_config = nuxt_config_packages.contains(&package_directory);
        let has_router_package = router_packages.contains_key(&package_directory);
        let router_app = router_packages
            .get(&package_directory)
            .map_or("app", String::as_str);
        let expo_root = expo_roots.get(&package_directory).cloned();
        let module = module_symbol_key(path);
        if summary.parse_status != ParseStatus::Ok {
            if let Some((framework, kind)) = framework_module(path) {
                add(builder, module, framework, kind, route_of(path), "both");
            }
            continue;
        }
        if test_config_modules.contains(path) {
            add(
                builder,
                module.clone(),
                "test-config",
                "test-config-reference",
                None,
                "both",
            );
        }
        let vite_package = vite_packages.contains(&package_directory);
        let storybook_package = storybook_packages.contains(&package_directory);
        // Vite turns the module scripts of `index.html` into production
        // entries. The document itself is loaded by the server, and every
        // export of the script it names is part of the page's surface.
        if vite_package && relative == "index.html" {
            add(
                builder,
                module.clone(),
                "vite",
                "html-entry",
                None,
                "client",
            );
            for import in &summary.imports {
                let Some(target) = builder.resolver.resolve(path, &import.source).target else {
                    continue;
                };
                if !analyzed.contains(target.as_str()) {
                    continue;
                }
                for (_, key) in builder.exports.get(&target).cloned().unwrap_or_default() {
                    add(builder, key, "vite", "html-entry-export", None, "client");
                }
                add(
                    builder,
                    module_symbol_key(&target),
                    "vite",
                    "html-entry-script",
                    None,
                    "client",
                );
            }
        }
        // Fallow keeps the standard Vite application roots. Protecting them
        // covers vanilla and Vue-style bootstraps that never call React's
        // `createRoot`.
        if vite_package && vite_root_entry(relative) {
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "vite", "root-export", None, "both");
            }
            add(builder, module.clone(), "vite", "root-entry", None, "both");
        }
        if next_config_modules.contains(path) {
            // Everything exported by a config-referenced module is
            // reachable through the configuration (loaders may expose
            // helper exports such as Turbopack's `raw`). Protect the whole
            // surface rather than guessing which hook the tool calls.
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(
                    builder,
                    key,
                    "next-config",
                    "config-reference-export",
                    None,
                    "both",
                );
            }
            add(
                builder,
                module.clone(),
                "next-config",
                "config-reference",
                None,
                "both",
            );
        }
        if webpack_entry_modules.contains(path) {
            // A webpack entry is loaded by the bundler rather than imported.
            // Its exports are part of the entry surface, matching how the
            // other framework roots are treated.
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "webpack", "entry-export", None, "both");
            }
            add(builder, module.clone(), "webpack", "entry", None, "both");
        }
        if expo_config_modules.contains(path) {
            // Expo resolves local config plugins without an import. Their
            // default export is the plugin function, and helper exports
            // are part of the same loaded module surface.
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(
                    builder,
                    key,
                    "expo-config",
                    "config-reference-export",
                    None,
                    "both",
                );
            }
            add(
                builder,
                module.clone(),
                "expo-config",
                "config-reference",
                None,
                "both",
            );
        }
        let file_name = path.rsplit('/').next().unwrap_or(path);
        let package_directory = builder
            .resolver
            .package_for(path)
            .map_or_else(String::new, |package| package.directory.clone());
        let configured_extensions = page_extensions.get(&package_directory);
        let stem = next_convention_stem(file_name, configured_extensions);
        let route = route_of(path);
        let package_path = if package_directory.is_empty() {
            path
        } else {
            path.strip_prefix(&format!("{package_directory}/"))
                .unwrap_or(path)
        };
        let is_app_shape = package_path.starts_with("app/") || package_path.starts_with("src/app/");
        let is_pages_shape =
            package_path.starts_with("pages/") || package_path.starts_with("src/pages/");
        // Next.js middleware/proxy, instrumentation, and the MDX component
        // map live at the project root (or `src/`), not under `app/`, but
        // they are still framework entrypoints.
        let root_convention_shape = matches!(
            stem,
            "middleware"
                | "proxy"
                | "mdx-components"
                | "instrumentation"
                | "instrumentation-client"
        ) && next_convention_base(path, configured_extensions)
            .is_some_and(|base| {
                [
                    "middleware",
                    "proxy",
                    "mdx-components",
                    "instrumentation",
                    "instrumentation-client",
                    "src/middleware",
                    "src/proxy",
                    "src/mdx-components",
                    "src/instrumentation",
                    "src/instrumentation-client",
                ]
                .iter()
                .any(|relative| at_own_package_root(base, &package_directory, relative))
            });
        // Next.js conventions only apply to a project that depends on
        // Next.js. An `app/` or `pages/` directory in a Remix, Expo Router,
        // or plain React package is not a Next route, and treating it as
        // one would hide genuinely dead files. Only consult the manifest
        // for paths that could match a convention.
        let package_uses_next = if is_app_shape || is_pages_shape || root_convention_shape {
            builder
                .package_declares_next(&package_directory)
                .unwrap_or(true)
        } else {
            true
        };
        let is_app = package_uses_next && is_app_shape;
        let is_pages = package_uses_next && is_pages_shape;
        // `mdx-components` is not a Next.js core convention: `@next/mdx`
        // aliases `next-mdx-import-source-file` to the file (at the package
        // root or in `src/`), and nothing else loads it by name. Knip's
        // Next MDX plugin gates the entry on `@next/mdx` for the same
        // reason. Without the package, the file is ordinary application
        // code: an explicit import still keeps it live, but an unreferenced
        // component map becomes a deletion candidate.
        let uses_mdx_component_map = stem == "mdx-components";
        let package_uses_mdx = if uses_mdx_component_map {
            let local = builder
                .package_declares_any(&package_directory, &["@next/mdx"])
                .unwrap_or(true);
            // Workspaces frequently hoist the MDX plugin to the root even
            // when each app declares `next` itbuilder. A hoisted dependency
            // must not make the map look dead.
            let hoisted = !package_directory.is_empty()
                && builder
                    .package_declares_any("", &["@next/mdx"])
                    .unwrap_or(false);
            local || hoisted
        } else {
            true
        };
        let root_convention = package_uses_next && root_convention_shape && package_uses_mdx;
        // next-intl loads its request/routing modules by convention. Gate
        // the protection on the project actually depending on next-intl so
        // an unrelated `i18n/request.ts` stays analyzable.
        if is_next_intl_convention(path)
            && builder.package_depends_on(&package_directory, "next-intl")
        {
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "next-intl", "convention-export", None, "both");
            }
            add(
                builder,
                module.clone(),
                "next-intl",
                "convention",
                None,
                "both",
            );
        }
        if docusaurus_page_files.contains(path) {
            // Docusaurus renders every page module and reads the MDX
            // module surface, so its imports and exports are framework
            // API rather than dead application code.
            add(builder, module.clone(), "docusaurus", "page", None, "both");
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "docusaurus", "page-export", None, "both");
            }
        }
        if gatsby_template_files.contains(path) {
            // Gatsby renders the default component and reads the page
            // query, Head, and getServerData exports. Other exports in a
            // live template remain application code.
            add(
                builder,
                module.clone(),
                "gatsby",
                "page-template",
                None,
                "both",
            );
            for (name, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                if matches!(
                    name.as_str(),
                    "default" | "query" | "Head" | "getServerData"
                ) {
                    add(builder, key, "gatsby", "page-template-export", None, "both");
                }
            }
        }
        if gatsby_packages.contains(&package_directory) && package_path.starts_with("src/pages/") {
            // Gatsby's default page directory is `src/pages`. The default
            // component and the page query/Head/getServerData exports are
            // framework API; other exports remain application code.
            add(builder, module.clone(), "gatsby", "page", None, "both");
            for (name, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                if matches!(
                    name.as_str(),
                    "default" | "query" | "Head" | "getServerData"
                ) {
                    add(builder, key, "gatsby", "page-export", None, "both");
                }
            }
        }
        // `middleware`, `proxy`, and instrumentation live at the package
        // root (or `src/`), never under `app/`; `root_convention` covers
        // those. The root-only app conventions are ignored inside a route
        // group, parallel slot, or nested segment, so only a direct app
        // root file is a framework entry.
        let app_entry = (is_app
            && (NEXT_APP_ANY_DEPTH_STEMS.contains(&stem)
                || (NEXT_APP_ROOT_ONLY_STEMS.contains(&stem) && is_app_root_file(package_path))))
            || root_convention;
        let pages_entry = is_pages;
        if app_entry || pages_entry {
            let public_names = next_framework_exports(
                stem,
                package_path,
                configured_extensions,
                is_app,
                is_pages,
                root_convention,
            );
            for name in public_names {
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(
                        builder,
                        key,
                        if app_entry { "next-app" } else { "next-pages" },
                        if name == "default" { "route" } else { "export" },
                        route.clone(),
                        "both",
                    );
                }
            }
            add(
                builder,
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
            "root.tsx",
            "root.ts",
            "root.jsx",
            "root.js",
            "entry.client.tsx",
            "entry.client.ts",
            "entry.client.jsx",
            "entry.client.js",
            "entry.server.tsx",
            "entry.server.ts",
            "entry.server.jsx",
            "entry.server.js",
            "routes.ts",
            "routes.js",
            "routes.mts",
            "routes.cts",
            "routes.mjs",
            "routes.cjs",
        ]
        .iter()
        .any(|relative| at_package_root(path, &format!("{router_app}/{relative}")));
        let router_root = ["root.tsx", "root.ts", "root.jsx", "root.js"]
            .iter()
            .any(|relative| at_package_root(path, &format!("{router_app}/{relative}")));
        // In framework mode every `<appDirectory>/routes/` file is a route
        // module, including files named `page.tsx`. The per-file import
        // check remains for standalone/Remix layouts without a routes
        // config.
        let router_route = (router_route_files.contains(path)
            || in_directory(path, &format!("{router_app}/routes/")))
            && (has_router_package || (router_import && stem != "page"));
        // Knip's Remix plugin lists a root `server.{js,ts}` as a production
        // entry: Remix apps with a custom server execute this file instead
        // of `remix-serve`. Only the module is an entry — nothing in Remix
        // reads its named exports — so its top-level work and imports stay
        // live while genuinely unused exports remain reportable.
        let custom_server_entry = has_router_package
            && [
                "server.ts",
                "server.tsx",
                "server.js",
                "server.jsx",
                "server.mjs",
                "server.cjs",
            ]
            .iter()
            .any(|relative| at_own_package_root(path, &package_directory, relative));
        if custom_server_entry {
            add(
                builder,
                module.clone(),
                "remix",
                "custom-server",
                None,
                "server",
            );
        } else if package_uses_next
            && [
                "server.js",
                "server.mjs",
                "server.cjs",
                "server.ts",
                "server.mts",
                "server.cts",
            ]
            .iter()
            .any(|relative| at_own_package_root(path, &package_directory, relative))
        {
            // Next.js apps commonly run their own HTTP server instead of
            // `next start`. Knip's Node plugin protects a root `server.js`
            // for this reason. Seed only the module: the runtime executes
            // it, but nothing reads its named exports.
            add(
                builder,
                module.clone(),
                "next-app",
                "custom-server",
                None,
                "server",
            );
        }
        if (router_import || has_router_package) && (router_entry || router_route) {
            add(
                builder,
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
                // React Router 7.3-7.8 exposed the middleware exports under
                // `unstable_` names before they stabilized.
                "unstable_middleware",
                "unstable_clientMiddleware",
                "meta",
                "links",
                "headers",
                "ErrorBoundary",
                // Remix v1 used a distinct boundary for thrown responses.
                "CatchBoundary",
                "HydrateFallback",
                "shouldRevalidate",
                // Remix v1's revalidation hook and its unstable spelling.
                "shouldReload",
                "unstable_shouldReload",
                "handle",
                "Component",
                "config",
            ] {
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(builder, key, "remix", "route-export", route.clone(), "both");
                }
            }
            // The root route alone may export `Layout`, the document shell
            // React Router wraps around the default component.
            if router_root {
                if let Some(key) = builder.resolve_export(path, "Layout", &mut BTreeSet::new()) {
                    add(builder, key, "remix", "route-export", route.clone(), "both");
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
                builder,
                module.clone(),
                "tanstack",
                "route",
                route.clone(),
                "both",
            );
            for name in ["Route", "route"] {
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(
                        builder,
                        key,
                        "tanstack",
                        "route-export",
                        route.clone(),
                        "both",
                    );
                }
            }
        }

        // Expo Router uses `app/` with `_layout` files and `+` special
        // routes. A layout is the strong signal that the tree is Expo.
        let expo_import = summary
            .imports
            .iter()
            .any(|import| import.source.starts_with("expo-router"));
        // Expo Router treats every module under its configured root as a
        // route. The default is `app/` or `src/app/`, but the `expo-router`
        // config plugin can move it (for example `src/routes`). The
        // `_layout` file and a configured root are strong signals that the
        // tree is Expo rather than Next; a direct `expo-router` import is
        // the weaker one.
        let expo_app = match expo_root.as_deref() {
            Some(root) => in_directory(path, &format!("{root}/")),
            None => in_directory(path, "app/") || in_directory(path, "src/app/"),
        };
        if expo_app && !app_entry && (has_expo_layout || expo_import || expo_root.is_some()) {
            add(
                builder,
                module.clone(),
                "expo",
                "route",
                route.clone(),
                "both",
            );
            let mut public_names = vec![
                "default",
                "unstable_settings",
                "ErrorBoundary",
                "getDerivedStateFromError",
            ];
            // `+api` routes are dispatched by HTTP method. Expo Router
            // reads these named handlers directly from the route module.
            if stem.ends_with("+api") {
                public_names.extend(["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]);
            }
            // `+native-intent` is consulted before native deep links are
            // resolved. `legacy_subscribe` is the alpha React Navigation
            // bridge; both are framework-invoked exports.
            if stem == "+native-intent" {
                public_names.extend(["redirectSystemPath", "legacy_subscribe"]);
            }
            for name in public_names {
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(builder, key, "expo", "route-export", route.clone(), "both");
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
                builder,
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
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(
                        builder,
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
                builder,
                module.clone(),
                "astro",
                "endpoint",
                route.clone(),
                "server",
            );
            for name in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(
                        builder,
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
                builder,
                module.clone(),
                "nuxt",
                "server",
                route.clone(),
                "server",
            );
            for name in ["default", "handler"] {
                if let Some(key) = builder.resolve_export(path, name, &mut BTreeSet::new()) {
                    add(
                        builder,
                        key,
                        "nuxt",
                        "server-export",
                        route.clone(),
                        "server",
                    );
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
            let key = builder
                .locals
                .get(path)
                .and_then(|locals| locals.get(&declaration.name))
                .cloned();
            if let Some(key) = key {
                add(builder, key, "ambient", "declaration", None, "both");
            }
        }
        if summary.has_directive("use client")
            && builder.config.client_boundary_policy == ClientBoundaryPolicy::Protect
        {
            for (name, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                let _ = name;
                add(builder, key, "react", "client-boundary", None, "both");
            }
            add(builder, module.clone(), "react", "module", None, "both");
        }
        if summary.has_directive("use server")
            && builder.config.server_action_policy == ServerActionPolicy::Protect
        {
            // Under `protect`, Next.js registers every export of a
            // `"use server"` module in the server-reference manifest,
            // whether or not application code imports it. Actions are
            // invoked through serialized references, so the import graph
            // is not evidence that an export is unused. The default
            // `graph` policy instead treats an unreferenced action as
            // dead, matching knip and fallow; build evidence can still
            // keep a referenced action live.
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(
                    builder,
                    key,
                    "next-app",
                    "server-action-export",
                    None,
                    "server",
                );
            }
            add(
                builder,
                module.clone(),
                "next-app",
                "server-action",
                None,
                "server",
            );
            if builder.config.client_boundary_policy == ClientBoundaryPolicy::Protect {
                add(builder, module.clone(), "react", "module", None, "both");
            }
        }
        if builder.config.server_action_policy == ServerActionPolicy::Protect
            && rsc_directives
            && !summary.has_directive("use server")
            && !summary.inline_server_action_exports.is_empty()
        {
            // An inline `"use server"` body directive is a Server Action
            // even without a file-level directive. The action is invoked
            // through a serialized reference, so `protect` keeps it live
            // exactly like an export of a whole `"use server"` module.
            let exports = builder.exports.get(path).cloned().unwrap_or_default();
            for name in &summary.inline_server_action_exports {
                if let Some(key) = exports.get(name) {
                    add(
                        builder,
                        key.clone(),
                        "react",
                        "inline-server-action",
                        None,
                        "server",
                    );
                }
            }
        }
        let cosmos_fixture = cosmos_fixture_files.contains(path);
        let cosmos_decorator = cosmos_decorator_files.contains(path);
        if builder.config.test_entries && builder.config.entry_policy != EntryPolicy::Production {
            if cosmos_decorator {
                // Cosmos invokes the decorator module's default export. A
                // named helper in the same file is not framework-invoked
                // and stays analyzable.
                add(
                    builder,
                    module.clone(),
                    "react-cosmos",
                    "decorator",
                    None,
                    "both",
                );
                add(
                    builder,
                    format!("{path}#default"),
                    "react-cosmos",
                    "decorator-default",
                    None,
                    "both",
                );
            } else if cosmos_fixture {
                // Cosmos imports the fixture module and renders every
                // exported fixture.
                add(
                    builder,
                    module.clone(),
                    "react-cosmos",
                    "fixture",
                    None,
                    "both",
                );
                for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                    add(builder, key, "react-cosmos", "fixture-export", None, "both");
                }
            }
        }
        let cypress_spec = cypress_specs.contains(path);
        let cypress_support_file = cypress_support.contains(path);
        if builder.config.test_entries
            && builder.config.entry_policy != EntryPolicy::Production
            && (cypress_spec || cypress_support_file)
        {
            // Cypress executes the module for its hooks; like other test
            // runners it does not consume arbitrary named exports.
            add(
                builder,
                module.clone(),
                "cypress",
                if cypress_support_file {
                    "support"
                } else {
                    "spec"
                },
                None,
                "both",
            );
        }
        let test = !cypress_spec
            && !cypress_support_file
            && (path.contains(".test.")
                    || path.contains(".spec.")
                    || path.contains(".test-d.")
                    || path.contains("__tests__/")
                    || path.contains("__typetest__/")
                    || path.contains("__mocks__/")
                    // Node's test runner executes every file under a `test/`
                    // directory. Other runners discover only matching
                    // filenames, so support modules under `tests/` follow the
                    // graph like any other module unless a test or a test
                    // config references them.
                    || path.starts_with("test/")
                    || path.contains("/test/"));
        let story = path.contains(".stories.")
                || path.contains(".story.")
                || path.starts_with(".storybook/")
                || path.contains("/.storybook/")
                || path.starts_with(".rnstorybook/")
                || path.contains("/.rnstorybook/")
                || storybook_story_files.contains(path)
                || ladle_story_files.contains(path)
                || ladle_entry_files.contains(path)
                // Storybook's default glob includes every MDX page. Docs pages
                // are entries, and their ESM imports keep rendered components
                // live. Gate on the dependency so ordinary `.mdx` content
                // stays analyzable in projects without Storybook.
                || (storybook_package && path.ends_with(".mdx"));
        if builder.config.test_entries
            && builder.config.entry_policy != EntryPolicy::Production
            && (test || story)
            && !cosmos_fixture
            && !cosmos_decorator
        {
            add(builder, module.clone(), "tests", "test", None, "both");
            // Storybook and Ladle render every export of a story module.
            // Jest loads `__mocks__` by convention, so every mock export
            // must stay available even when the mock is not imported by
            // path. Type tests export aliases to assert them. Ordinary
            // test files are executed for their top-level test calls;
            // their exports are helpers and follow the graph like any
            // other module, matching knip and fallow.
            let framework_exports = story
                || path.contains("__mocks__/")
                || path.contains("__typetest__/")
                || path.contains(".test-d.");
            if framework_exports {
                for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                    add(builder, key, "tests", "test-export", None, "both");
                }
            }
        }
        // Standalone examples and benchmarks are run directly rather than
        // imported. The module is the root, so its top-level work stays
        // live; its exports are ordinary helpers and follow the graph.
        // Files under `scripts/` are not entries by directory alone: a
        // documented header or shebang is not a reference. They enter only
        // when package.json names them or another module imports them.
        let local_root = path.starts_with("examples/")
            || path.starts_with("example/")
            || path.starts_with("demo/")
            || path.starts_with("playground/")
            || path.starts_with("playgrounds/")
            || path.starts_with("benchmarks/");
        if builder.config.entry_policy != EntryPolicy::Production && local_root {
            add(builder, module.clone(), "local", "standalone", None, "both");
        }
        // Gatsby loads lifecycle files by convention and pages under
        // `src/pages`; both are executed directly by the framework.
        let gatsby_lifecycle = matches!(
            stem,
            "gatsby-config" | "gatsby-node" | "gatsby-browser" | "gatsby-ssr" | "gatsby-shared"
        );
        if builder.config.entry_policy != EntryPolicy::Production && gatsby_lifecycle {
            add(builder, module.clone(), "gatsby", "lifecycle", None, "both");
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "gatsby", "lifecycle-export", None, "both");
            }
        }
        // Gatsby functions under `src/api` are HTTP endpoints executed by
        // the framework, not imported by application code.
        let gatsby_function = path.starts_with("src/api/") || path.contains("/src/api/");
        if builder.config.entry_policy != EntryPolicy::Production && gatsby_function {
            add(
                builder,
                module.clone(),
                "gatsby",
                "function",
                None,
                "server",
            );
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "gatsby", "function-export", None, "server");
            }
        }
        if builder.config.entry_policy != EntryPolicy::Production
            && is_vercel_function(builder.resolver.root(), path)
        {
            add(
                builder,
                module.clone(),
                "vercel",
                "function",
                None,
                "server",
            );
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "vercel", "function-export", None, "server");
            }
        }
        // Build/test tool configs are consumed directly by the tool, so
        // both the module and everything it exports are entries. Matching
        // on the `.config` suffix covers rollup, drizzle, shrike, and any
        // future tool without maintaining a name list.
        let config_file = stem.ends_with(".config") || stem == "tsconfig";
        if config_file {
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "config", "config-export", None, "both");
            }
            add(builder, module.clone(), "config", "config", None, "both");
        }
        let react_root = summary
            .module_refs
            .identifiers
            .iter()
            .any(|id| id == "createRoot" || id == "hydrateRoot")
            || react_dom_legacy_root(summary);
        if react_root {
            add(builder, module.clone(), "react", "root", None, "client");
            for identifier in &summary.module_refs.identifiers {
                if let Some(target) = builder.local_or_imported(path, identifier) {
                    add(builder, target, "react", "root-reference", None, "client");
                }
            }
        }
        let package_flags = builder.resolver.package_for(path).map(|package| {
            (
                package.script_files.contains(path),
                package.public_sources.contains(path) || package.entry.as_deref() == Some(path),
            )
        });
        if let Some((is_script, is_public_source)) = package_flags {
            if is_script {
                add(builder, module.clone(), "scripts", "script", None, "both");
            }
            if is_public_source {
                for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                    add(builder, key, "packages", "public-export", None, "both");
                }
                add(
                    builder,
                    module.clone(),
                    "packages",
                    "package-entry",
                    None,
                    "both",
                );
            }
        }
        if builder.public_files.contains(path) {
            for (_, key) in builder.exports.get(path).cloned().unwrap_or_default() {
                add(builder, key, "packages", "public-api", None, "both");
            }
        }
    }
}
