//! Precompute package/framework facts once before entry classification.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use super::Builder;
use super::context::EntryContext;
use super::helpers::*;
use crate::config::webpack::{self, EntryBase};
use crate::config::{cosmos, cypress, docusaurus, gatsby, ladle, next, storybook, test};

impl Builder<'_> {
    /// Next's `pageExtensions` may contain dots (`page.tsx`). Knip reads it
    /// from the config AST; read only the literal values here and scope each
    /// config to its package so a nested app is not widened by the root.
    pub(crate) fn configured_page_extensions(&mut self) -> BTreeMap<String, BTreeSet<String>> {
        if self.page_extensions.is_none() {
            let mut page_extensions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for summary in self.summaries.iter() {
                if !next::is_config_path(&summary.path) {
                    continue;
                }
                let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path))
                else {
                    continue;
                };
                let extensions = next::page_extensions(&source);
                if extensions.is_empty() {
                    continue;
                }
                let package_directory = self
                    .resolver
                    .package_for(&summary.path)
                    .map_or_else(String::new, |package| package.directory.clone());
                page_extensions
                    .entry(package_directory)
                    .or_default()
                    .extend(extensions);
            }
            self.page_extensions = Some(page_extensions);
        }
        self.page_extensions.clone().unwrap_or_default()
    }
}

pub(crate) fn build(builder: &mut Builder<'_>) -> EntryContext {
    let summaries = builder.summaries.clone();
    let rsc_directives = builder.project_uses_rsc_directives();
    let analyzed: BTreeSet<&str> = summaries
        .iter()
        .map(|summary| summary.path.as_str())
        .collect();
    let page_extensions = builder.configured_page_extensions();
    let mut vite_packages: BTreeSet<String> = BTreeSet::new();
    let mut storybook_packages: BTreeSet<String> = BTreeSet::new();
    let mut cosmos_packages: BTreeSet<String> = BTreeSet::new();
    let mut docusaurus_packages: BTreeSet<String> = BTreeSet::new();
    let mut gatsby_packages: BTreeSet<String> = BTreeSet::new();
    let mut ladle_packages: BTreeSet<String> = BTreeSet::new();
    for package in builder.resolver.packages() {
        if builder.package_depends_on_any(&package.directory, VITE_ENABLERS) {
            vite_packages.insert(package.directory.clone());
        }
        if builder.package_depends_on_storybook(&package.directory) {
            storybook_packages.insert(package.directory.clone());
        }
        if builder.package_depends_on(&package.directory, "react-cosmos") {
            cosmos_packages.insert(package.directory.clone());
        }
        if builder.package_depends_on_any(
            &package.directory,
            &[
                "@docusaurus/core",
                "@docusaurus/preset-classic",
                "docusaurus",
            ],
        ) {
            docusaurus_packages.insert(package.directory.clone());
        }
        if builder.package_depends_on_any(&package.directory, &["gatsby", "gatsby-cli"]) {
            gatsby_packages.insert(package.directory.clone());
        }
        if builder.package_depends_on(&package.directory, "@ladle/react") {
            ladle_packages.insert(package.directory.clone());
        }
    }
    let mut expo_layout_packages: BTreeSet<String> = BTreeSet::new();
    let mut expo_roots: BTreeMap<String, String> = BTreeMap::new();
    let mut expo_config_modules: BTreeSet<String> = BTreeSet::new();
    let mut nuxt_config_packages: BTreeSet<String> = BTreeSet::new();
    let mut router_packages: BTreeMap<String, String> = BTreeMap::new();
    // Expo Router reads its route root from the `expo-router` config
    // plugin in the app config. The same file may name local config
    // plugins by path. These files are usually JSON and are not parsed as
    // modules, so read them from each workspace package. `app.config.*`
    // takes precedence over `app.json` when both exist.
    for package in builder.resolver.packages() {
        for file_name in [
            "app.json",
            "app.config.json",
            "app.config.js",
            "app.config.mjs",
            "app.config.cjs",
            "app.config.ts",
            "app.config.mts",
            "app.config.cts",
        ] {
            let relative = if package.directory.is_empty() {
                file_name.to_string()
            } else {
                format!("{}/{file_name}", package.directory)
            };
            let Ok(source) = fs::read_to_string(builder.resolver.root().join(&relative)) else {
                continue;
            };
            // Expo resolves local config plugins when it loads the app
            // config. They are entries even when nothing imports them.
            for specifier in expo_config_plugin_paths(&source) {
                if let Some(target) = builder.resolver.resolve(&relative, &specifier).target {
                    if analyzed.contains(target.as_str()) {
                        expo_config_modules.insert(target);
                    }
                }
            }
            let Some(root) = expo_router_root(&source) else {
                continue;
            };
            if file_name == "app.json" {
                expo_roots.entry(package.directory.clone()).or_insert(root);
            } else {
                expo_roots.insert(package.directory.clone(), root);
            }
        }
    }
    // A layout is the strong signal that a directory tree is Expo Router.
    // Detect it before the entry pass so file order cannot hide routes.
    for summary in summaries.iter() {
        let package_directory = builder
            .resolver
            .package_for(&summary.path)
            .map_or("", |package| package.directory.as_str());
        let relative = summary
            .path
            .strip_prefix(package_directory)
            .unwrap_or(&summary.path)
            .trim_start_matches('/');
        let layout = |root: &str| {
            ["tsx", "ts", "jsx", "js"]
                .iter()
                .any(|extension| relative == format!("{root}/_layout.{extension}"))
        };
        let is_layout = match expo_roots.get(package_directory) {
            Some(root) => layout(root),
            None => layout("app") || layout("src/app"),
        };
        if is_layout {
            expo_layout_packages.insert(package_directory.to_string());
        }
    }
    for summary in summaries.iter() {
        let package_directory = builder
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
        // The config may move the app directory, so read `appDirectory`.
        // Remix has no route map, but its root module always imports the
        // framework; use that to protect routes that have no imports.
        let router_config = matches!(
            relative,
            "react-router.config.ts"
                | "react-router.config.js"
                | "react-router.config.mts"
                | "react-router.config.cts"
                | "react-router.config.mjs"
                | "react-router.config.cjs"
        );
        let router_routes = matches!(
            relative,
            "app/routes.ts"
                | "app/routes.js"
                | "app/routes.mts"
                | "app/routes.cts"
                | "app/routes.mjs"
                | "app/routes.cjs"
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
        if router_config {
            let app_directory = fs::read_to_string(builder.resolver.root().join(&summary.path))
                .ok()
                .and_then(|source| react_router_app_directory(&source))
                .unwrap_or_else(|| "app".to_string());
            router_packages.insert(package_directory.to_string(), app_directory);
        } else if (router_routes && imports_router)
            || (router_root && (imports_router || imports_remix))
        {
            router_packages
                .entry(package_directory.to_string())
                .or_insert_with(|| "app".to_string());
        }
    }
    // Next.js loads configured modules directly (custom image loader,
    // cache handlers, Turbopack loaders/aliases). They are roots even
    // though no application module imports them.
    let mut next_config_modules: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
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
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            continue;
        };
        for specifier in next_config_references(&source) {
            let resolved = builder.resolver.resolve(&summary.path, &specifier);
            if let Some(target) = resolved.target {
                if analyzed.contains(target.as_str()) {
                    next_config_modules.insert(target);
                }
            }
        }
    }
    // Webpack loads its configured entries and never imports them from
    // application code. A static read of `entry`/`context` keeps React and
    // vanilla bootstraps live without executing the config.
    let mut webpack_entry_modules: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
        if !webpack::is_config_path(&summary.path) {
            continue;
        }
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            continue;
        };
        let package_directory = builder
            .resolver
            .package_for(&summary.path)
            .map_or("", |package| package.directory.as_str());
        for entry in webpack::entries(&summary.path, &source) {
            let (importer, specifier) = match entry.base {
                EntryBase::ConfigDir => (summary.path.clone(), entry.specifier),
                EntryBase::PackageDir => {
                    let importer = if package_directory.is_empty() {
                        "__webpack_entry__".to_string()
                    } else {
                        format!("{package_directory}/__webpack_entry__")
                    };
                    (importer, entry.specifier)
                }
            };
            let specifier = if specifier.starts_with('.') {
                specifier
            } else {
                format!("./{specifier}")
            };
            let resolved = builder.resolver.resolve(&importer, &specifier);
            if let Some(target) = resolved.target {
                if analyzed.contains(target.as_str()) {
                    webpack_entry_modules.insert(target);
                }
            }
        }
    }
    // Storybook's `stories` option can rename the story suffix or narrow
    // discovery to another directory. Read literal patterns from each main
    // config and make every matched module a story entry. Functions and
    // computed values are skipped; the conventional suffix rules below
    // still apply.
    let mut storybook_story_files: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
        if !storybook::is_config_path(&summary.path) {
            continue;
        }
        let package_directory = builder
            .resolver
            .package_for(&summary.path)
            .map_or("", |package| package.directory.as_str());
        if !storybook_packages.contains(package_directory) {
            continue;
        }
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            continue;
        };
        let patterns = storybook::story_patterns(&summary.path, &source);
        let Some(matcher) = storybook::matcher(&patterns) else {
            continue;
        };
        for path in &analyzed {
            if matcher.is_match(path) {
                storybook_story_files.insert((*path).to_string());
            }
        }
    }
    // Ladle loads `.ladle/components.*` as the provider entry and discovers
    // stories from its default `src/**/*.stories.*` glob. A
    // `.ladle/config.*` file can replace that glob, and can point at a Vite
    // config that Ladle executes. Computed story values fall back to the
    // default glob so a dynamic config cannot erase story protection.
    let mut ladle_story_files: BTreeSet<String> = BTreeSet::new();
    let mut ladle_entry_files: BTreeSet<String> = BTreeSet::new();
    for package_directory in &ladle_packages {
        let mut patterns: Vec<String> = Vec::new();
        let mut saw_config = false;
        for summary in summaries.iter() {
            if !ladle::is_config_path(&summary.path)
                || ladle::package_directory(&summary.path) != Some(package_directory.as_str())
            {
                continue;
            }
            saw_config = true;
            ladle_entry_files.insert(summary.path.clone());
            let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
                continue;
            };
            let scan = ladle::scan(&summary.path, &source);
            patterns.extend(ladle::story_patterns(package_directory, &scan));
            if let Some(vite_config) = ladle::vite_config(package_directory, &scan) {
                ladle_entry_files.insert(vite_config);
            }
        }
        if !saw_config {
            patterns.extend(ladle::story_patterns(
                package_directory,
                &ladle::ConfigScan::default(),
            ));
        }
        let Some(matcher) = storybook::matcher(&patterns) else {
            continue;
        };
        for path in &analyzed {
            if matcher.is_match(path) {
                ladle_story_files.insert((*path).to_string());
            }
        }
    }
    for summary in summaries.iter() {
        if ladle::is_components_path(&summary.path)
            && ladle::package_directory(&summary.path)
                .is_some_and(|package| ladle_packages.contains(package))
        {
            ladle_entry_files.insert(summary.path.clone());
        }
    }
    // React Cosmos loads fixture and decorator modules by convention instead
    // of importing them. The package dependency enables the conventions;
    // `cosmos.config.json` can rename the fixture directory and suffix.
    // Decorators are removed from the fixture set first, matching Cosmos's
    // own user-module discovery.
    let mut cosmos_fixture_files: BTreeSet<String> = BTreeSet::new();
    let mut cosmos_decorator_files: BTreeSet<String> = BTreeSet::new();
    for package_directory in &cosmos_packages {
        let config = cosmos::CosmosConfig::load(builder.resolver.root(), package_directory);
        let Some(matcher) = cosmos::matcher(package_directory, &config) else {
            continue;
        };
        for path in &analyzed {
            if matcher.is_decorator(path) {
                cosmos_decorator_files.insert((*path).to_string());
            } else if matcher.is_fixture(path) {
                cosmos_fixture_files.insert((*path).to_string());
            }
        }
    }
    // Docusaurus renders docs, blog, and versioned-doc MDX pages by
    // convention. The page modules are not imported, so their own imports
    // need the page entry to stay live.
    let mut docusaurus_page_files: BTreeSet<String> = BTreeSet::new();
    for package_directory in &docusaurus_packages {
        let Some(matcher) = docusaurus::matcher(package_directory) else {
            continue;
        };
        for path in &analyzed {
            if matcher.is_match(path) {
                docusaurus_page_files.insert((*path).to_string());
            }
        }
    }
    // Gatsby page templates are loaded by `createPage`, not by directory
    // convention. Follow the static component values in `gatsby-node.*`.
    // When a component is opaque, protect the conventional template
    // directory for that package rather than reporting a live page.
    let mut gatsby_template_files: BTreeSet<String> = BTreeSet::new();
    let mut gatsby_dynamic_packages: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
        if !gatsby::is_node_path(&summary.path) {
            continue;
        }
        let Some(package) = builder.resolver.package_for(&summary.path) else {
            continue;
        };
        if !gatsby_packages.contains(&package.directory) {
            continue;
        }
        // Gatsby only executes a node file at the package root or in a
        // local plugin directory, not at an arbitrary nested path such as
        // `src/gatsby-node.ts`.
        let relative = summary
            .path
            .strip_prefix(package.directory.as_str())
            .unwrap_or(&summary.path)
            .trim_start_matches('/');
        let in_plugin = relative.starts_with("plugins/");
        let at_root = !relative.contains('/');
        if !at_root && !in_plugin {
            continue;
        }
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            gatsby_dynamic_packages.insert(package.directory.clone());
            continue;
        };
        let scan = gatsby::template_references(&summary.path, &source);
        // A plugin's `gatsby-node` resolves page components from the site
        // that loads it, not from the plugin directory. Try the node file
        // first (for plugin-local templates) and the containing Gatsby
        // package second (for site templates).
        let site_anchor = if in_plugin {
            gatsby_packages
                .iter()
                .filter(|directory| {
                    directory.is_empty() || summary.path.starts_with(&format!("{directory}/"))
                })
                .min_by_key(|directory| directory.len())
                .map(|directory| {
                    if directory.is_empty() {
                        "__gatsby_node__".to_string()
                    } else {
                        format!("{directory}/__gatsby_node__")
                    }
                })
        } else {
            None
        };
        for specifier in scan.specifiers {
            for importer in std::iter::once(summary.path.as_str()).chain(site_anchor.as_deref()) {
                let resolved = builder.resolver.resolve(importer, &specifier);
                if let Some(target) = resolved.target
                    && analyzed.contains(target.as_str())
                {
                    gatsby_template_files.insert(target);
                }
            }
        }
        if scan.dynamic {
            gatsby_dynamic_packages.insert(package.directory.clone());
        }
    }
    for package_directory in &gatsby_dynamic_packages {
        let prefix = if package_directory.is_empty() {
            "src/templates/".to_string()
        } else {
            format!("{package_directory}/src/templates/")
        };
        for path in &analyzed {
            if path.starts_with(&prefix) {
                gatsby_template_files.insert((*path).to_string());
            }
        }
    }
    // React Router route configs name route modules as strings. The router
    // imports them at runtime, so files named outside `<appDirectory>/routes/`
    // are still route modules and must keep their framework exports.
    let mut router_route_files: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
        let package_directory = builder
            .resolver
            .package_for(&summary.path)
            .map_or("", |package| package.directory.as_str());
        let Some(app_directory) = router_packages.get(package_directory) else {
            continue;
        };
        let relative = summary
            .path
            .strip_prefix(package_directory)
            .unwrap_or(&summary.path)
            .trim_start_matches('/');
        let prefix = format!("{app_directory}/routes.");
        if !relative.starts_with(&prefix)
            || !matches!(
                relative.rsplit('.').next(),
                Some("ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" | "cts" | "cjs")
            )
        {
            continue;
        }
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            continue;
        };
        for target in react_router_route_targets(&source) {
            if let Some(path) =
                resolve_route_target(package_directory, app_directory, &target, &analyzed)
            {
                router_route_files.insert(path);
            }
        }
    }
    // Test runners load setup files, global setup, and alias targets by
    // path from their configuration. A support file that no test imports
    // is still live when the runner names it, so resolve the config's
    // path literals against analyzed files before classifying entries.
    let mut test_config_modules: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
        if !test::is_config_path(&summary.path) {
            continue;
        }
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            continue;
        };
        test_config_modules.extend(test::referenced_files(
            builder.resolver.root(),
            &analyzed,
            &summary.path,
            &source,
        ));
    }
    // Cypress discovers specs from `specPattern` and loads its support
    // module before them; neither is imported by application code. Only
    // read configs in packages that depend on Cypress, and only trust
    // literal values, so an unrelated `cypress.config.*` cannot protect
    // dead files.
    let mut cypress_specs: BTreeSet<String> = BTreeSet::new();
    let mut cypress_support: BTreeSet<String> = BTreeSet::new();
    for summary in summaries.iter() {
        if !cypress::is_config_path(&summary.path) {
            continue;
        }
        let package_directory = builder
            .resolver
            .package_for(&summary.path)
            .map_or("", |package| package.directory.as_str());
        if !builder.package_depends_on(package_directory, "cypress") {
            continue;
        }
        let Ok(source) = fs::read_to_string(builder.resolver.root().join(&summary.path)) else {
            continue;
        };
        let entries = cypress::entries(&analyzed, &summary.path, &source);
        cypress_specs.extend(entries.specs);
        cypress_support.extend(entries.support);
    }
    EntryContext {
        summaries,
        rsc_directives,
        page_extensions,
        vite_packages,
        storybook_packages,
        gatsby_packages,
        expo_layout_packages,
        expo_config_modules,
        nuxt_config_packages,
        next_config_modules,
        webpack_entry_modules,
        storybook_story_files,
        ladle_story_files,
        ladle_entry_files,
        cosmos_fixture_files,
        cosmos_decorator_files,
        docusaurus_page_files,
        gatsby_template_files,
        router_route_files,
        test_config_modules,
        cypress_specs,
        cypress_support,
        expo_roots,
        router_packages,
    }
}
