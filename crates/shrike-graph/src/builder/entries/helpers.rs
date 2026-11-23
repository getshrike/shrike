//! Pure helpers for framework entry detection.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use shrike_syntax::{BindingKind, FileSummary};

use super::Builder;

/// Relative module paths named in a `next.config.*` file. Next.js loads these
/// directly (custom image loader, cache handlers, Turbopack loaders/aliases),
/// so they are entries even though no application module imports them. The
/// scan is deliberately lexer-light but skips comments: any relative string is
/// treated as a possible config reference, which fails closed.
pub(crate) fn next_config_references(source: &str) -> Vec<String> {
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
pub(crate) fn is_vercel_function(root: &Path, path: &str) -> bool {
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
pub(crate) fn in_directory(path: &str, directory: &str) -> bool {
    path.starts_with(directory) || path.contains(&format!("/{directory}"))
}

/// Whether `path` is exactly `relative` at the repository root or at the root
/// of a nested workspace package.
pub(crate) fn at_package_root(path: &str, relative: &str) -> bool {
    path == relative || path.ends_with(&format!("/{relative}"))
}

/// Compare a path against a convention relative to its owning package. The
/// package directory is already known, so a nested `lib/middleware.ts` is not
/// mistaken for `<package>/middleware.ts`.
pub(crate) fn at_own_package_root(path: &str, package_directory: &str, relative: &str) -> bool {
    let local = if package_directory.is_empty() {
        path
    } else {
        path.strip_prefix(&format!("{package_directory}/"))
            .unwrap_or(path)
    };
    local == relative
}

/// Configured `pageExtensions` in longest-first order. A dotted value such as
/// `page.tsx` is a single extension, so it must win over the plain `.tsx`
/// suffix before computing a route stem.
pub(crate) fn dotted_page_extensions(extensions: Option<&BTreeSet<String>>) -> Vec<String> {
    let Some(extensions) = extensions else {
        return Vec::new();
    };
    let mut values: Vec<String> = extensions
        .iter()
        .filter(|extension| extension.contains('.'))
        .cloned()
        .collect();
    values.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    values
}

/// The route stem of a Next.js file. With `pageExtensions: ["page.tsx"]`,
/// `page.page.tsx` has the stem `page`, not `page.page`.
pub(crate) fn next_convention_stem<'a>(
    file_name: &'a str,
    extensions: Option<&BTreeSet<String>>,
) -> &'a str {
    for extension in dotted_page_extensions(extensions) {
        if let Some(stem) = file_name.strip_suffix(&format!(".{extension}")) {
            return stem;
        }
    }
    file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
}

/// Strip the module extension from a path before matching a root convention
/// such as `middleware` or `mdx-components`, honouring dotted page extensions.
pub(crate) fn next_convention_base<'a>(
    path: &'a str,
    extensions: Option<&BTreeSet<String>>,
) -> Option<&'a str> {
    for extension in dotted_page_extensions(extensions) {
        if let Some(base) = path.strip_suffix(&format!(".{extension}")) {
            return Some(base);
        }
    }
    [".tsx", ".ts", ".jsx", ".js"]
        .iter()
        .find_map(|suffix| path.strip_suffix(suffix))
}

/// next-intl reads `{src/,}i18n/request.*` and `{src/,}i18n/routing.*` by
/// convention when its plugin is configured. Only the file names are fixed, so
/// the dependency check decides whether the convention applies.
pub(crate) fn is_next_intl_convention(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    matches!(stem, "request" | "routing")
        && matches!(extension, "ts" | "tsx" | "js" | "jsx")
        && at_package_root(path, &format!("i18n/{file}"))
}

/// App Router convention stems that Next.js loads at any depth. `sitemap` and
/// the image-metadata routes are matched anywhere in the app tree.
pub(crate) const NEXT_APP_ANY_DEPTH_STEMS: &[&str] = &[
    "page",
    "layout",
    "template",
    "loading",
    "error",
    "not-found",
    "forbidden",
    "unauthorized",
    "route",
    "default",
    "sitemap",
    "icon",
    "apple-icon",
    "opengraph-image",
    "twitter-image",
];

/// App Router convention stems that Next.js loads only directly under the app
/// root. A route group, parallel slot, or nested segment is not a framework
/// file: `next build` ignores `app/(group)/robots.ts`, `app/@slot/robots.ts`,
/// and `app/nested/robots.ts`. Verified against Next.js 16.4.6.
pub(crate) const NEXT_APP_ROOT_ONLY_STEMS: &[&str] =
    &["global-error", "global-not-found", "manifest", "robots"];

/// Whether a package-relative path is a file directly under the app root.
pub(crate) fn is_app_root_file(package_path: &str) -> bool {
    package_path
        .strip_prefix("app/")
        .or_else(|| package_path.strip_prefix("src/app/"))
        .is_some_and(|rest| !rest.contains('/'))
}

/// Route-level segment config that Next.js reads from every app segment
/// module, including a route handler or metadata route. `dynamicParams` is
/// consumed by static-path generation when a route exports
/// `generateStaticParams`; the rest are read by the app-route module.
pub(crate) const NEXT_APP_ROUTE_CONFIG_EXPORTS: &[&str] = &[
    "dynamic",
    "dynamicParams",
    "revalidate",
    "fetchCache",
    "runtime",
    "preferredRegion",
    "maxDuration",
];

/// Page and layout rendering config. Next.js parses these names on every app
/// segment, but only the component-tree renderer consumes them: route handlers
/// and metadata routes do not render a tree. The list keeps the `unstable_`
/// spellings used by Next.js 16.0-16.2 and `experimental_ppr` for the
/// documented 15.x fallback; Next.js 16.0 removed `experimental_ppr` from
/// `AppSegmentConfigSchemaKeys`.
pub(crate) const NEXT_APP_CACHE_CONFIG_EXPORTS: &[&str] = &[
    "ensureStatic",
    "instant",
    "unstable_instant",
    "prefetch",
    "unstable_prefetch",
    "experimental_ppr",
];

/// Metadata and viewport APIs are read from page, layout, and default modules,
/// plus the HTTP access fallback boundaries Next.js renders with an error
/// convention (`not-found`, `forbidden`, `unauthorized`).
pub(crate) const NEXT_APP_METADATA_EXPORTS: &[&str] = &[
    "metadata",
    "generateMetadata",
    "viewport",
    "generateViewport",
];

/// Static params and parameter matching are read from the modules that
/// represent a segment: page, layout, default, and route handlers.
pub(crate) const NEXT_APP_PARAM_EXPORTS: &[&str] = &[
    "generateStaticParams",
    "unstable_paramMatching",
    "unstable_generateParamMatching",
];

pub(crate) const NEXT_APP_HTTP_METHODS: &[&str] =
    &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// The framework exports Next.js actually reads from a convention file.
///
/// Next.js reads a different surface from each file type: a page reads
/// metadata, segment config, and static params; a route handler reads HTTP
/// methods and segment config but no metadata; `error.tsx` reads only its
/// default component; Pages Router files read data-fetching functions or
/// `config` depending on whether they are a page, `_app`, or API route.
/// Protecting the union on every entry keeps exports Next.js never reads
/// live, so each convention gets its own surface.
pub(crate) fn next_framework_exports(
    stem: &str,
    package_path: &str,
    extensions: Option<&BTreeSet<String>>,
    is_app: bool,
    is_pages: bool,
    root_convention: bool,
) -> Vec<&'static str> {
    let mut names = Vec::new();
    if is_app {
        match stem {
            "page" => {
                names.extend(["default", "unstable_dynamicStaleTime"]);
                names.extend(NEXT_APP_METADATA_EXPORTS);
                names.extend(NEXT_APP_PARAM_EXPORTS);
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
                names.extend(NEXT_APP_CACHE_CONFIG_EXPORTS);
            }
            "layout" | "default" => {
                names.push("default");
                names.extend(NEXT_APP_METADATA_EXPORTS);
                names.extend(NEXT_APP_PARAM_EXPORTS);
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
                names.extend(NEXT_APP_CACHE_CONFIG_EXPORTS);
            }
            "route" => {
                // Next.js reads static params from a route handler but throws
                // when it exports parameter matching, so only the former is
                // framework API here.
                names.extend(NEXT_APP_HTTP_METHODS);
                names.push("generateStaticParams");
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
            }
            "sitemap" => {
                names.extend(["default", "generateSitemaps", "generateStaticParams"]);
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
            }
            "robots" | "manifest" => {
                names.extend(["default", "generateStaticParams"]);
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
            }
            "icon" | "apple-icon" => {
                // The metadata image loader forwards every named export into
                // the icon descriptor, and Next.js spreads that descriptor onto
                // the rendered `<link>`. A probe build emits `alt` on the icon
                // link even though the docs list only `size` and
                // `contentType`.
                names.extend([
                    "default",
                    "size",
                    "contentType",
                    "alt",
                    "generateImageMetadata",
                    "generateStaticParams",
                ]);
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
            }
            "opengraph-image" | "twitter-image" => {
                names.extend([
                    "default",
                    "size",
                    "contentType",
                    "alt",
                    "generateImageMetadata",
                    "generateStaticParams",
                ]);
                names.extend(NEXT_APP_ROUTE_CONFIG_EXPORTS);
            }
            // `errorType` selects these boundaries when Next.js renders an
            // access fallback, and metadata is read from the boundary module.
            "not-found" | "forbidden" | "unauthorized" => {
                names.push("default");
                names.extend(NEXT_APP_METADATA_EXPORTS);
            }
            // `global-not-found` renders with `errorType` undefined, which
            // reads the page metadata instead of the boundary; the client-only
            // error boundaries and the remaining default-only files expose
            // only their component.
            "global-not-found" | "template" | "loading" | "error" | "global-error" => {
                names.push("default");
            }
            _ => {}
        }
    } else if is_pages {
        // Next.js reserves `/_app`, `/_error`, `/_document`, and `/api` only
        // at the pages root (`RESERVED_PAGE`), and `isAPIRoute` accepts both
        // `/api` and `/api/...`. A nested `_app` or `_error` is an ordinary
        // page, and `pages/api.ts` is the `/api` route, not a page.
        let pages_relative = package_path
            .strip_prefix("src/pages/")
            .or_else(|| package_path.strip_prefix("pages/"))
            .unwrap_or(package_path);
        let pages_base = next_convention_base(pages_relative, extensions).unwrap_or(pages_relative);
        if pages_base == "api" || pages_base.starts_with("api/") {
            names.extend(["default", "config", "runtime", "maxDuration"]);
        } else if pages_base == "_app" {
            names.extend([
                "default",
                "reportWebVitals",
                "config",
                "runtime",
                "maxDuration",
            ]);
        } else if matches!(pages_base, "_document" | "_error") {
            names.extend(["default", "config", "runtime", "maxDuration"]);
        } else {
            names.extend([
                "default",
                "getStaticProps",
                "getServerSideProps",
                "getStaticPaths",
                "config",
                "runtime",
                "maxDuration",
            ]);
        }
    } else if root_convention {
        match stem {
            "middleware" => names.extend(["default", "middleware", "config"]),
            "proxy" => names.extend(["default", "proxy", "config"]),
            "instrumentation" => names.extend(["register", "onRequestError"]),
            "instrumentation-client" => names.push("onRouterTransitionStart"),
            "mdx-components" => names.push("useMDXComponents"),
            _ => {}
        }
    }
    names
}

/// Packages that make Vite the project's bundler. `vitest` implies a Vite
/// toolchain, and `rolldown-vite` is the drop-in Vite distribution.
pub(crate) const VITE_ENABLERS: &[&str] = &["vite", "vitest", "rolldown-vite"];

impl Builder<'_> {
    pub(crate) fn package_depends_on(&self, directory: &str, name: &str) -> bool {
        self.package_depends_on_any(directory, &[name])
    }

    /// Whether the package manifest explicitly declares any of `names`. A
    /// missing or unreadable manifest returns `None`, so convention detection
    /// stays conservative for incomplete trees instead of hiding a possible
    /// entry. Manifests are parsed once when the builder is created.
    pub(crate) fn package_declares_any(&self, directory: &str, names: &[&str]) -> Option<bool> {
        let value = self.package_manifests.get(directory)?.as_ref()?;
        Some(
            [
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
            ]
            .iter()
            .any(|section| {
                let Some(dependencies) = value.get(section) else {
                    return false;
                };
                names.iter().any(|name| dependencies.get(name).is_some())
            }),
        )
    }

    /// Whether the owning package declares Next.js itself.
    pub(crate) fn package_declares_next(&self, directory: &str) -> Option<bool> {
        self.package_declares_any(directory, &["next"])
    }

    /// Storybook's default story glob includes every `.mdx` page, not just
    /// files named `*.stories.mdx`, and the plugin only applies when the
    /// package depends on Storybook. `@nrwl/storybook` is the Nx wrapper.
    pub(crate) fn package_depends_on_storybook(&self, directory: &str) -> bool {
        let Some(Some(value)) = self.package_manifests.get(directory) else {
            return false;
        };
        [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ]
        .iter()
        .filter_map(|section| value.get(section).and_then(|deps| deps.as_object()))
        .any(|dependencies| {
            dependencies.keys().any(|name| {
                name == "storybook" || name.starts_with("@storybook/") || name == "@nrwl/storybook"
            })
        })
    }

    pub(crate) fn package_depends_on_any(&self, directory: &str, names: &[&str]) -> bool {
        self.package_declares_any(directory, names) == Some(true)
    }
}

/// React roots that existed before `createRoot`: `ReactDOM.render` and
/// `ReactDOM.hydrate`. Only bindings imported from `react-dom` itself count, so
/// a testing-library `render(<App />)` call cannot make the application tree
/// live. Default and namespace imports both expose the legacy methods; named
/// `render`/`hydrate` imports are recognized too because React 17 supported
/// them directly.
pub(crate) fn react_dom_legacy_root(summary: &FileSummary) -> bool {
    let mut namespaces: BTreeSet<&str> = BTreeSet::new();
    let mut named_roots: BTreeSet<&str> = BTreeSet::new();
    for import in &summary.imports {
        if import.source != "react-dom" {
            continue;
        }
        for binding in &import.bindings {
            match binding.kind {
                BindingKind::Default | BindingKind::Namespace => {
                    namespaces.insert(binding.local.as_str());
                }
                BindingKind::Named if matches!(binding.imported.as_str(), "render" | "hydrate") => {
                    named_roots.insert(binding.local.as_str());
                }
                BindingKind::Named => {}
            }
        }
    }
    let named_call = !named_roots.is_empty()
        && summary
            .module_refs
            .identifiers
            .iter()
            .any(|identifier| named_roots.contains(identifier.as_str()));
    named_call
        || summary.module_refs.members.iter().any(|member| {
            namespaces.contains(member.object.as_str())
                && matches!(member.property.as_deref(), Some("render" | "hydrate"))
        })
}

/// Vite's conventional application roots. `index.html` names the real script,
/// but a project without the document (or with a template that the resolver
/// cannot place) still starts here.
pub(crate) fn vite_root_entry(relative: &str) -> bool {
    ["src/main", "src/index"].iter().any(|base| {
        ["ts", "tsx", "js", "jsx"]
            .iter()
            .any(|extension| relative == format!("{base}.{extension}"))
    })
}

/// Framework files that are executed by convention even when the TypeScript
/// parser does not understand their extension. These files are never deletion
/// candidates, but their module symbol is an entry.
pub(crate) fn framework_module(path: &str) -> Option<(&'static str, &'static str)> {
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

/// Normalize a framework directory setting (React Router `appDirectory` or
/// Expo Router `root`). Only a relative directory with no traversal is
/// accepted; anything else falls back to the framework default.
pub(crate) fn normalize_framework_directory(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('/') {
        return None;
    }
    let value = value
        .strip_prefix("./")
        .unwrap_or(value)
        .trim_end_matches('/');
    if value.is_empty()
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    Some(value.to_string())
}

/// Read the statically written `appDirectory` from a React Router config
/// source. React Router defaults to `app` when the option is absent.
pub(crate) fn react_router_app_directory(source: &str) -> Option<String> {
    let mut chars = source.chars().peekable();
    let mut identifier = String::new();
    while let Some(character) = chars.next() {
        match character {
            '/' if chars.peek() == Some(&'/') => {
                for character in chars.by_ref() {
                    if character == '\n' {
                        break;
                    }
                }
                identifier.clear();
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
                identifier.clear();
            }
            '\'' | '"' | '`' => {
                let quote = character;
                while let Some(character) = chars.next() {
                    if character == '\\' {
                        chars.next();
                    } else if character == quote {
                        break;
                    }
                }
                identifier.clear();
            }
            character if character.is_ascii_alphanumeric() || character == '_' => {
                identifier.push(character);
            }
            character if character.is_whitespace() => {}

            ':' if identifier == "appDirectory" => {
                while chars
                    .peek()
                    .is_some_and(|character| character.is_whitespace())
                {
                    chars.next();
                }
                let &quote = chars.peek()?;
                if !matches!(quote, '\'' | '"' | '`') {
                    return None;
                }
                chars.next();
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
                if interpolation {
                    return None;
                }
                return normalize_framework_directory(&value);
            }
            _ => identifier.clear(),
        }
    }
    None
}

/// Read the statically written `root` option of the `expo-router` config
/// plugin. Expo Router defaults to `app` when the option is absent.
pub(crate) fn expo_router_root(source: &str) -> Option<String> {
    let tokens = route_config_tokens(source);
    for (index, token) in tokens.iter().enumerate() {
        let RouteToken::Text(plugin) = token else {
            continue;
        };
        if plugin != "expo-router" {
            continue;
        }
        if !matches!(tokens.get(index + 1), Some(RouteToken::Punct(',')))
            || !matches!(tokens.get(index + 2), Some(RouteToken::Punct('{')))
        {
            continue;
        }
        let mut depth = 0usize;
        let mut cursor = index + 2;
        while let Some(token) = tokens.get(cursor) {
            match token {
                RouteToken::Punct('{') => depth += 1,
                RouteToken::Punct('}') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                RouteToken::Ident(key) | RouteToken::Text(key)
                    if depth == 1
                        && key == "root"
                        && matches!(tokens.get(cursor + 1), Some(RouteToken::Punct(':'))) =>
                {
                    if let Some(RouteToken::Text(value)) = tokens.get(cursor + 2) {
                        return normalize_framework_directory(value);
                    }
                    return None;
                }
                _ => {}
            }
            cursor += 1;
        }
    }
    None
}

/// Relative module paths named by Expo config plugins. Expo resolves local
/// config plugins (for example `plugins: ["./plugins/with-thing"]`) while
/// reading the app config, without importing them from application code. The
/// tokenizer ignores comments and dynamic values; scanning the whole `plugins`
/// value, including a local `const plugins = [...]`, catches aliases and
/// wrapper calls while staying limited to that property.
pub(crate) fn expo_config_plugin_paths(source: &str) -> Vec<String> {
    let tokens = route_config_tokens(source);
    let mut paths = BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        let key = match token {
            RouteToken::Ident(key) | RouteToken::Text(key) => key,
            _ => continue,
        };
        if key != "plugins" {
            continue;
        }
        // `plugins: [...]` (object property) or `const plugins = [...]`
        // (local binding that is later passed to Expo). Both values may be
        // wrapped in helper calls or spread into the final array.
        let start = match tokens.get(index + 1) {
            Some(RouteToken::Punct(':')) | Some(RouteToken::Punct('=')) => index + 2,
            _ => continue,
        };
        let mut depth = 0usize;
        let mut cursor = start;
        while let Some(token) = tokens.get(cursor) {
            match token {
                RouteToken::Punct('(') | RouteToken::Punct('[') | RouteToken::Punct('{') => {
                    depth += 1;
                }
                RouteToken::Punct(')') | RouteToken::Punct(']') | RouteToken::Punct('}') => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                RouteToken::Punct(',') | RouteToken::Punct(';') if depth == 0 => break,
                // Plugin names are array elements (or the first element of a
                // `[name, options]` tuple), optionally wrapped in a call such
                // as `require.resolve`. Skip object option values: strings
                // there are settings or asset paths, not loaded modules.
                RouteToken::Text(value)
                    if depth <= 2
                        && !matches!(
                            cursor.checked_sub(1).and_then(|at| tokens.get(at)),
                            Some(RouteToken::Punct(':'))
                        )
                        && (value.starts_with("./") || value.starts_with("../")) =>
                {
                    paths.insert(value.clone());
                }
                _ => {}
            }
            cursor += 1;
        }
    }
    paths.into_iter().collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteHelper {
    Index,
    Route,
    Layout,
}

pub(crate) fn route_helper(name: &str) -> Option<RouteHelper> {
    match name {
        "index" => Some(RouteHelper::Index),
        "route" => Some(RouteHelper::Route),
        "layout" => Some(RouteHelper::Layout),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RouteToken {
    Ident(String),
    Text(String),
    Punct(char),
}

/// Tokenize a React Router route config well enough to find statically written
/// `index`, `route`, `layout`, and `file:` targets. Comments, interpolation,
/// and dynamic values are ignored rather than guessed at.
pub(crate) fn route_config_tokens(source: &str) -> Vec<RouteToken> {
    let mut tokens = Vec::new();
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
                let mut dynamic = false;
                while let Some(character) = chars.next() {
                    if character == '\\' {
                        if let Some(escaped) = chars.next() {
                            value.push(escaped);
                        }
                    } else if character == quote {
                        break;
                    } else {
                        if quote == '`' && character == '$' && chars.peek() == Some(&'{') {
                            dynamic = true;
                        }
                        value.push(character);
                    }
                }
                if !dynamic {
                    tokens.push(RouteToken::Text(value));
                }
            }
            character if character.is_ascii_alphabetic() || matches!(character, '_' | '$') => {
                let mut identifier = String::from(character);
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || matches!(next, '_' | '$') {
                        identifier.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(RouteToken::Ident(identifier));
            }
            character if character.is_ascii_digit() => {
                while chars
                    .peek()
                    .is_some_and(|next| next.is_ascii_alphanumeric() || matches!(next, '.' | '_'))
                {
                    chars.next();
                }
            }
            character if character.is_whitespace() => {}
            character => tokens.push(RouteToken::Punct(character)),
        }
    }
    tokens
}

/// Local aliases for the route helpers imported from a React Router module.
pub(crate) fn route_helper_aliases(tokens: &[RouteToken]) -> BTreeMap<String, RouteHelper> {
    let mut aliases = BTreeMap::new();
    for (index, token) in tokens.iter().enumerate() {
        let RouteToken::Ident(keyword) = token else {
            continue;
        };
        if keyword != "import" {
            continue;
        }
        let mut cursor = index + 1;
        let mut source = None;
        while let Some(token) = tokens.get(cursor) {
            match token {
                RouteToken::Text(value) => {
                    source = Some(value.as_str());
                    break;
                }
                RouteToken::Punct(';') => break,
                _ => cursor += 1,
            }
        }
        let Some(source) = source else {
            continue;
        };
        if source != "react-router"
            && !source.starts_with("react-router/")
            && !source.starts_with("@react-router/")
        {
            continue;
        }
        let mut position = index + 1;
        while position < cursor {
            if let RouteToken::Ident(original) = &tokens[position] {
                if original == "as" {
                    if let Some(RouteToken::Ident(local)) = tokens.get(position + 1) {
                        if let Some(RouteToken::Ident(imported)) =
                            position.checked_sub(1).and_then(|at| tokens.get(at))
                        {
                            if let Some(helper) = route_helper(imported) {
                                aliases.insert(local.clone(), helper);
                            }
                        }
                    }
                }
            }
            position += 1;
        }
    }
    aliases
}

/// Arguments of one call, with an entry only when the argument is exactly one
/// static string literal.
pub(crate) fn route_call_arguments(
    tokens: &[RouteToken],
    open: usize,
) -> Option<Vec<Option<String>>> {
    let mut arguments = Vec::new();
    let mut current: Vec<&RouteToken> = Vec::new();
    let mut depth = 0usize;
    let mut index = open;
    while let Some(token) = tokens.get(index) {
        match token {
            RouteToken::Punct('(' | '[' | '{') => {
                depth += 1;
                if depth > 1 {
                    current.push(token);
                }
            }
            RouteToken::Punct(')' | ']' | '}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    arguments.push(exactly_one_string(&current));
                    return Some(arguments);
                }
                current.push(token);
            }
            RouteToken::Punct(',') if depth == 1 => {
                arguments.push(exactly_one_string(&current));
                current.clear();
            }
            _ => {
                if depth >= 1 {
                    current.push(token);
                }
            }
        }
        index += 1;
    }
    None
}

pub(crate) fn exactly_one_string(tokens: &[&RouteToken]) -> Option<String> {
    match tokens {
        [RouteToken::Text(value)] => Some(value.clone()),
        _ => None,
    }
}

/// Statically visible route module targets in a React Router route config.
pub(crate) fn react_router_route_targets(source: &str) -> BTreeSet<String> {
    let tokens = route_config_tokens(source);
    let aliases = route_helper_aliases(&tokens);
    let mut targets = BTreeSet::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        let RouteToken::Ident(identifier) = token else {
            index += 1;
            continue;
        };
        if identifier == "file" && matches!(tokens.get(index + 1), Some(RouteToken::Punct(':'))) {
            if let Some(RouteToken::Text(value)) = tokens.get(index + 2) {
                targets.insert(value.clone());
            }
        }
        let helper = route_helper(identifier).or_else(|| aliases.get(identifier).copied());
        if let Some(helper) = helper {
            if matches!(tokens.get(index + 1), Some(RouteToken::Punct('('))) {
                if let Some(arguments) = route_call_arguments(&tokens, index + 1) {
                    let position = usize::from(helper == RouteHelper::Route);
                    if let Some(Some(value)) = arguments.get(position) {
                        targets.insert(value.clone());
                    }
                }
            }
        }
        index += 1;
    }
    targets
}

/// Resolve a route config target against the package's app directory. Targets
/// that escape the repository or do not name an analyzed module are ignored.
pub(crate) fn resolve_route_target(
    package_directory: &str,
    app_directory: &str,
    target: &str,
    analyzed: &BTreeSet<&str>,
) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    let target = target.strip_prefix("./").unwrap_or(target);
    let target = target.trim_start_matches('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in package_directory
        .split('/')
        .chain(app_directory.split('/'))
        .chain(target.split('/'))
    {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return None;
    }
    let candidate = parts.join("/");
    if analyzed.contains(candidate.as_str()) {
        return Some(candidate);
    }
    if Path::new(&candidate).extension().is_none() {
        for extension in ["tsx", "ts", "jsx", "js", "mts", "mjs", "cts", "cjs"] {
            let with_extension = format!("{candidate}.{extension}");
            if analyzed.contains(with_extension.as_str()) {
                return Some(with_extension);
            }
        }
    }
    None
}
