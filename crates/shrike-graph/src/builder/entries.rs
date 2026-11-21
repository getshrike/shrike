use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use shrike_core::ids::module_symbol_key;
use shrike_core::{ClientBoundaryPolicy, EntryPolicy};
use shrike_syntax::{BindingKind, FileSummary, ParseStatus};

use super::Builder;
use crate::storybook_config;
use crate::types::EntryRecord;
use crate::util::route_of;
use crate::webpack_config::{self, EntryBase};

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

/// next-intl reads `{src/,}i18n/request.*` and `{src/,}i18n/routing.*` by
/// convention when its plugin is configured. Only the file names are fixed, so
/// the dependency check decides whether the convention applies.
fn is_next_intl_convention(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    matches!(stem, "request" | "routing")
        && matches!(extension, "ts" | "tsx" | "js" | "jsx")
        && at_package_root(path, &format!("i18n/{file}"))
}

/// Packages that make Vite the project's bundler. `vitest` implies a Vite
/// toolchain, and `rolldown-vite` is the drop-in Vite distribution.
const VITE_ENABLERS: &[&str] = &["vite", "vitest", "rolldown-vite"];

fn package_depends_on(root: &Path, directory: &str, name: &str) -> bool {
    package_depends_on_any(root, directory, &[name])
}

/// Storybook's default story glob includes every `.mdx` page, not just files
/// named `*.stories.mdx`, and the plugin only applies when the package depends
/// on Storybook. `@nrwl/storybook` is the Nx wrapper.
fn package_depends_on_storybook(root: &Path, directory: &str) -> bool {
    let Ok(text) = fs::read_to_string(root.join(directory).join("package.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
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

fn package_depends_on_any(root: &Path, directory: &str, names: &[&str]) -> bool {
    let Ok(text) = fs::read_to_string(root.join(directory).join("package.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
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
    })
}

/// React roots that existed before `createRoot`: `ReactDOM.render` and
/// `ReactDOM.hydrate`. Only bindings imported from `react-dom` itself count, so
/// a testing-library `render(<App />)` call cannot make the application tree
/// live. Default and namespace imports both expose the legacy methods; named
/// `render`/`hydrate` imports are recognized too because React 17 supported
/// them directly.
fn react_dom_legacy_root(summary: &FileSummary) -> bool {
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
fn vite_root_entry(relative: &str) -> bool {
    ["src/main", "src/index"].iter().any(|base| {
        ["ts", "tsx", "js", "jsx"]
            .iter()
            .any(|extension| relative == format!("{base}.{extension}"))
    })
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

/// Normalize a framework directory setting (React Router `appDirectory` or
/// Expo Router `root`). Only a relative directory with no traversal is
/// accepted; anything else falls back to the framework default.
fn normalize_framework_directory(value: &str) -> Option<String> {
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
fn react_router_app_directory(source: &str) -> Option<String> {
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
fn expo_router_root(source: &str) -> Option<String> {
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
fn expo_config_plugin_paths(source: &str) -> Vec<String> {
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
enum RouteHelper {
    Index,
    Route,
    Layout,
}

fn route_helper(name: &str) -> Option<RouteHelper> {
    match name {
        "index" => Some(RouteHelper::Index),
        "route" => Some(RouteHelper::Route),
        "layout" => Some(RouteHelper::Layout),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RouteToken {
    Ident(String),
    Text(String),
    Punct(char),
}

/// Tokenize a React Router route config well enough to find statically written
/// `index`, `route`, `layout`, and `file:` targets. Comments, interpolation,
/// and dynamic values are ignored rather than guessed at.
fn route_config_tokens(source: &str) -> Vec<RouteToken> {
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
fn route_helper_aliases(tokens: &[RouteToken]) -> BTreeMap<String, RouteHelper> {
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
fn route_call_arguments(tokens: &[RouteToken], open: usize) -> Option<Vec<Option<String>>> {
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

fn exactly_one_string(tokens: &[&RouteToken]) -> Option<String> {
    match tokens {
        [RouteToken::Text(value)] => Some(value.clone()),
        _ => None,
    }
}

/// Statically visible route module targets in a React Router route config.
fn react_router_route_targets(source: &str) -> BTreeSet<String> {
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
fn resolve_route_target(
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

impl<'a> Builder<'a> {
    pub(super) fn detect_entries(&mut self) {
        let summaries = self.summaries.clone();
        let analyzed: BTreeSet<&str> = summaries
            .iter()
            .map(|summary| summary.path.as_str())
            .collect();
        let mut vite_packages: BTreeSet<String> = BTreeSet::new();
        let mut storybook_packages: BTreeSet<String> = BTreeSet::new();
        for package in self.resolver.packages() {
            if package_depends_on_any(self.resolver.root(), &package.directory, VITE_ENABLERS) {
                vite_packages.insert(package.directory.clone());
            }
            if package_depends_on_storybook(self.resolver.root(), &package.directory) {
                storybook_packages.insert(package.directory.clone());
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
        for package in self.resolver.packages() {
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
                let Ok(source) = fs::read_to_string(self.resolver.root().join(&relative)) else {
                    continue;
                };
                // Expo resolves local config plugins when it loads the app
                // config. They are entries even when nothing imports them.
                for specifier in expo_config_plugin_paths(&source) {
                    if let Some(target) = self.resolver.resolve(&relative, &specifier).target {
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
                let app_directory = fs::read_to_string(self.resolver.root().join(&summary.path))
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
        // Webpack loads its configured entries and never imports them from
        // application code. A static read of `entry`/`context` keeps React and
        // vanilla bootstraps live without executing the config.
        let mut webpack_entry_modules: BTreeSet<String> = BTreeSet::new();
        for summary in &summaries {
            if !webpack_config::is_config_path(&summary.path) {
                continue;
            }
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                continue;
            };
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or("", |package| package.directory.as_str());
            for entry in webpack_config::entries(&summary.path, &source) {
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
                let resolved = self.resolver.resolve(&importer, &specifier);
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
        for summary in &summaries {
            if !storybook_config::is_config_path(&summary.path) {
                continue;
            }
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or("", |package| package.directory.as_str());
            if !storybook_packages.contains(package_directory) {
                continue;
            }
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                continue;
            };
            let patterns = storybook_config::story_patterns(&summary.path, &source);
            let Some(matcher) = storybook_config::matcher(&patterns) else {
                continue;
            };
            for path in &analyzed {
                if matcher.is_match(path) {
                    storybook_story_files.insert((*path).to_string());
                }
            }
        }
        // React Router route configs name route modules as strings. The router
        // imports them at runtime, so files named outside `<appDirectory>/routes/`
        // are still route modules and must keep their framework exports.
        let mut router_route_files: BTreeSet<String> = BTreeSet::new();
        for summary in &summaries {
            let package_directory = self
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
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
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
        for summary in &summaries {
            let path = summary.path.as_str();
            let package_directory = self
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
            let vite_package = vite_packages.contains(&package_directory);
            let storybook_package = storybook_packages.contains(&package_directory);
            // Vite turns the module scripts of `index.html` into production
            // entries. The document itself is loaded by the server, and every
            // export of the script it names is part of the page's surface.
            if vite_package && relative == "index.html" {
                add(self, module.clone(), "vite", "html-entry", None, "client");
                for import in &summary.imports {
                    let Some(target) = self.resolver.resolve(path, &import.source).target else {
                        continue;
                    };
                    if !analyzed.contains(target.as_str()) {
                        continue;
                    }
                    for (_, key) in self.exports.get(&target).cloned().unwrap_or_default() {
                        add(self, key, "vite", "html-entry-export", None, "client");
                    }
                    add(
                        self,
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
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "vite", "root-export", None, "both");
                }
                add(self, module.clone(), "vite", "root-entry", None, "both");
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
            if webpack_entry_modules.contains(path) {
                // A webpack entry is loaded by the bundler rather than imported.
                // Its exports are part of the entry surface, matching how the
                // other framework roots are treated.
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "webpack", "entry-export", None, "both");
                }
                add(self, module.clone(), "webpack", "entry", None, "both");
            }
            if expo_config_modules.contains(path) {
                // Expo resolves local config plugins without an import. Their
                // default export is the plugin function, and helper exports
                // are part of the same loaded module surface.
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(
                        self,
                        key,
                        "expo-config",
                        "config-reference-export",
                        None,
                        "both",
                    );
                }
                add(
                    self,
                    module.clone(),
                    "expo-config",
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
            // next-intl loads its request/routing modules by convention. Gate
            // the protection on the project actually depending on next-intl so
            // an unrelated `i18n/request.ts` stays analyzable.
            let package_directory = self
                .resolver
                .package_for(path)
                .map_or("", |package| package.directory.as_str());
            if is_next_intl_convention(path)
                && package_depends_on(self.resolver.root(), package_directory, "next-intl")
            {
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "next-intl", "convention-export", None, "both");
                }
                add(
                    self,
                    module.clone(),
                    "next-intl",
                    "convention",
                    None,
                    "both",
                );
            }
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
                // Next reads the per-page dynamic stale time from the page
                // module. Fallow's Next plugin names it on pages, not layouts.
                if stem == "page" {
                    public_names.push("unstable_dynamicStaleTime");
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
                // Next.js registers every export of a `"use server"` module in
                // the server-reference manifest, whether or not application
                // code imports it. Actions are invoked through serialized
                // references, so the import graph is not evidence that an
                // export is unused. This is independent of the client-boundary
                // policy, which only governs `"use client"` modules.
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(
                        self,
                        key,
                        "next-app",
                        "server-action-export",
                        None,
                        "server",
                    );
                }
                add(
                    self,
                    module.clone(),
                    "next-app",
                    "server-action",
                    None,
                    "server",
                );
                if self.config.client_boundary_policy == ClientBoundaryPolicy::Protect {
                    add(self, module.clone(), "react", "module", None, "both");
                }
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
                || path.contains("/.storybook/")
                || path.starts_with(".rnstorybook/")
                || path.contains("/.rnstorybook/")
                || storybook_story_files.contains(path)
                // Storybook's default glob includes every MDX page. Docs pages
                // are entries, and their ESM imports keep rendered components
                // live. Gate on the dependency so ordinary `.mdx` content
                // stays analyzable in projects without Storybook.
                || (storybook_package && path.ends_with(".mdx"));
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
            let react_root = summary
                .module_refs
                .identifiers
                .iter()
                .any(|id| id == "createRoot" || id == "hydrateRoot")
                || react_dom_legacy_root(summary);
            if react_root {
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

    use super::expo_config_plugin_paths;
    use super::expo_router_root;
    use crate::discovery::{analyze_paths, discover_files};
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

    /// Analyze only the files the production discovery walk finds. Use this
    /// when the feature under test is inventory, not entry classification.
    fn discovered_snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
        let directory = tempdir().expect("tempdir");
        for (path, content) in files {
            let full = directory.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parents");
            }
            fs::write(full, content).expect("write fixture");
        }
        let paths = discover_files(directory.path(), &EngineConfig::default());
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
    fn every_server_action_export_is_an_entry_by_default() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { importedAction } from '../src/actions';\n\
                 export default function Page() { return importedAction(); }\n",
            ),
            (
                "src/actions.ts",
                "\"use server\";\n\
                 export async function importedAction() { return 1; }\n\
                 export async function unimportedAction() { return 2; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for name in ["importedAction", "unimportedAction"] {
            let key = format!("src/actions.ts#{name}");
            assert!(entries.contains(&key.as_str()), "{name}: {entries:?}");
            assert!(
                snapshot.is_live(&key),
                "{name} must stay live: {:?}",
                snapshot.live
            );
            assert!(
                !snapshot.is_candidate(&key),
                "{name} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn unimported_client_boundary_exports_remain_candidates_by_default() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "export default function Page() { return null; }\n",
            ),
            (
                "src/client.ts",
                "\"use client\";\nexport function unusedClientExport() { return 1; }\n",
            ),
        ]);
        assert!(
            snapshot.is_candidate("src/client.ts#unusedClientExport"),
            "the client-boundary policy must still govern use client: {:?}",
            snapshot.candidates
        );
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
                "export const instant = { level: 'warning' };\nexport const unstable_dynamicStaleTime = 60;\nexport function unusedPageHelper() { return 1; }\nexport default function Page() { return null; }\n",
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
            entries.contains(&"app/page.tsx#unstable_dynamicStaleTime"),
            "page dynamic stale time must be public: {entries:?}"
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
    fn next_intl_conventions_require_the_dependency() {
        let with_dependency = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","next-intl":"^4.0.0"}}"#,
            ),
            (
                "src/i18n/request.ts",
                "export function getRequestConfig() { return 1; }\nexport function unusedRequestHelper() { return 2; }\n",
            ),
            (
                "i18n/routing.ts",
                "export const routing = { locales: ['en'] };\nexport const unusedRoutingHelper = 1;\n",
            ),
            ("components/dead.ts", "export const deadThing = 1;\n"),
        ]);
        let entries = entry_symbols(&with_dependency);
        for key in [
            "src/i18n/request.ts#getRequestConfig",
            "src/i18n/request.ts#unusedRequestHelper",
            "i18n/routing.ts#routing",
            "i18n/routing.ts#unusedRoutingHelper",
        ] {
            assert!(
                entries.contains(&key),
                "{key} must be an entry: {entries:?}"
            );
        }
        assert!(
            with_dependency
                .candidates
                .iter()
                .any(|key| key == "components/dead.ts#deadThing"),
            "unrelated dead code must remain a candidate: {:?}",
            with_dependency.candidates
        );

        let without_dependency = snapshot(&[
            ("package.json", r#"{"name":"app"}"#),
            (
                "i18n/request.ts",
                "export function unusedRequestHelper() { return 2; }\n",
            ),
        ]);
        assert!(
            without_dependency
                .candidates
                .iter()
                .any(|key| key == "i18n/request.ts#unusedRequestHelper"),
            "the convention must not apply without next-intl: {:?}",
            without_dependency.candidates
        );
    }

    #[test]
    fn vite_index_html_module_scripts_are_entries() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"vite-app","devDependencies":{"vite":"^6.0.0"}}"#,
            ),
            (
                "index.html",
                "<div id=\"app\"></div>\n<script type=\"module\" src=\"/src/main.ts\"></script>\n",
            ),
            (
                "src/main.ts",
                "import { mount } from \"./app\";\nexport const bootFlag = 1;\nmount();\n",
            ),
            (
                "src/app.ts",
                "export function mount() { return 1; }\nexport function unusedMount() { return 2; }\n",
            ),
            ("src/dead.ts", "export function DeadThing() { return 3; }\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "index.html#__module__",
            "src/main.ts#__module__",
            "src/main.ts#bootFlag",
            "src/app.ts#mount",
        ] {
            assert!(
                entries.contains(&key) || snapshot.is_live(key),
                "{key} must be live: {entries:?}"
            );
        }
        for key in ["src/app.ts#unusedMount", "src/dead.ts#DeadThing"] {
            assert!(
                snapshot.candidates.iter().any(|candidate| candidate == key),
                "{key} must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn vite_root_fallbacks_require_the_dependency() {
        let files: &[(&str, &str)] = &[
            (
                "src/main.ts",
                "import { helper } from \"./helpers\";\nexport const rootBoot = 1;\nhelper();\n",
            ),
            ("src/index.ts", "export const indexBoot = 1;\n"),
            ("src/helpers.ts", "export function helper() { return 1; }\n"),
            ("src/dead.ts", "export function DeadThing() { return 2; }\n"),
        ];
        let mut with_vite: Vec<(&str, &str)> = vec![(
            "package.json",
            r#"{"name":"vite-app","devDependencies":{"vite":"^6.0.0"}}"#,
        )];
        with_vite.extend_from_slice(files);
        let with_vite_snapshot = snapshot(&with_vite);
        let entries = entry_symbols(&with_vite_snapshot);
        for key in [
            "src/main.ts#__module__",
            "src/main.ts#rootBoot",
            "src/index.ts#__module__",
            "src/index.ts#indexBoot",
            "src/helpers.ts#helper",
        ] {
            assert!(
                entries.contains(&key) || with_vite_snapshot.is_live(key),
                "{key} must be live: {entries:?}"
            );
        }
        assert!(
            with_vite_snapshot
                .candidates
                .iter()
                .any(|candidate| candidate == "src/dead.ts#DeadThing"),
            "unrelated dead code must remain a candidate: {:?}",
            with_vite_snapshot.candidates
        );

        let mut without_vite: Vec<(&str, &str)> = vec![("package.json", r#"{"name":"plain-app"}"#)];
        without_vite.extend_from_slice(files);
        let without_vite_snapshot = snapshot(&without_vite);
        assert!(
            without_vite_snapshot
                .candidates
                .iter()
                .any(|candidate| candidate == "src/main.ts#rootBoot"),
            "the root fallback must not apply without Vite: {:?}",
            without_vite_snapshot.candidates
        );
    }

    fn legacy_react_app(bootstrap: &str) -> GraphSnapshot {
        snapshot(&[
            (
                "package.json",
                r#"{"name":"legacy-react-app","dependencies":{"react":"^17.0.2","react-dom":"^17.0.2"}}"#,
            ),
            ("src/client.js", bootstrap),
            (
                "src/App.js",
                "export default function App({ label }) { return <div>{label}</div>; }\nexport function unusedAppHelper() { return 1; }\n",
            ),
            (
                "src/format.js",
                "export function formatting() { return \"ready\"; }\nexport function unusedFormatter() { return 2; }\n",
            ),
            ("src/dead.js", "export function DeadThing() { return 3; }\n"),
        ])
    }

    fn assert_legacy_react_app_is_live(snapshot: &GraphSnapshot) {
        let entries = entry_symbols(snapshot);
        assert!(
            entries.contains(&"src/client.js#__module__"),
            "the render call must make the bootstrap a root: {entries:?}"
        );
        for key in ["src/App.js#default", "src/format.js#formatting"] {
            assert!(
                entries.contains(&key) || snapshot.is_live(key),
                "{key} is rendered by the root and must be live: {entries:?}"
            );
        }
        for key in [
            "src/App.js#unusedAppHelper",
            "src/format.js#unusedFormatter",
            "src/dead.js#DeadThing",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    fn storybook_mdx_app(package_json: &str) -> GraphSnapshot {
        snapshot(&[
            ("package.json", package_json),
            (
                ".storybook/main.ts",
                "export default { stories: [\"../src/**/*.mdx\"] };\n",
            ),
            (
                "src/DocsCard.tsx",
                "export function DocsCard() { return <div>docs</div>; }\nexport function unusedDocsHelper() { return 1; }\n",
            ),
            (
                "src/Docs.mdx",
                "import { DocsCard } from \"./DocsCard\";\n\n# Documentation\n\n<DocsCard />\n",
            ),
            (
                "src/dead.tsx",
                "export function DeadThing() { return 2; }\n",
            ),
        ])
    }

    #[test]
    fn storybook_mdx_documents_are_entries() {
        let snapshot =
            storybook_mdx_app(r#"{"name":"docs-app","devDependencies":{"storybook":"^8.0.0"}}"#);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"src/Docs.mdx#__module__"),
            "Storybook's default glob makes every MDX page an entry: {entries:?}"
        );
        assert!(
            snapshot.is_live("src/DocsCard.tsx#DocsCard"),
            "the component rendered by the docs page must be live: {:?}",
            snapshot.live
        );
        for key in [
            "src/DocsCard.tsx#unusedDocsHelper",
            "src/dead.tsx#DeadThing",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn storybook_mdx_entries_require_the_dependency() {
        let snapshot =
            storybook_mdx_app(r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#);
        for key in [
            "src/DocsCard.tsx#DocsCard",
            "src/DocsCard.tsx#unusedDocsHelper",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "without Storybook the MDX page must not protect its imports: {:?}",
                snapshot.candidates
            );
        }
    }

    fn custom_story_app(package_json: &str) -> GraphSnapshot {
        snapshot(&[
            ("package.json", package_json),
            (
                ".storybook/main.ts",
                r#"const config = {
  stories: [
    "../src/**/*.demo.tsx",
    { directory: "../src", files: "**/*.preview.tsx" },
  ],
  framework: "@storybook/react-vite",
};

export default config;
"#,
            ),
            (
                "src/Button.demo.tsx",
                "import { Button } from \"./button\";\n\nexport default function ButtonDemo() {\n  return <Button />;\n}\n\nexport function unusedDemoExport() {\n  return 1;\n}\n",
            ),
            (
                "src/button.tsx",
                "export function Button() {\n  return <button />;\n}\n\nexport function unusedButtonHelper() {\n  return 2;\n}\n",
            ),
            (
                "src/Card.preview.tsx",
                "import { Card } from \"./card\";\n\nexport default function CardPreview() {\n  return <Card />;\n}\n",
            ),
            (
                "src/card.tsx",
                "export function Card() {\n  return <div />;\n}\n",
            ),
            (
                "src/dead.tsx",
                "export function Dead() {\n  return null;\n}\n",
            ),
        ])
    }

    #[test]
    fn storybook_custom_story_globs_are_entries() {
        let snapshot = custom_story_app(
            r#"{"name":"custom-stories","devDependencies":{"storybook":"^8.0.0"}}"#,
        );
        let entries = entry_symbols(&snapshot);
        for key in [
            ".storybook/main.ts#__module__",
            "src/Button.demo.tsx#__module__",
            "src/Card.preview.tsx#__module__",
        ] {
            assert!(
                entries.contains(&key),
                "{key} is named by the Storybook config: {entries:?}"
            );
        }
        for key in [
            "src/Button.demo.tsx#default",
            "src/Button.demo.tsx#unusedDemoExport",
            "src/button.tsx#Button",
            "src/card.tsx#Card",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} is part of a story entry surface: {:?}",
                snapshot.live
            );
        }
        for key in ["src/button.tsx#unusedButtonHelper", "src/dead.tsx#Dead"] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn storybook_custom_story_globs_require_the_dependency() {
        let snapshot =
            custom_story_app(r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#);
        for key in [
            "src/Button.demo.tsx#default",
            "src/button.tsx#Button",
            "src/Card.preview.tsx#default",
            "src/card.tsx#Card",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "without Storybook the custom glob must not protect {key}: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn react_dom_render_is_a_legacy_root() {
        assert_legacy_react_app_is_live(&legacy_react_app(
            "import React from \"react\";\nimport ReactDOM from \"react-dom\";\nimport App from \"./App\";\nimport { formatting } from \"./format\";\n\nReactDOM.render(<App label={formatting()} />, document.getElementById(\"root\"));\n",
        ));
    }

    #[test]
    fn react_dom_namespace_and_named_roots_are_recognized() {
        assert_legacy_react_app_is_live(&legacy_react_app(
            "import * as ReactDOM from \"react-dom\";\nimport App from \"./App\";\nimport { formatting } from \"./format\";\n\nReactDOM.hydrate(<App label={formatting()} />, document.getElementById(\"root\"));\n",
        ));
        assert_legacy_react_app_is_live(&legacy_react_app(
            "import { render } from \"react-dom\";\nimport App from \"./App\";\nimport { formatting } from \"./format\";\n\nrender(<App label={formatting()} />, document.getElementById(\"root\"));\n",
        ));
    }

    #[test]
    fn testing_library_render_is_not_a_react_root() {
        let snapshot = legacy_react_app(
            "import { render } from \"@testing-library/react\";\nimport App from \"./App\";\n\nrender(<App label=\"test\" />);\n",
        );
        assert!(
            snapshot.is_candidate("src/App.js#default"),
            "testing-library render must not make the application tree live: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn worker_modules_are_reachable_without_an_import() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"worker-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { WorkerHost } from \"../components/worker-host\";\n\nexport default function Page() {\n  return <WorkerHost />;\n}\n",
            ),
            (
                "components/worker-host.tsx",
                "export function WorkerHost() {\n  const worker = new Worker(new URL(\"./heavy-worker.ts\", import.meta.url), { type: \"module\" });\n  return <div>{worker ? \"ready\" : \"idle\"}</div>;\n}\n",
            ),
            (
                "components/heavy-worker.ts",
                "import { render } from \"./renderer\";\n\nconst state = { count: 0 };\n\nfunction handle(message: string) {\n  state.count += 1;\n  return render(message);\n}\n\nself.onmessage = (event) => handle(event.data);\n\nfunction deadWorkerHelper() {\n  return 1;\n}\n",
            ),
            (
                "components/renderer.ts",
                "export function render(value: string) {\n  return value;\n}\n\nexport function deadRenderer() {\n  return 2;\n}\n",
            ),
            (
                "components/dead.ts",
                "export function Dead() {\n  return null;\n}\n",
            ),
        ]);
        for key in [
            "components/worker-host.tsx#WorkerHost",
            "components/heavy-worker.ts#handle",
            "components/heavy-worker.ts#state",
            "components/renderer.ts#render",
        ] {
            assert!(
                snapshot.is_live(key),
                "worker-reachable symbol must be live: {key}: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "components/heavy-worker.ts#deadWorkerHelper",
            "components/renderer.ts#deadRenderer",
            "components/dead.ts#Dead",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "unreferenced symbol must remain a candidate: {key}: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot.edges.iter().any(|edge| edge.kind == "worker"
                && edge.target == "components/heavy-worker.ts#__module__"),
            "worker edge must be recorded: {:?}",
            snapshot.edges
        );
    }

    #[test]
    fn public_worker_urls_do_not_widen_the_repository() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"worker-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { WorkerHost } from \"../components/worker-host\";\n\nexport default function Page() {\n  return <WorkerHost />;\n}\n",
            ),
            (
                "components/worker-host.tsx",
                "export function WorkerHost() {\n  void navigator.serviceWorker.register(\"/sw.js\", { scope: \"/\" });\n  return null;\n}\n",
            ),
            (
                "components/dead.ts",
                "export function Dead() {\n  return null;\n}\n",
            ),
        ]);
        assert!(
            snapshot.is_live("components/worker-host.tsx#WorkerHost"),
            "route-reachable component must stay live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("components/dead.ts#Dead"),
            "a public worker URL must not make unrelated symbols live: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot
                .unknowns
                .iter()
                .any(|unknown| unknown.scope == "repo"),
            "public worker URL must not create a repository-wide unknown: {:?}",
            snapshot.unknowns
        );
        assert!(
            snapshot.modules.iter().all(|module| !module
                .external
                .iter()
                .any(|specifier| specifier == "/sw.js")),
            "a runtime worker URL is not an external package: {:?}",
            snapshot.modules
        );
    }

    #[test]
    fn expo_router_root_is_read_from_app_config() {
        assert_eq!(
            expo_router_root(r#"{"expo":{"plugins":[["expo-router",{"root":"./src/routes"}]]}}"#)
                .as_deref(),
            Some("src/routes")
        );
        assert_eq!(
            expo_router_root(
                "export default { expo: { plugins: [['expo-router', { root: 'src/features' }]] } };"
            )
            .as_deref(),
            Some("src/features")
        );
        assert_eq!(
            expo_router_root(
                "export default { expo: { plugins: [['expo-router', { root: someValue }]] } };"
            ),
            None
        );
        assert_eq!(
            expo_router_root(r#"{"expo":{"plugins":["expo-router"]}}"#),
            None
        );
    }

    #[test]
    fn expo_config_plugin_paths_are_static_only() {
        assert_eq!(
            expo_config_plugin_paths(
                r#"{"expo":{"plugins":["./plugins/with-thing",["expo-router",{"root":"./app"}]]}}"#
            ),
            vec!["./plugins/with-thing"]
        );
        assert_eq!(
            expo_config_plugin_paths(
                "const plugins = ['./plugins/a', ['expo-router', { root: './app' }]];\nexport default { plugins };\n"
            ),
            vec!["./plugins/a"]
        );
        assert_eq!(
            expo_config_plugin_paths(
                "export default { plugins: [require.resolve('./plugins/b')] };\n"
            ),
            vec!["./plugins/b"]
        );
        assert_eq!(
            expo_config_plugin_paths(
                "// plugins: ['./commented']\nexport default { name: 'x' };\n"
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            expo_config_plugin_paths(
                "export default { name: './plugins/not-a-plugin', plugins: [someValue] };\n"
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn expo_router_custom_root_routes_are_entries() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"expo-app","main":"expo-router/entry","dependencies":{"expo":"^54.0.0","expo-router":"^6.0.0"}}"#,
            ),
            (
                "app.config.ts",
                "export default { expo: { plugins: [['expo-router', { root: './src/routes' }]] } };\n",
            ),
            (
                "src/routes/_layout.tsx",
                "import { Stack } from 'expo-router';\nexport const unstable_settings = { initialRouteName: 'index' };\nexport default function Layout() { return <Stack />; }\nexport function unusedLayoutHelper() { return 1; }\n",
            ),
            (
                "src/routes/index.tsx",
                "export default function Home() { return null; }\nexport function unusedHomeHelper() { return 1; }\n",
            ),
            (
                "src/routes/user/[id].tsx",
                "export default function User() { return null; }\nexport function unusedUserHelper() { return 1; }\n",
            ),
            (
                "app/dead.tsx",
                "export function deadOutsideRoot() { return 1; }\n",
            ),
            (
                "src/components/dead.tsx",
                "export function deadThing() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "src/routes/_layout.tsx#default",
            "src/routes/_layout.tsx#unstable_settings",
            "src/routes/index.tsx#default",
            "src/routes/user/[id].tsx#default",
        ] {
            assert!(
                entries.contains(&key),
                "custom Expo root entry {key} must be public: {entries:?}"
            );
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "custom Expo root entry {key} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "src/routes/_layout.tsx#unusedLayoutHelper",
            "src/routes/index.tsx#unusedHomeHelper",
            "src/routes/user/[id].tsx#unusedUserHelper",
            "app/dead.tsx#deadOutsideRoot",
            "src/components/dead.tsx#deadThing",
        ] {
            assert!(
                snapshot.candidates.iter().any(|candidate| candidate == key),
                "{key} must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn expo_config_plugins_are_entries() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"expo-plugin-app","main":"expo-router/entry","dependencies":{"expo":"^54.0.0","expo-router":"^6.0.0","react":"^19.0.0","react-native":"^0.81.0"}}"#,
            ),
            (
                "app.json",
                r#"{"expo":{"plugins":["./plugins/with-thing",["expo-router",{"root":"./app"}]]}}"#,
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
                "plugins/with-thing.ts",
                "export default function withThing(config: object) { return config; }\nexport function helper() { return true; }\nexport function unusedPluginExport() { return 1; }\n",
            ),
            ("src/dead.ts", "export function deadThing() { return 1; }\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "plugins/with-thing.ts#__module__",
            "plugins/with-thing.ts#default",
            "plugins/with-thing.ts#helper",
            "plugins/with-thing.ts#unusedPluginExport",
        ] {
            assert!(
                entries.contains(&key),
                "local Expo config plugin surface {key} must be an entry: {entries:?}"
            );
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "local Expo config plugin surface {key} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|candidate| candidate == "src/dead.ts#deadThing"),
            "an unrelated module must remain a candidate: {:?}",
            snapshot.candidates
        );
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

    fn next_page_extensions_fixture(page_extensions: bool) -> Vec<(&'static str, &'static str)> {
        let config = if page_extensions {
            "const nextConfig = { pageExtensions: ['js', 'jsx', 'ts', 'tsx', 'md', 'mdx'] };\nexport default nextConfig;\n"
        } else {
            "const nextConfig = { reactStrictMode: true };\nexport default nextConfig;\n"
        };
        vec![
            (
                "package.json",
                r#"{"name":"docs-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            ("next.config.ts", config),
            (
                "app/page.md",
                "import { Header } from \"../components/header\";\n\n# Home\n\n<Header />\n",
            ),
            (
                "components/header.tsx",
                "export function Header() { return <h1>hi</h1>; }\nexport function unusedHeaderHelper() { return 1; }\n",
            ),
            (
                "pages/about.md",
                "import { Footer } from \"../components/footer\";\n\n# About\n\n<Footer />\n",
            ),
            (
                "components/footer.tsx",
                "export function Footer() { return <footer />; }\n",
            ),
            (
                "app/blog/post.md",
                "import { DeadCard } from \"../../components/dead-card\";\n\n# Post\n\n<DeadCard />\n",
            ),
            (
                "components/dead-card.tsx",
                "export function DeadCard() { return <div />; }\n",
            ),
        ]
    }

    #[test]
    fn next_page_extensions_make_markdown_routes_live() {
        let snapshot = discovered_snapshot(&next_page_extensions_fixture(true));
        let files: Vec<&str> = snapshot
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect();
        for path in ["app/page.md", "pages/about.md"] {
            assert!(
                files.contains(&path),
                "{path} must be inventoried: {files:?}"
            );
        }
        assert!(
            !files.contains(&"app/blog/post.md"),
            "only App Router convention stems become routes: {files:?}"
        );
        let entries = entry_symbols(&snapshot);
        for key in ["app/page.md#__module__", "pages/about.md#__module__"] {
            assert!(
                entries.contains(&key),
                "{key} must be a route entry: {entries:?}"
            );
        }
        for key in [
            "components/header.tsx#Header",
            "components/footer.tsx#Footer",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} is rendered by a Markdown route and must be live: {:?}",
                snapshot.live
            );
        }
        for key in [
            "components/header.tsx#unusedHeaderHelper",
            "components/dead-card.tsx#DeadCard",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn markdown_routes_require_page_extensions() {
        let snapshot = discovered_snapshot(&next_page_extensions_fixture(false));
        let files: Vec<&str> = snapshot
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect();
        assert!(
            !files.iter().any(|path| path.ends_with(".md")),
            "no Markdown may be inventoried without pageExtensions: {files:?}"
        );
        for key in [
            "components/header.tsx#Header",
            "components/footer.tsx#Footer",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} must not be protected without a Markdown route: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn webpack_entries_are_roots() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"webpack-app","devDependencies":{"webpack":"^5.90.0","webpack-cli":"^5.1.0"}}"#,
            ),
            (
                "webpack.config.js",
                "const path = require(\"path\");\n\nmodule.exports = {\n  context: path.resolve(__dirname, \"src\"),\n  entry: { app: [\"./boot.js\"], admin: { import: \"./admin.js\" } },\n};\n",
            ),
            (
                "src/boot.js",
                "import { start } from \"./runtime\";\nimport { mountHeader } from \"./header\";\n\nstart();\nmountHeader();\n",
            ),
            (
                "src/admin.js",
                "import { renderAdmin } from \"./admin-panel\";\n\nrenderAdmin();\n",
            ),
            (
                "src/runtime.js",
                "export function start() { return \"started\"; }\nexport function unusedRuntime() { return \"dead\"; }\n",
            ),
            (
                "src/header.js",
                "export function mountHeader() { return \"header\"; }\n",
            ),
            (
                "src/admin-panel.js",
                "export function renderAdmin() { return \"admin\"; }\nexport function unusedAdminPanel() { return \"dead\"; }\n",
            ),
            (
                "src/dead.js",
                "export function DeadThing() { return \"dead\"; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in ["src/boot.js#__module__", "src/admin.js#__module__"] {
            assert!(
                entries.contains(&key),
                "{key} is named by the webpack entry object: {entries:?}"
            );
        }
        for key in [
            "src/runtime.js#start",
            "src/header.js#mountHeader",
            "src/admin-panel.js#renderAdmin",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} is imported by a webpack entry and must be live: {:?}",
                snapshot.live
            );
        }
        for key in [
            "src/runtime.js#unusedRuntime",
            "src/admin-panel.js#unusedAdminPanel",
            "src/dead.js#DeadThing",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn webpack_literal_context_is_package_relative() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"webpack-config-dir","devDependencies":{"webpack":"^5.90.0"}}"#,
            ),
            (
                "config/webpack.client.js",
                "module.exports = {\n  context: \"src\",\n  entry: \"./boot.js\",\n};\n",
            ),
            (
                "src/boot.js",
                "import { start } from \"./runtime\";\n\nstart();\n",
            ),
            (
                "src/runtime.js",
                "export function start() { return \"started\"; }\nexport function unusedRuntime() { return \"dead\"; }\n",
            ),
        ]);
        assert!(
            snapshot.is_live("src/boot.js#__module__"),
            "a literal context is resolved from the package root: {:?}",
            snapshot.live
        );
        assert!(
            snapshot.is_live("src/runtime.js#start"),
            "the config-directory entry must keep its imports live: {:?}",
            snapshot.live
        );
        assert!(
            snapshot.is_candidate("src/runtime.js#unusedRuntime"),
            "unused exports must still be reported: {:?}",
            snapshot.candidates
        );
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
    fn react_router_custom_app_directory_is_detected() {
        let snapshot = snapshot(&[
            (
                "react-router.config.ts",
                "import type { Config } from '@react-router/dev/config';\nexport default { appDirectory: 'src/frontend' } satisfies Config;\n",
            ),
            (
                "src/frontend/root.tsx",
                "export function Layout() { return null; }\nexport default function Root() { return null; }\nexport function unusedRootHelper() { return 1; }\n",
            ),
            ("src/frontend/routes.ts", "export default [];\n"),
            (
                "src/frontend/routes/home.tsx",
                "export function loader() { return null; }\nexport default function Home() { return null; }\nexport function unusedRouteHelper() { return 1; }\n",
            ),
            (
                "src/components/dead.tsx",
                "export function deadThing() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "src/frontend/root.tsx#default",
            "src/frontend/root.tsx#Layout",
            "src/frontend/routes/home.tsx#default",
            "src/frontend/routes/home.tsx#loader",
        ] {
            assert!(
                entries.contains(&key),
                "custom appDirectory entry {key} must be public: {entries:?}"
            );
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "custom appDirectory entry {key} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "src/frontend/root.tsx#unusedRootHelper",
            "src/frontend/routes/home.tsx#unusedRouteHelper",
            "src/components/dead.tsx#deadThing",
        ] {
            assert!(
                snapshot.candidates.iter().any(|candidate| candidate == key),
                "{key} must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn react_router_route_config_targets_outside_routes_are_entries() {
        let snapshot = snapshot(&[
            (
                "react-router.config.ts",
                "export default { appDirectory: 'src/frontend' };\n",
            ),
            (
                "src/frontend/routes.ts",
                "import { index, layout, route as r } from '@react-router/dev/routes';\n\
                 // route('ignored', 'route-modules/comment-only.tsx');\n\
                 export default [\n\
                   index('route-modules/home.tsx'),\n\
                   r('about', './route-modules/about.tsx'),\n\
                   layout('shell.tsx', [r('login', 'route-modules/login.tsx')]),\n\
                   { path: 'contact', file: 'route-modules/contact.tsx' },\n\
                 ];\n",
            ),
            (
                "src/frontend/root.tsx",
                "export default function Root() { return null; }\n",
            ),
            (
                "src/frontend/route-modules/home.tsx",
                "export default function Home() { return null; }\nexport function loader() { return null; }\nexport function unusedHomeHelper() { return 1; }\n",
            ),
            (
                "src/frontend/route-modules/about.tsx",
                "export default function About() { return null; }\nexport function loader() { return null; }\nexport function unusedAboutHelper() { return 1; }\n",
            ),
            (
                "src/frontend/route-modules/login.tsx",
                "export default function Login() { return null; }\n",
            ),
            (
                "src/frontend/route-modules/contact.tsx",
                "export default function Contact() { return null; }\n",
            ),
            (
                "src/frontend/shell.tsx",
                "export default function Shell() { return null; }\n",
            ),
            (
                "src/frontend/route-modules/comment-only.tsx",
                "export default function CommentOnly() { return null; }\n",
            ),
            (
                "src/frontend/route-modules/unreferenced.tsx",
                "export default function Unreferenced() { return null; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "src/frontend/route-modules/home.tsx#default",
            "src/frontend/route-modules/home.tsx#loader",
            "src/frontend/route-modules/about.tsx#default",
            "src/frontend/route-modules/about.tsx#loader",
            "src/frontend/route-modules/login.tsx#default",
            "src/frontend/route-modules/contact.tsx#default",
            "src/frontend/shell.tsx#default",
        ] {
            assert!(
                entries.contains(&key),
                "route config target {key} must be public: {entries:?}"
            );
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "route config target {key} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "src/frontend/route-modules/home.tsx#unusedHomeHelper",
            "src/frontend/route-modules/about.tsx#unusedAboutHelper",
            "src/frontend/route-modules/comment-only.tsx#default",
            "src/frontend/route-modules/unreferenced.tsx#default",
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
    fn promise_all_destructuring_keeps_only_named_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const [{ used }, other] = await Promise.all([import('./mod'), import('./other')]);\n  return used + (other ? 1 : 0);\n}\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
            (
                "src/other.ts",
                "export const any = 1;\nexport const alsoAny = 2;\n",
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
        for key in ["src/other.ts#any", "src/other.ts#alsoAny"] {
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "a namespace binding can be used in unknown ways: {key} {:?}",
                snapshot.candidates
            );
        }
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
    fn then_callback_keeps_only_accessed_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const used = await import('./mod').then((module) => module.used);\n  const escaped = await import('./escaped').then((module) => use(module));\n  return used + (escaped ? 1 : 0);\n}\nfunction use(value: unknown) { return value ? 1 : 0; }\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
            (
                "src/escaped.ts",
                "export const any = 1;\nexport const alsoAny = 2;\n",
            ),
        ]);
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#used"),
            "the member read by the .then callback must stay live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#dead"),
            "an export the callback does not read must be a candidate: {:?}",
            snapshot.candidates
        );
        for key in ["src/escaped.ts#any", "src/escaped.ts#alsoAny"] {
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "a namespace passed as a value can be used in unknown ways: {key} {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn const_namespace_dynamic_import_keeps_only_accessed_exports_live() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
            ),
            (
                "src/loader.ts",
                "export async function load() {\n  const used = await import('./mod');\n  const escaped = await import('./escaped');\n  return used.used + (escaped ? 1 : 0);\n}\nfunction use(value: unknown) { return value ? 1 : 0; }\n",
            ),
            (
                "src/mod.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
            (
                "src/escaped.ts",
                "export const any = 1;\nexport const alsoAny = 2;\n",
            ),
        ]);
        assert!(
            !snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#used"),
            "the member read through the const namespace must stay live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .candidates
                .iter()
                .any(|key| key == "src/mod.ts#dead"),
            "an export the const namespace does not read must be a candidate: {:?}",
            snapshot.candidates
        );
        for key in ["src/escaped.ts#any", "src/escaped.ts#alsoAny"] {
            assert!(
                !snapshot.candidates.iter().any(|candidate| candidate == key),
                "a namespace used as a value can be used in unknown ways: {key} {:?}",
                snapshot.candidates
            );
        }
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
                "export async function load() {\n  const mod = await import('./mod');\n  return mod;\n}\n",
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
    fn imports_from_dead_modules_do_not_make_exports_external() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"internal-dead-import","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { bootstrap } from '../src/registry';\nexport default function Page() { bootstrap(); return null; }\n",
            ),
            (
                "src/registry.ts",
                "export function helper() { return 1; }\nexport function bootstrap() { return helper(); }\n",
            ),
            (
                "src/dead.ts",
                "import { helper } from './registry';\nexport function deadConsumer() { return helper(); }\n",
            ),
            ("src/barrel.ts", "export { helper } from './registry';\n"),
        ]);
        assert!(
            snapshot.is_candidate("src/dead.ts#deadConsumer"),
            "dead importer must remain a candidate: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .internal_only
                .iter()
                .any(|key| key == "src/registry.ts#helper"),
            "a use from a dead module must not count as external visibility: {:?}",
            snapshot.internal_only
        );
        assert!(
            !snapshot.is_candidate("src/registry.ts#helper"),
            "the helper is still used by live bootstrap code: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/barrel.ts#helper"),
            "the dead alias to the live helper is actionable: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn live_external_imports_keep_exports_out_of_internal_only() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"internal-live-import","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { helper } from '../src/registry';\nexport default function Page() { return helper(); }\n",
            ),
            (
                "src/registry.ts",
                "export function helper() { return 1; }\n",
            ),
        ]);
        assert!(
            snapshot.is_live("src/registry.ts#helper"),
            "a live import must keep the export live"
        );
        assert!(
            !snapshot
                .internal_only
                .iter()
                .any(|key| key == "src/registry.ts#helper"),
            "a live external import is external visibility: {:?}",
            snapshot.internal_only
        );
        assert!(!snapshot.is_candidate("src/registry.ts#helper"));
    }

    #[test]
    fn unused_barrel_aliases_are_candidates_only_when_their_target_is_live() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"barrel-app","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { used } from \"../src/impl\";\n\nexport default function Page() {\n  return used;\n}\n",
            ),
            ("src/barrel.ts", "export { used, dead } from \"./impl\";\n"),
            (
                "src/impl.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
        ]);
        assert!(
            snapshot.is_live("src/impl.ts#used"),
            "directly imported declaration must be live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/barrel.ts#used"),
            "unused barrel alias with a live target is actionable: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/impl.ts#dead"),
            "dead declaration must be reported: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot.is_candidate("src/barrel.ts#dead"),
            "a dead target is reported at its declaration, not through the alias: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn unused_barrel_alias_chains_report_every_link_to_a_live_target() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"barrel-chain","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { used } from \"../src/impl\";\n\nexport default function Page() {\n  return used;\n}\n",
            ),
            ("src/barrel.ts", "export { used, dead } from \"./impl\";\n"),
            ("src/outer.ts", "export { used, dead } from \"./barrel\";\n"),
            (
                "src/impl.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
        ]);
        assert!(
            snapshot.is_candidate("src/barrel.ts#used"),
            "inner alias with a live target is actionable: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/outer.ts#used"),
            "outer alias whose chain reaches a live target is actionable: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot.is_candidate("src/barrel.ts#dead"),
            "dead alias is reported at its declaration, not through the alias: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot.is_candidate("src/outer.ts#dead"),
            "dead outer alias is reported at its declaration, not through the alias: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/impl.ts#dead"),
            "dead declaration must be reported: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn local_exports_of_imported_bindings_are_alias_candidates() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"local-alias","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { used } from \"../src/impl\";\n\nexport default function Page() {\n  return used;\n}\n",
            ),
            (
                "src/barrel.ts",
                "import { used as localUsed, dead } from \"./impl\";\nexport { localUsed as used, dead };\n",
            ),
            (
                "src/impl.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
        ]);
        assert!(
            snapshot.is_live("src/impl.ts#used"),
            "directly imported declaration must be live: {:?}",
            snapshot.candidates
        );
        let alias = snapshot
            .symbols
            .iter()
            .find(|symbol| symbol.key == "src/barrel.ts#used")
            .expect("local export of an import must create an alias symbol");
        assert_eq!(alias.kind, "re-export");
        assert!(
            snapshot.is_candidate("src/barrel.ts#used"),
            "unused local alias with a live target is actionable: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/impl.ts#dead"),
            "dead declaration must be reported: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot.is_candidate("src/barrel.ts#dead"),
            "a dead target is reported at its declaration, not through the alias: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn importing_a_local_export_alias_keeps_the_alias_and_target_live() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"local-alias-live","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { used } from \"../src/barrel\";\n\nexport default function Page() {\n  return used;\n}\n",
            ),
            (
                "src/barrel.ts",
                "import { used } from \"./impl\";\nexport { used };\n",
            ),
            (
                "src/impl.ts",
                "export const used = 1;\nexport const dead = 2;\n",
            ),
        ]);
        assert!(
            snapshot.is_live("src/barrel.ts#used"),
            "importing the alias must keep it live: {:?}",
            snapshot.candidates
        );
        assert!(
            !snapshot.is_candidate("src/barrel.ts#used"),
            "a live alias is not a candidate: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_live("src/impl.ts#used"),
            "the alias chain must keep the target live: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/impl.ts#dead"),
            "dead declaration must be reported: {:?}",
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
                 export const unstable_middleware = [];\n\
                 export default function Home({ loaderData }: Route.ComponentProps) { return <main>{loaderData.title}</main>; }\n",
            ),
            (
                "app/routes/about.tsx",
                "import type { Route } from './+types/about';\n\
                 export async function action({ request }: Route.ActionArgs) { return request.formData(); }\n\
                 export const clientMiddleware = [];\n\
                 export const unstable_clientMiddleware = [];\n\
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
            "app/routes/home.tsx#unstable_middleware",
            "app/routes/home.tsx#default",
            "app/routes/about.tsx#action",
            "app/routes/about.tsx#clientMiddleware",
            "app/routes/about.tsx#unstable_clientMiddleware",
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
                 export default function App() { return <Outlet />; }\n\
                 export function CatchBoundary() { return <main>Caught</main>; }\n\
                 export const shouldReload = () => false;\n\
                 export const unstable_shouldReload = () => false;\n",
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
        for symbol in [
            "app/root.tsx#default",
            "app/root.tsx#CatchBoundary",
            "app/root.tsx#shouldReload",
            "app/root.tsx#unstable_shouldReload",
            "app/routes/about._index.tsx#default",
        ] {
            assert!(
                entries.contains(&symbol),
                "a Remix route export is framework-invoked even with no imports: {symbol} missing from {entries:?}"
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

    #[test]
    fn types_reachable_through_an_externally_used_type_stay_live() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"type-graph","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import type { Host } from '../src/host';\n\nexport default function Page(props: { host: Host }) {\n  return <div>{props.host.label}</div>;\n}\n",
            ),
            (
                "src/host.ts",
                "export interface Host {\n  label: string;\n  image: Image | null;\n}\n\nexport interface Image {\n  width: number;\n  height: number;\n}\n\nexport function unusedHostHelper() {\n  return 1;\n}\n",
            ),
        ]);
        for key in ["src/host.ts#Host", "src/host.ts#Image"] {
            assert!(snapshot.is_live(key), "{key}: {:?}", snapshot.candidates);
            assert!(
                !snapshot
                    .internal_only
                    .iter()
                    .any(|candidate| candidate == key),
                "{key} is used through an externally imported type: {:?}",
                snapshot.internal_only
            );
        }
        assert!(
            snapshot.is_candidate("src/host.ts#unusedHostHelper"),
            "{:?}",
            snapshot.candidates
        );
    }
}
