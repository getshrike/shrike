use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use shrike_core::ids::module_symbol_key;
use shrike_core::{ClientBoundaryPolicy, EntryPolicy, ServerActionPolicy};
use shrike_syntax::{BindingKind, FileSummary, ParseStatus};

use super::Builder;
use crate::cosmos_config;
use crate::cypress_config;
use crate::docusaurus_config;
use crate::gatsby_config;
use crate::ladle_config;
use crate::next_config;
use crate::storybook_config;
use crate::test_config;
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

/// Compare a path against a convention relative to its owning package. The
/// package directory is already known, so a nested `lib/middleware.ts` is not
/// mistaken for `<package>/middleware.ts`.
fn at_own_package_root(path: &str, package_directory: &str, relative: &str) -> bool {
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
fn dotted_page_extensions(extensions: Option<&BTreeSet<String>>) -> Vec<String> {
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
pub(super) fn next_convention_stem<'a>(
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
fn next_convention_base<'a>(
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

/// Whether the package manifest explicitly declares any of `names`. A missing
/// or unreadable manifest returns `None`, so convention detection stays
/// conservative for incomplete trees instead of hiding a possible entry.
pub(super) fn package_declares_any(root: &Path, directory: &str, names: &[&str]) -> Option<bool> {
    let text = fs::read_to_string(root.join(directory).join("package.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
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
pub(super) fn package_declares_next(root: &Path, directory: &str) -> Option<bool> {
    package_declares_any(root, directory, &["next"])
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
    /// Next's `pageExtensions` may contain dots (`page.tsx`). Knip reads it
    /// from the config AST; read only the literal values here and scope each
    /// config to its package so a nested app is not widened by the root.
    pub(super) fn configured_page_extensions(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut page_extensions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for summary in &self.summaries {
            if !next_config::is_config_path(&summary.path) {
                continue;
            }
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                continue;
            };
            let extensions = next_config::page_extensions(&source);
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
        page_extensions
    }

    pub(super) fn detect_entries(&mut self) {
        let summaries = self.summaries.clone();
        let rsc_directives = self.project_uses_rsc_directives();
        let analyzed: BTreeSet<&str> = summaries
            .iter()
            .map(|summary| summary.path.as_str())
            .collect();
        let page_extensions = self.configured_page_extensions();
        let mut vite_packages: BTreeSet<String> = BTreeSet::new();
        let mut storybook_packages: BTreeSet<String> = BTreeSet::new();
        let mut cosmos_packages: BTreeSet<String> = BTreeSet::new();
        let mut docusaurus_packages: BTreeSet<String> = BTreeSet::new();
        let mut gatsby_packages: BTreeSet<String> = BTreeSet::new();
        let mut ladle_packages: BTreeSet<String> = BTreeSet::new();
        for package in self.resolver.packages() {
            if package_depends_on_any(self.resolver.root(), &package.directory, VITE_ENABLERS) {
                vite_packages.insert(package.directory.clone());
            }
            if package_depends_on_storybook(self.resolver.root(), &package.directory) {
                storybook_packages.insert(package.directory.clone());
            }
            if package_depends_on(self.resolver.root(), &package.directory, "react-cosmos") {
                cosmos_packages.insert(package.directory.clone());
            }
            if package_depends_on_any(
                self.resolver.root(),
                &package.directory,
                &[
                    "@docusaurus/core",
                    "@docusaurus/preset-classic",
                    "docusaurus",
                ],
            ) {
                docusaurus_packages.insert(package.directory.clone());
            }
            if package_depends_on_any(
                self.resolver.root(),
                &package.directory,
                &["gatsby", "gatsby-cli"],
            ) {
                gatsby_packages.insert(package.directory.clone());
            }
            if package_depends_on(self.resolver.root(), &package.directory, "@ladle/react") {
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
            for summary in &summaries {
                if !ladle_config::is_config_path(&summary.path)
                    || ladle_config::package_directory(&summary.path)
                        != Some(package_directory.as_str())
                {
                    continue;
                }
                saw_config = true;
                ladle_entry_files.insert(summary.path.clone());
                let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path))
                else {
                    continue;
                };
                let scan = ladle_config::scan(&summary.path, &source);
                patterns.extend(ladle_config::story_patterns(package_directory, &scan));
                if let Some(vite_config) = ladle_config::vite_config(package_directory, &scan) {
                    ladle_entry_files.insert(vite_config);
                }
            }
            if !saw_config {
                patterns.extend(ladle_config::story_patterns(
                    package_directory,
                    &ladle_config::ConfigScan::default(),
                ));
            }
            let Some(matcher) = storybook_config::matcher(&patterns) else {
                continue;
            };
            for path in &analyzed {
                if matcher.is_match(path) {
                    ladle_story_files.insert((*path).to_string());
                }
            }
        }
        for summary in &summaries {
            if ladle_config::is_components_path(&summary.path)
                && ladle_config::package_directory(&summary.path)
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
            let config = cosmos_config::CosmosConfig::load(self.resolver.root(), package_directory);
            let Some(matcher) = cosmos_config::matcher(package_directory, &config) else {
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
            let Some(matcher) = docusaurus_config::matcher(package_directory) else {
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
        for summary in &summaries {
            if !gatsby_config::is_node_path(&summary.path) {
                continue;
            }
            let Some(package) = self.resolver.package_for(&summary.path) else {
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
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                gatsby_dynamic_packages.insert(package.directory.clone());
                continue;
            };
            let scan = gatsby_config::template_references(&summary.path, &source);
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
                for importer in std::iter::once(summary.path.as_str()).chain(site_anchor.as_deref())
                {
                    let resolved = self.resolver.resolve(importer, &specifier);
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
        for package_directory in gatsby_dynamic_packages {
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
        // Test runners load setup files, global setup, and alias targets by
        // path from their configuration. A support file that no test imports
        // is still live when the runner names it, so resolve the config's
        // path literals against analyzed files before classifying entries.
        let mut test_config_modules: BTreeSet<String> = BTreeSet::new();
        for summary in &summaries {
            if !test_config::is_config_path(&summary.path) {
                continue;
            }
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                continue;
            };
            test_config_modules.extend(test_config::referenced_files(
                self.resolver.root(),
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
        for summary in &summaries {
            if !cypress_config::is_config_path(&summary.path) {
                continue;
            }
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or("", |package| package.directory.as_str());
            if !package_depends_on(self.resolver.root(), package_directory, "cypress") {
                continue;
            }
            let Ok(source) = fs::read_to_string(self.resolver.root().join(&summary.path)) else {
                continue;
            };
            let entries = cypress_config::entries(&analyzed, &summary.path, &source);
            cypress_specs.extend(entries.specs);
            cypress_support.extend(entries.support);
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
            if test_config_modules.contains(path) {
                add(
                    self,
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
            let package_directory = self
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
            let is_app_shape =
                package_path.starts_with("app/") || package_path.starts_with("src/app/");
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
                package_declares_next(self.resolver.root(), &package_directory).unwrap_or(true)
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
                let local =
                    package_declares_any(self.resolver.root(), &package_directory, &["@next/mdx"])
                        .unwrap_or(true);
                // Workspaces frequently hoist the MDX plugin to the root even
                // when each app declares `next` itself. A hoisted dependency
                // must not make the map look dead.
                let hoisted = !package_directory.is_empty()
                    && package_declares_any(self.resolver.root(), "", &["@next/mdx"])
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
                && package_depends_on(self.resolver.root(), &package_directory, "next-intl")
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
            if docusaurus_page_files.contains(path) {
                // Docusaurus renders every page module and reads the MDX
                // module surface, so its imports and exports are framework
                // API rather than dead application code.
                add(self, module.clone(), "docusaurus", "page", None, "both");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "docusaurus", "page-export", None, "both");
                }
            }
            if gatsby_template_files.contains(path) {
                // Gatsby renders the default component and reads the page
                // query, Head, and getServerData exports. Other exports in a
                // live template remain application code.
                add(
                    self,
                    module.clone(),
                    "gatsby",
                    "page-template",
                    None,
                    "both",
                );
                for (name, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    if matches!(
                        name.as_str(),
                        "default" | "query" | "Head" | "getServerData"
                    ) {
                        add(self, key, "gatsby", "page-template-export", None, "both");
                    }
                }
            }
            if gatsby_packages.contains(&package_directory)
                && package_path.starts_with("src/pages/")
            {
                // Gatsby's default page directory is `src/pages`. The default
                // component and the page query/Head/getServerData exports are
                // framework API; other exports remain application code.
                add(self, module.clone(), "gatsby", "page", None, "both");
                for (name, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    if matches!(
                        name.as_str(),
                        "default" | "query" | "Head" | "getServerData"
                    ) {
                        add(self, key, "gatsby", "page-export", None, "both");
                    }
                }
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
                // Next.js 16.4 reads dynamic param matching from both page
                // and layout modules (`collectAppPageSegments`). The generated
                // route type guards allow the pair on the same stems; route
                // handlers are excluded.
                if matches!(stem, "page" | "layout") {
                    public_names
                        .extend(["unstable_paramMatching", "unstable_generateParamMatching"]);
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
                    self,
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
                    self,
                    module.clone(),
                    "next-app",
                    "custom-server",
                    None,
                    "server",
                );
            }
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
            if summary.has_directive("use server")
                && self.config.server_action_policy == ServerActionPolicy::Protect
            {
                // Under `protect`, Next.js registers every export of a
                // `"use server"` module in the server-reference manifest,
                // whether or not application code imports it. Actions are
                // invoked through serialized references, so the import graph
                // is not evidence that an export is unused. The default
                // `graph` policy instead treats an unreferenced action as
                // dead, matching knip and fallow; build evidence can still
                // keep a referenced action live.
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
            if self.config.server_action_policy == ServerActionPolicy::Protect
                && rsc_directives
                && !summary.has_directive("use server")
                && !summary.inline_server_action_exports.is_empty()
            {
                // An inline `"use server"` body directive is a Server Action
                // even without a file-level directive. The action is invoked
                // through a serialized reference, so `protect` keeps it live
                // exactly like an export of a whole `"use server"` module.
                let exports = self.exports.get(path).cloned().unwrap_or_default();
                for name in &summary.inline_server_action_exports {
                    if let Some(key) = exports.get(name) {
                        add(
                            self,
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
            if self.config.test_entries && self.config.entry_policy != EntryPolicy::Production {
                if cosmos_decorator {
                    // Cosmos invokes the decorator module's default export. A
                    // named helper in the same file is not framework-invoked
                    // and stays analyzable.
                    add(
                        self,
                        module.clone(),
                        "react-cosmos",
                        "decorator",
                        None,
                        "both",
                    );
                    add(
                        self,
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
                        self,
                        module.clone(),
                        "react-cosmos",
                        "fixture",
                        None,
                        "both",
                    );
                    for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                        add(self, key, "react-cosmos", "fixture-export", None, "both");
                    }
                }
            }
            let cypress_spec = cypress_specs.contains(path);
            let cypress_support_file = cypress_support.contains(path);
            if self.config.test_entries
                && self.config.entry_policy != EntryPolicy::Production
                && (cypress_spec || cypress_support_file)
            {
                // Cypress executes the module for its hooks; like other test
                // runners it does not consume arbitrary named exports.
                add(
                    self,
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
            if self.config.test_entries
                && self.config.entry_policy != EntryPolicy::Production
                && (test || story)
                && !cosmos_fixture
                && !cosmos_decorator
            {
                add(self, module.clone(), "tests", "test", None, "both");
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
                    for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                        add(self, key, "tests", "test-export", None, "both");
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
            if self.config.entry_policy != EntryPolicy::Production && local_root {
                add(self, module.clone(), "local", "standalone", None, "both");
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
            let config_file = stem.ends_with(".config") || stem == "tsconfig";
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

    use shrike_core::{EngineConfig, ServerActionPolicy};
    use tempfile::tempdir;

    use super::expo_config_plugin_paths;
    use super::expo_router_root;
    use crate::discovery::{analyze_paths, discover_files};
    use crate::types::GraphSnapshot;

    fn snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
        snapshot_with_config(files, &EngineConfig::default())
    }

    fn snapshot_with_config(files: &[(&str, &str)], config: &EngineConfig) -> GraphSnapshot {
        let directory = tempdir().expect("tempdir");
        for (path, content) in files {
            let full = directory.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parents");
            }
            fs::write(full, content).expect("write fixture");
        }
        let paths: Vec<String> = files.iter().map(|(path, _)| (*path).to_string()).collect();
        analyze_paths(directory.path(), config, &paths)
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
    fn node_test_directories_and_test_files_are_entries() {
        let snapshot = snapshot(&[
            ("source/index.ts", "export const api = 1;\n"),
            (
                "test/test.ts",
                "import { api } from '../source/index.ts';\nconsole.log(api);\n",
            ),
            ("test/unit/helper.ts", "export const helper = 1;\n"),
            ("tests/unit/helper.ts", "export const helper = 1;\n"),
            (
                "tests/example.test.ts",
                "import { helper } from './unit/helper';\ntest('helper', () => helper);\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(entries.contains(&"test/test.ts#__module__"), "{entries:?}");
        assert!(
            entries.contains(&"test/unit/helper.ts#__module__"),
            "node --test executes every file under test/: {entries:?}"
        );
        assert!(
            entries.contains(&"tests/example.test.ts#__module__"),
            "{entries:?}"
        );
        assert!(
            !entries.contains(&"tests/unit/helper.ts#__module__"),
            "a tests/ support module is not executed by a runner: {entries:?}"
        );
        assert!(
            snapshot.is_live("tests/unit/helper.ts#helper"),
            "the imported support module stays live: {:?}",
            snapshot.live
        );
    }

    #[test]
    fn test_config_references_are_entries() {
        let snapshot = snapshot(&[
            (
                "config/vitest.config.ts",
                "export default { test: { setupFiles: ['./tests/setup.ts'] } };\n",
            ),
            (
                "vitest.config.ts",
                "const path = require('node:path');\nexport default { resolve: { alias: { 'server-only': path.resolve(__dirname, 'tests/stubs/server-only.ts') } } };\n",
            ),
            (
                "tests/setup.ts",
                "import { configure } from './support/live';\nconfigure();\n",
            ),
            (
                "tests/support/live.ts",
                "export const configure = () => 1;\n",
            ),
            (
                "tests/stubs/server-only.ts",
                "export const serverOnly = true;\n",
            ),
            ("tests/support/dead.ts", "export const dead = 1;\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"tests/setup.ts#__module__"),
            "a Vitest setup file is loaded by path: {entries:?}"
        );
        assert!(
            entries.contains(&"tests/stubs/server-only.ts#__module__"),
            "an alias target is loaded by the runner: {entries:?}"
        );
        assert!(
            snapshot.is_live("tests/support/live.ts#configure"),
            "the setup file's import stays live: {:?}",
            snapshot.live
        );
        assert!(
            !entries.contains(&"tests/support/dead.ts#__module__"),
            "an unreferenced support module is not an entry: {entries:?}"
        );
        assert!(
            snapshot.is_candidate("tests/support/dead.ts#dead"),
            "an unreferenced support module is dead: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot
                .unused_files
                .contains(&"tests/support/dead.ts".to_string()),
            "the dead support module is an unused file: {:?}",
            snapshot.unused_files
        );
    }

    #[test]
    fn cypress_specs_and_support_are_entries_only_with_the_dependency() {
        let files = [
            (
                "package.json",
                r#"{"name":"web","private":true,"dependencies":{"next":"^16.0.0","react":"^19.1.0"},"devDependencies":{"cypress":"^15.0.0"}}"#,
            ),
            (
                "cypress.config.ts",
                "export default { e2e: { specPattern: ['cypress/e2e/**/*.cy.ts', 'cypress/e2e/**/*.spec.ts'] } };\n",
            ),
            (
                "cypress/e2e/home.cy.ts",
                "import { specHelper } from '../../lib/spec-helper';\nspecHelper();\n",
            ),
            (
                "cypress/e2e/checkout.spec.ts",
                "import { specHelper } from '../../lib/spec-helper';\nspecHelper();\n",
            ),
            (
                "cypress/support/e2e.ts",
                "import { commandHelper } from '../../lib/command-helper';\ncommandHelper();\n",
            ),
            (
                "lib/spec-helper.ts",
                "export const specHelper = () => 1;\nexport const unusedSpecHelper = 2;\n",
            ),
            (
                "lib/command-helper.ts",
                "export const commandHelper = () => 1;\nexport const unusedCommandHelper = 2;\n",
            ),
            ("lib/dead.ts", "export const dead = 1;\n"),
        ];
        let cypress = snapshot(&files);
        let entries = entry_symbols(&cypress);
        for key in [
            "cypress/e2e/home.cy.ts#__module__",
            "cypress/support/e2e.ts#__module__",
        ] {
            assert!(
                entries.contains(&key),
                "{key} is executed by Cypress: {entries:?}"
            );
        }
        for key in [
            "lib/spec-helper.ts#specHelper",
            "lib/command-helper.ts#commandHelper",
        ] {
            assert!(
                cypress.is_live(key),
                "{key} is imported by Cypress: {:?}",
                cypress.live
            );
        }
        for key in [
            "lib/spec-helper.ts#unusedSpecHelper",
            "lib/command-helper.ts#unusedCommandHelper",
            "lib/dead.ts#dead",
        ] {
            assert!(
                cypress.is_candidate(key),
                "{key} is genuinely unused: {:?}",
                cypress.candidates
            );
        }
        assert!(
            cypress.unused_files.contains(&"lib/dead.ts".to_string()),
            "only the genuinely dead file is unused: {:?}",
            cypress.unused_files
        );
        // A custom pattern may also match the generic `.spec.` convention.
        // The file is still one Cypress spec, not a Cypress spec plus a test.
        let checkout: Vec<&str> = cypress
            .entries
            .iter()
            .filter(|entry| entry.symbol == "cypress/e2e/checkout.spec.ts#__module__")
            .map(|entry| entry.framework.as_str())
            .collect();
        assert_eq!(
            checkout,
            ["cypress"],
            "a Cypress spec is not also a generic test: {entries:?}"
        );

        let plain = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain","dependencies":{"react":"^19.0.0"}}"#,
            ),
            ("cypress.config.ts", "export default { e2e: {} };\n"),
            ("cypress/e2e/home.cy.ts", "export const home = 1;\n"),
            ("cypress/support/e2e.ts", "export const support = 1;\n"),
        ]);
        let entries = entry_symbols(&plain);
        for key in [
            "cypress/e2e/home.cy.ts#__module__",
            "cypress/support/e2e.ts#__module__",
        ] {
            assert!(
                !entries.contains(&key),
                "without a cypress dependency the config is inert: {key}"
            );
        }
        for path in ["cypress/e2e/home.cy.ts", "cypress/support/e2e.ts"] {
            assert!(
                plain.unused_files.contains(&path.to_string()),
                "unprotected Cypress files are dead: {:?}",
                plain.unused_files
            );
        }
    }

    #[test]
    fn unused_test_exports_are_candidates_but_framework_exports_are_entries() {
        let snapshot = snapshot(&[
            (
                "tests/helper.ts",
                "export const usedHelper = 1;\nexport const unusedHelper = 2;\n",
            ),
            (
                "tests/use-helper.test.ts",
                "import { usedHelper } from './helper';\ntest('helper', () => { usedHelper; });\n",
            ),
            (
                "src/Button.stories.tsx",
                "export default { title: 'Button' };\nexport const Primary = {};\n",
            ),
            ("__mocks__/client.ts", "export const mockValue = 1;\n"),
            (
                "tests/types.test-d.ts",
                "export type Assertion = { value: number };\nexport const fixture: Assertion = { value: 1 };\n",
            ),
            (
                "__typetest__/assertions.ts",
                "export type Expect<T extends true> = T;\nexport type Equal<A, B> = A extends B ? true : false;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            snapshot.is_candidate("tests/helper.ts#unusedHelper"),
            "an unreferenced test helper is dead: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_live("tests/helper.ts#usedHelper"),
            "an imported test helper stays live: {:?}",
            snapshot.live
        );
        for key in [
            "src/Button.stories.tsx#Primary",
            "src/Button.stories.tsx#default",
            "__mocks__/client.ts#mockValue",
            "tests/types.test-d.ts#Assertion",
            "tests/types.test-d.ts#fixture",
            "__typetest__/assertions.ts#Expect",
            "__typetest__/assertions.ts#Equal",
        ] {
            assert!(entries.contains(&key), "{key}: {entries:?}");
        }
    }

    #[test]
    fn unused_files_require_no_entry_and_no_incoming_import() {
        let snapshot = snapshot(&[
            (
                "app/page.tsx",
                "import { live } from '../src/live';\nexport default function Page() { return live; }\n",
            ),
            ("src/live.ts", "export const live = 1;\n"),
            ("src/dead.ts", "export const dead = 1;\n"),
            (
                "src/dead-importer.ts",
                "import { dependency } from './dependency';\nexport const dead = dependency;\n",
            ),
            ("src/dependency.ts", "export const dependency = 1;\n"),
            ("src/effects.ts", "console.log('side effect');\n"),
        ]);
        assert_eq!(
            snapshot.unused_files,
            vec![
                "src/dead-importer.ts".to_string(),
                "src/dead.ts".to_string(),
                "src/effects.ts".to_string(),
            ],
            "the page is an entry; the dependency has a dead importer"
        );
        assert!(
            snapshot.is_candidate("src/dead.ts#dead"),
            "{:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/dead-importer.ts#dead"),
            "{:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/dependency.ts#dependency"),
            "the dependency's only importer is itself dead"
        );
        assert!(
            !snapshot
                .unused_files
                .contains(&"src/dependency.ts".to_string()),
            "a file is unused only when no module imports it"
        );
        assert!(
            !snapshot.unused_files.contains(&"src/live.ts".to_string()),
            "an imported file is not unused"
        );
        assert!(
            !snapshot.unused_files.contains(&"app/page.tsx".to_string()),
            "an entry is not unused"
        );
    }

    #[test]
    fn standalone_script_exports_follow_the_graph() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","scripts":{"build":"tsx scripts/build.ts"}}"#,
            ),
            (
                "scripts/build.ts",
                "export function usedByScript() { return 1; }\n\
                 export function unusedExport() { return 2; }\n\
                 usedByScript();\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"scripts/build.ts#__module__"),
            "a package script names the module: {entries:?}"
        );
        assert!(
            snapshot.is_live("scripts/build.ts#usedByScript"),
            "a helper called by the script stays live: {:?}",
            snapshot.live
        );
        assert!(
            snapshot.is_candidate("scripts/build.ts#unusedExport"),
            "an unreferenced script export is dead: {:?}",
            snapshot.candidates
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
    fn documented_tool_scripts_are_not_entries_without_a_reference() {
        let snapshot = snapshot(&[
            (
                "scripts/generate-report.ts",
                "/**\n * Report generator.\n *\n * Usage: npx tsx scripts/generate-report.ts\n */\nfunction main() {\n  console.log('report');\n}\nmain();\n",
            ),
            (
                "scripts/tool.mjs",
                "#!/usr/bin/env node\nconsole.log('tool');\n",
            ),
            (
                "src/library.ts",
                "/** Usage: this is documentation, not an entry. */\nexport const value = 1;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            !entries.contains(&"scripts/generate-report.ts#__module__"),
            "a documented header is not a reference: {entries:?}"
        );
        assert!(
            !entries.contains(&"scripts/tool.mjs#__module__"),
            "a shebang is not a reference: {entries:?}"
        );
        assert!(
            snapshot
                .unused_files
                .contains(&"scripts/generate-report.ts".to_string()),
            "an unreferenced tool is an unused file: {:?}",
            snapshot.unused_files
        );
        assert!(
            snapshot
                .unused_files
                .contains(&"scripts/tool.mjs".to_string()),
            "an unreferenced shebang tool is an unused file: {:?}",
            snapshot.unused_files
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
    fn root_next_instrumentation_entries_keep_only_the_api_live() {
        let snapshot = snapshot(&[
            (
                "instrumentation.ts",
                "import { setup } from \"./lib/setup\";\n\
                 export async function register() { setup(); }\n\
                 export function onRequestError() {}\n\
                 export function unusedInstrumentationHelper() {}\n",
            ),
            (
                "instrumentation-client.ts",
                "export function onRouterTransitionStart() {}\n\
                 export const unusedClientHelper = 1;\n",
            ),
            ("lib/setup.ts", "export function setup() { return 1; }\n"),
            (
                "src/instrumentation.ts",
                "export function register() {}\n\
                 export const unusedSrcInstrumentationHelper = 1;\n",
            ),
            (
                "lib/instrumentation.ts",
                "export function unrelatedInstrumentation() {}\n",
            ),
            (
                "app/page.tsx",
                "export default function Page() { return null; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "instrumentation.ts#__module__",
            "instrumentation.ts#register",
            "instrumentation.ts#onRequestError",
            "instrumentation-client.ts#__module__",
            "instrumentation-client.ts#onRouterTransitionStart",
            "src/instrumentation.ts#__module__",
            "src/instrumentation.ts#register",
            "app/page.tsx#default",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} must stay live: {:?}",
                snapshot.live
            );
            assert!(
                entries.contains(&key),
                "{key} must be an entry: {entries:?}"
            );
        }
        assert!(
            snapshot.is_live("lib/setup.ts#setup"),
            "an import from instrumentation must keep setup live: {:?}",
            snapshot.live
        );
        for key in [
            "instrumentation.ts#unusedInstrumentationHelper",
            "instrumentation-client.ts#unusedClientHelper",
            "src/instrumentation.ts#unusedSrcInstrumentationHelper",
            "lib/instrumentation.ts#unrelatedInstrumentation",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn next_app_and_pages_are_package_relative() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"workspaces":["apps/*","packages/*"]}"#,
            ),
            (
                "apps/web/package.json",
                r#"{"name":"@probe/web","private":true,"dependencies":{"next":"^16.0.0","react":"^19.1.0"}}"#,
            ),
            (
                "packages/app/package.json",
                r#"{"name":"@probe/app","private":true,"dependencies":{"react":"^19.1.0"}}"#,
            ),
            (
                "packages/pages/package.json",
                r#"{"name":"@probe/pages","private":true,"dependencies":{"react":"^19.1.0"}}"#,
            ),
            (
                "apps/web/app/page.tsx",
                "export default function WebPage() { return null; }\nexport const unusedWebHelper = 1;\n",
            ),
            (
                "packages/app/src/page.tsx",
                "export default function DeadPage() { return null; }\nexport const unusedPageHelper = 2;\n",
            ),
            (
                "packages/app/src/layout.tsx",
                "export default function DeadLayout() { return null; }\nexport const unusedLayoutHelper = 3;\n",
            ),
            (
                "packages/pages/src/settings.tsx",
                "export default function DeadSettings() { return null; }\nexport const unusedSettingsHelper = 4;\n",
            ),
        ]);
        assert!(
            snapshot.is_live("apps/web/app/page.tsx#default"),
            "a package-relative app route is live: {:?}",
            snapshot.live
        );
        assert!(
            snapshot.is_candidate("apps/web/app/page.tsx#unusedWebHelper"),
            "a route helper is still analyzable: {:?}",
            snapshot.candidates
        );
        for key in [
            "packages/app/src/page.tsx#default",
            "packages/app/src/layout.tsx#default",
            "packages/pages/src/settings.tsx#default",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is not a Next route because the package path only contains `app`/`pages`: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn next_conventions_require_a_next_dependency() {
        let plain = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain-react","dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "export default function Page() { return null; }\n",
            ),
            (
                "pages/index.tsx",
                "export default function Home() { return null; }\n",
            ),
            (
                "middleware.ts",
                "export function middleware() { return null; }\n",
            ),
        ]);
        for key in [
            "app/page.tsx#default",
            "pages/index.tsx#default",
            "middleware.ts#middleware",
        ] {
            assert!(
                plain.is_candidate(key),
                "{key} is not a Next convention without a next dependency: {:?}",
                plain.candidates
            );
        }

        let next = snapshot(&[
            (
                "package.json",
                r#"{"name":"next-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "export default function Page() { return null; }\nexport const unusedPageHelper = 1;\n",
            ),
            (
                "pages/index.tsx",
                "export default function Home() { return null; }\n",
            ),
            (
                "middleware.ts",
                "export function middleware() { return null; }\n",
            ),
        ]);
        for key in [
            "app/page.tsx#default",
            "pages/index.tsx#default",
            "middleware.ts#middleware",
        ] {
            assert!(
                next.is_live(key),
                "{key} is a Next convention when the package depends on next: {:?}",
                next.live
            );
        }
        assert!(
            next.is_candidate("app/page.tsx#unusedPageHelper"),
            "a route helper stays analyzable: {:?}",
            next.candidates
        );

        // An incomplete tree has no manifest to consult; keep the convention
        // rather than hiding a possible route.
        let unknown = snapshot(&[(
            "app/page.tsx",
            "export default function Page() { return null; }\n",
        )]);
        assert!(
            unknown.is_live("app/page.tsx#default"),
            "a missing manifest fails open: {:?}",
            unknown.candidates
        );
    }

    #[test]
    fn unimported_server_actions_are_candidates_by_default() {
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
        let imported = "src/actions.ts#importedAction";
        let unimported = "src/actions.ts#unimportedAction";
        assert!(snapshot.is_live(imported), "{:?}", snapshot.live);
        assert!(
            !snapshot.is_candidate(imported),
            "{:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate(unimported),
            "an unreferenced action is dead under the default policy: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn server_actions_are_entries_under_protect_policy() {
        let config = EngineConfig {
            server_action_policy: ServerActionPolicy::Protect,
            ..EngineConfig::default()
        };
        let files = [
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "import { importedAction } from '../src/actions';\n\
                 import { inlineAction } from '../src/inline';\n\
                 export default function Page() { return importedAction() + inlineAction(); }\n",
            ),
            (
                "src/actions.ts",
                "\"use server\";\n\
                 export async function importedAction() { return 1; }\n\
                 export async function unimportedAction() { return 2; }\n\
                 export const arrowAction = async () => 3;\n\
                 export const value = 4;\n",
            ),
            (
                "src/inline.ts",
                "export async function inlineAction() { \"use server\"; return 1; }\n\
                 export const inlineArrow = async () => { \"use server\"; return 2; };\n\
                 export default async function () { \"use server\"; return 3; }\n\
                 export const plain = () => 4;\n",
            ),
        ];
        let snapshot = snapshot_with_config(&files, &config);
        let entries = entry_symbols(&snapshot);
        for key in [
            "src/actions.ts#importedAction",
            "src/actions.ts#unimportedAction",
            "src/inline.ts#inlineAction",
            "src/inline.ts#inlineArrow",
            "src/inline.ts#default",
        ] {
            assert!(entries.contains(&key), "{key}: {entries:?}");
            assert!(
                snapshot.is_live(key),
                "{key} must stay live: {:?}",
                snapshot.live
            );
            assert!(
                !snapshot.is_candidate(key),
                "{key} must not be a candidate: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot.is_candidate("src/inline.ts#plain"),
            "an ordinary export stays analyzable: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_live("src/actions.ts#value"),
            "a file-level use server directive protects every export: {:?}",
            snapshot.live
        );

        // Inline actions are only protected when the project has RSC bundler
        // evidence. Without it, the body directive has no special meaning.
        let no_bundler = snapshot_with_config(
            &[
                (
                    "package.json",
                    r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
                ),
                (
                    "src/inline.ts",
                    "export async function inlineAction() { \"use server\"; return 1; }\n",
                ),
            ],
            &config,
        );
        assert!(
            no_bundler.is_candidate("src/inline.ts#inlineAction"),
            "no RSC bundler evidence: {:?}",
            no_bundler.candidates
        );

        // Under the default `graph` policy, an inline action is dead unless
        // application code imports it, matching fallow's unused-action model.
        let graph = snapshot_with_config(&files, &EngineConfig::default());
        assert!(
            graph.is_candidate("src/inline.ts#inlineArrow"),
            "default graph policy: {:?}",
            graph.candidates
        );
        assert!(
            graph.is_candidate("src/inline.ts#default"),
            "an unreferenced default inline action is dead: {:?}",
            graph.candidates
        );
        assert!(
            graph.is_live("src/inline.ts#inlineAction"),
            "the imported inline action is live: {:?}",
            graph.live
        );
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
    fn invalid_client_exports_are_build_errors_in_client_routes() {
        let next_snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "packages/plain/package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "\"use client\";\nexport const metadata = { title: \"page\" };\nexport const revalidate = 60;\nexport default function Page() { return null; }\n",
            ),
            (
                "app/layout.tsx",
                "\"use client\";\nexport const viewport = { width: \"device-width\" };\nexport function generateMetadata() { return { title: \"layout\" }; }\nexport default function Layout({ children }: { children: React.ReactNode }) { return <>{children}</>; }\n",
            ),
            (
                "app/api/route.ts",
                "\"use client\";\nexport const metadata = { title: \"api\" };\nexport const viewport = { width: \"device-width\" };\nexport const revalidate = 60;\nexport function GET() { return new Response(\"ok\"); }\n",
            ),
            (
                "app/aliased/page.tsx",
                "\"use client\";\nconst metadata = { title: \"x\" };\nexport { metadata };\nexport default function Page() { return null; }\n",
            ),
            (
                "app/renamed/page.tsx",
                "\"use client\";\nconst metadata = { title: \"x\" };\nexport { metadata as config };\nexport default function Page() { return null; }\n",
            ),
            (
                "app/direct/page.tsx",
                "\"use client\";\nexport const metadata = { title: \"x\" };\nexport { metadata as config };\nexport default function Page() { return null; }\n",
            ),
            (
                "app/type-specifier/page.tsx",
                "\"use client\";\ntype metadata = { title: string };\nexport type { metadata };\nexport default function Page() { return null; }\n",
            ),
            (
                "app/type-decl/page.tsx",
                "\"use client\";\nexport interface metadata { title: string }\nexport default function Page() { return null; }\n",
            ),
            (
                "app/reexport/page.tsx",
                "\"use client\";\nexport { metadata } from \"./server\";\nexport default function Page() { return null; }\n",
            ),
            (
                "app/reexport/server.ts",
                "export const metadata = { title: \"x\" };\n",
            ),
            (
                "app/star/page.tsx",
                "\"use client\";\nexport * as metadata from \"./server\";\nexport default function Page() { return null; }\n",
            ),
            (
                "app/star/server.ts",
                "export const metadata = { title: \"x\" };\n",
            ),
            (
                "app/non-failing/page.tsx",
                "\"use client\";\nexport const dynamic = \"force-static\";\nexport const dynamicParams = false;\nexport const fetchCache = \"force-cache\";\nexport const runtime = \"edge\";\nexport const preferredRegion = \"home\";\nexport const maxDuration = 10;\nexport function GET() { return new Response(\"ok\"); }\nexport default function Page() { return null; }\n",
            ),
            (
                "app/template.tsx",
                "\"use client\";\nexport const metadata = { title: \"template\" };\nexport default function Template({ children }: { children: React.ReactNode }) { return <>{children}</>; }\n",
            ),
            (
                "app/not-found.tsx",
                "\"use client\";\nexport const metadata = { title: \"missing\" };\nexport default function NotFound() { return null; }\n",
            ),
            (
                "app/widget.tsx",
                "\"use client\";\nexport const metadata = { title: \"widget\" };\nexport default function Widget() { return null; }\n",
            ),
            (
                "packages/plain/app/page.tsx",
                "\"use client\";\nexport const metadata = { title: \"plain\" };\nexport default function Page() { return null; }\n",
            ),
            (
                "app/server/page.tsx",
                "export const metadata = { title: \"server\" };\nexport default function Page() { return null; }\n",
            ),
        ]);
        let invalid: Vec<(&str, u32)> = next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_client_export")
            .map(|diagnostic| {
                (
                    diagnostic.file.as_deref().unwrap_or(""),
                    diagnostic.span.map_or(0, |span| span.line),
                )
            })
            .collect();
        assert_eq!(
            invalid,
            vec![
                ("app/aliased/page.tsx", 3),
                ("app/api/route.ts", 2),
                ("app/api/route.ts", 4),
                ("app/direct/page.tsx", 2),
                ("app/layout.tsx", 2),
                ("app/layout.tsx", 3),
                ("app/page.tsx", 2),
                ("app/page.tsx", 3),
                ("app/reexport/page.tsx", 2),
                ("app/star/page.tsx", 2),
                ("app/type-specifier/page.tsx", 3),
            ],
            "{:?}",
            next_snapshot.diagnostics
        );
        assert!(
            next_snapshot
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "invalid_client_export")
                .all(|diagnostic| {
                    diagnostic.severity == shrike_core::Severity::Error
                        && diagnostic.message.contains("fails the build")
                }),
            "invalid client exports are build errors: {:?}",
            next_snapshot.diagnostics
        );

        // A project without Next.js, and a tree with no manifest at all, are
        // not enough evidence to claim a Next.js build failure.
        let react_only = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/page.tsx",
                "\"use client\";\nexport const metadata = { title: \"x\" };\nexport default function Page() { return null; }\n",
            ),
        ]);
        let missing_manifest = snapshot(&[(
            "app/page.tsx",
            "\"use client\";\nexport const metadata = { title: \"x\" };\nexport default function Page() { return null; }\n",
        )]);
        for control in [&react_only, &missing_manifest] {
            assert!(
                control
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code != "invalid_client_export"),
                "no positive Next.js evidence: {:?}",
                control.diagnostics
            );
        }
    }

    #[test]
    fn invalid_server_exports_are_project_gated_warnings() {
        let next_snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "packages/plain/package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/actions.ts",
                "\"use server\";\nexport const count = 1;\nexport const action = async () => 2;\nexport function declared() { return 3; }\nexport class Direct {}\n",
            ),
            (
                "src/values.ts",
                "\"use server\";\nconst local = \"text\";\nexport { local as alias };\nexport enum Kind { A }\n",
            ),
            ("src/default.ts", "\"use server\";\nexport default 42;\n"),
            (
                "src/mixed.ts",
                "\"use server\";\nexport const wrapped = (1 as number);\nexport const later = makeAction();\nexport const maybe = someIdentifier;\nexport default class {}\nexport { remote } from \"./remote\";\nexport type Payload = string;\n",
            ),
            ("src/no-directive.ts", "export const count = 1;\n"),
            (
                "packages/plain/src/values.ts",
                "\"use server\";\nexport const count = 1;\n",
            ),
        ]);
        let mut invalid: Vec<(&str, u32)> = next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_server_export")
            .map(|diagnostic| {
                (
                    diagnostic.file.as_deref().unwrap_or(""),
                    diagnostic.span.map_or(0, |span| span.line),
                )
            })
            .collect();
        invalid.sort_unstable();
        assert_eq!(
            invalid,
            vec![
                ("app/actions.ts", 2),
                // A shared package inside an RSC project is compiled in the
                // consuming app's context, so the project-level gate applies.
                ("packages/plain/src/values.ts", 2),
                ("src/default.ts", 2),
                ("src/mixed.ts", 2),
                // The local alias is anchored on its export specifier, the enum
                // on its declaration.
                ("src/values.ts", 3),
                ("src/values.ts", 4),
            ],
            "{:?}",
            next_snapshot.diagnostics
        );
        assert!(
            next_snapshot
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "invalid_server_export")
                .all(|diagnostic| {
                    diagnostic.severity == shrike_core::Severity::Warning
                        && diagnostic.message.contains("async function")
                        && diagnostic.message.contains("\"use server\"")
                }),
            "invalid server exports are warnings that name the boundary: {:?}",
            next_snapshot.diagnostics
        );

        // A project without an RSC bundler, and a tree with no manifest at
        // all, are not enough evidence to claim a Next.js build failure.
        let react_only = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/actions.ts",
                "\"use server\";\nexport const count = 1;\n",
            ),
        ]);
        let missing_manifest = snapshot(&[(
            "app/actions.ts",
            "\"use server\";\nexport const count = 1;\n",
        )]);
        for control in [&react_only, &missing_manifest] {
            assert!(
                control
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code != "invalid_server_export"),
                "no positive RSC bundler evidence: {:?}",
                control.diagnostics
            );
        }
    }

    #[test]
    fn misplaced_rsc_directives_are_project_gated_warnings() {
        let next_snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "packages/plain/package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/late-client.tsx",
                "import { Widget } from \"../components/widget\";\n\"use client\";\nexport function Page() { return <Widget />; }\n",
            ),
            (
                "app/late-server.ts",
                "const value = 1;\n\"use server\";\nexport async function action() { return value; }\n",
            ),
            // A leading directive is honored, even when imports follow it.
            (
                "app/leading.tsx",
                "\"use client\";\nimport { Widget } from \"../components/widget\";\nexport function Page() { return <Widget />; }\n",
            ),
            // A nested string and an out-of-scope `"use strict"` are not
            // directive statements.
            (
                "app/nested.ts",
                "export const labels = { directive: \"use client\" };\n",
            ),
            (
                "app/strict.ts",
                "import { Widget } from \"../components/widget\";\n\"use strict\";\nexport const Page = Widget;\n",
            ),
            // The gate is project-wide: a shared package inside an RSC project
            // is compiled in the consuming app's context.
            (
                "packages/plain/src/late-client.ts",
                "const value = 1;\n\"use client\";\nexport const Widget = value;\n",
            ),
            (
                "components/widget.tsx",
                "export function Widget() { return null; }\n",
            ),
        ]);
        let misplaced: Vec<(&str, u32, &str)> = next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "misplaced_directive")
            .map(|diagnostic| {
                (
                    diagnostic.file.as_deref().unwrap_or(""),
                    diagnostic.span.map_or(0, |span| span.line),
                    diagnostic.message.as_str(),
                )
            })
            .collect();
        assert_eq!(
            misplaced.len(),
            3,
            "one warning per misplaced occurrence: {:?}",
            next_snapshot.diagnostics
        );
        assert_eq!(
            misplaced
                .iter()
                .map(|(file, line, _)| (*file, *line))
                .collect::<Vec<_>>(),
            vec![
                ("app/late-client.tsx", 2),
                ("app/late-server.ts", 2),
                ("packages/plain/src/late-client.ts", 2),
            ],
            "{:?}",
            next_snapshot.diagnostics
        );
        assert!(
            misplaced[0].2.contains("\"use client\"")
                && misplaced[1].2.contains("\"use server\"")
                && misplaced[0].2.contains("leading prologue"),
            "the warning names the ignored directive: {:?}",
            misplaced
        );
        assert!(
            next_snapshot
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "misplaced_directive")
                .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Warning),
            "a misplaced directive is a warning, not an error: {:?}",
            next_snapshot.diagnostics
        );

        // Without an RSC bundler dependency the directives carry no special
        // meaning, so the same files produce no warning.
        let react_only = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/late-client.tsx",
                "import { Widget } from \"../components/widget\";\n\"use client\";\nexport function Page() { return <Widget />; }\n",
            ),
            (
                "components/widget.tsx",
                "export function Widget() { return null; }\n",
            ),
        ]);
        assert!(
            react_only
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "misplaced_directive"),
            "no RSC bundler evidence: {:?}",
            react_only.diagnostics
        );

        // Any of the framework-agnostic RSC bundlers is enough.
        let waku = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"waku":"^0.21.0","react":"^19.0.0"}}"#,
            ),
            (
                "src/late-client.ts",
                "const value = 1;\n\"use client\";\nexport const Widget = value;\n",
            ),
        ]);
        assert!(
            waku.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "misplaced_directive"),
            "waku is an RSC bundler: {:?}",
            waku.diagnostics
        );
    }

    #[test]
    fn mixed_client_server_barrels_are_project_gated_warnings() {
        let next_snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "packages/plain/package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "components/client.tsx",
                "\"use client\";\nexport function Button() { return null; }\n",
            ),
            (
                "lib/server.ts",
                "import \"server-only\";\nexport function readFile() { return null; }\n",
            ),
            (
                "lib/action.ts",
                "\"use server\";\nexport async function action() { return 1; }\n",
            ),
            (
                "lib/headers.ts",
                "import { cookies } from \"next/headers\";\nexport function getCookie() { return cookies(); }\n",
            ),
            (
                "lib/headers-namespace.ts",
                "import * as headers from \"next/headers\";\nexport function getHeader() { return headers.headers(); }\n",
            ),
            (
                "lib/headers-type-only.ts",
                "import type { cookies } from \"next/headers\";\nexport const util = 1;\n",
            ),
            (
                "lib/headers-side-effect.ts",
                "import \"next/headers\";\nexport const util = 1;\n",
            ),
            (
                "lib/format.ts",
                "export function format() { return \"\"; }\n",
            ),
            // Explicit re-exports, in both source orders.
            (
                "src/barrel.ts",
                "export { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
            ),
            (
                "src/barrel-server-first.ts",
                "export { readFile } from \"../lib/server\";\nexport { Button } from \"../components/client\";\n",
            ),
            (
                "src/barrel-order-client.ts",
                "export const note = 1;\nexport { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
            ),
            (
                "src/barrel-order-server.ts",
                "export const note = 1;\nexport const other = 2;\nexport { readFile } from \"../lib/server\";\nexport { Button } from \"../components/client\";\n",
            ),
            // Whole-module re-exports.
            (
                "src/barrel-star.ts",
                "export * from \"../components/client\";\nexport * from \"../lib/server\";\n",
            ),
            (
                "src/barrel-namespace.ts",
                "export * as client from \"../components/client\";\nexport * as server from \"../lib/server\";\n",
            ),
            // `"use server"` and a named `next/headers` API are server-only.
            (
                "src/barrel-action.ts",
                "export { Button } from \"../components/client\";\nexport { action } from \"../lib/action\";\n",
            ),
            (
                "src/barrel-headers.ts",
                "export { Button } from \"../components/client\";\nexport { getCookie } from \"../lib/headers\";\n",
            ),
            (
                "src/barrel-headers-namespace.ts",
                "export { Button } from \"../components/client\";\nexport { getHeader } from \"../lib/headers-namespace\";\n",
            ),
            // An ordinary utility, an external package, a type-only
            // re-export, and a type-only or side-effect server import are not
            // mixed boundaries.
            (
                "src/barrel-plain.ts",
                "export { Button } from \"../components/client\";\nexport { format } from \"../lib/format\";\n",
            ),
            (
                "src/barrel-external.ts",
                "export { Button } from \"../components/client\";\nexport { something } from \"some-package\";\n",
            ),
            (
                "src/barrel-type-only-reexport.ts",
                "export type { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
            ),
            (
                "src/barrel-type-only-server.ts",
                "export { Button } from \"../components/client\";\nexport { util } from \"../lib/headers-type-only\";\n",
            ),
            (
                "src/barrel-side-effect-server.ts",
                "export { Button } from \"../components/client\";\nexport { util } from \"../lib/headers-side-effect\";\n",
            ),
            (
                "src/barrel-no-client.ts",
                "export { readFile } from \"../lib/server\";\nexport { format } from \"../lib/format\";\n",
            ),
            (
                "src/barrel-no-server.ts",
                "export { Button } from \"../components/client\";\n",
            ),
            // The gate is project-wide: a shared barrel in a React-only
            // workspace package is compiled by the sibling Next app.
            (
                "packages/plain/src/barrel.ts",
                "export { Button } from \"../../../components/client\";\nexport { readFile } from \"../../../lib/server\";\n",
            ),
        ]);
        let mut mixed: Vec<(String, u32)> = next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "mixed_client_server_barrel")
            .map(|diagnostic| {
                (
                    diagnostic.file.clone().unwrap_or_default(),
                    diagnostic.span.map_or(0, |span| span.line),
                )
            })
            .collect();
        mixed.sort();
        assert_eq!(
            mixed,
            vec![
                ("packages/plain/src/barrel.ts".to_string(), 1),
                ("src/barrel-action.ts".to_string(), 1),
                ("src/barrel-headers-namespace.ts".to_string(), 1),
                ("src/barrel-headers.ts".to_string(), 1),
                ("src/barrel-namespace.ts".to_string(), 1),
                ("src/barrel-order-client.ts".to_string(), 2),
                ("src/barrel-order-server.ts".to_string(), 3),
                ("src/barrel-server-first.ts".to_string(), 1),
                ("src/barrel-star.ts".to_string(), 1),
                ("src/barrel.ts".to_string(), 1),
            ],
            "{:?}",
            next_snapshot.diagnostics
        );
        let first = next_snapshot
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "mixed_client_server_barrel")
            .expect("a mixed barrel");
        assert_eq!(first.severity, shrike_core::Severity::Warning);
        assert!(
            first.message.contains("../components/client")
                && first.message.contains("../lib/server")
                && first.message.contains("\"use client\"")
                && first.message.contains("server-only"),
            "the warning names both origins: {}",
            first.message
        );

        // Without an RSC bundler the directives carry no special meaning.
        let react_only = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "components/client.tsx",
                "\"use client\";\nexport function Button() { return null; }\n",
            ),
            (
                "lib/server.ts",
                "import \"server-only\";\nexport function readFile() { return null; }\n",
            ),
            (
                "src/barrel.ts",
                "export { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
            ),
        ]);
        assert!(
            react_only
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "mixed_client_server_barrel"),
            "no RSC bundler evidence: {:?}",
            react_only.diagnostics
        );

        // Any framework-agnostic RSC bundler enables the rule.
        let waku = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"waku":"^0.21.0","react":"^19.0.0"}}"#,
            ),
            (
                "components/client.tsx",
                "\"use client\";\nexport function Button() { return null; }\n",
            ),
            (
                "lib/server.ts",
                "import \"server-only\";\nexport function readFile() { return null; }\n",
            ),
            (
                "src/barrel.ts",
                "export { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
            ),
        ]);
        assert_eq!(
            waku.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "mixed_client_server_barrel")
                .count(),
            1,
            "{:?}",
            waku.diagnostics
        );
    }

    #[test]
    fn client_server_leaks_follow_the_use_client_cone() {
        let next_snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            // Direct server-only import in a client module.
            (
                "app/direct/page.tsx",
                "\"use client\";\n\nimport { headers } from \"next/headers\";\nexport default function Direct() { return null; }\n",
            ),
            // Transitive static import and a re-export chain.
            (
                "lib/transitive.ts",
                "import fs from \"node:fs\";\nexport function read() { return fs.readFileSync(\"x\", \"utf8\"); }\n",
            ),
            (
                "app/transitive/page.tsx",
                "\"use client\";\n\nimport { read } from \"../../lib/transitive\";\nexport default function Page() { return <main>{read()}</main>; }\n",
            ),
            ("lib/chain.ts", "export { read } from \"./transitive\";\n"),
            (
                "app/reexport/page.tsx",
                "\"use client\";\n\nimport { read } from \"../../lib/chain\";\nexport default function Page() { return <main>{read()}</main>; }\n",
            ),
            // A client module that re-exports server-only code directly.
            (
                "app/star/page.tsx",
                "\"use client\";\n\nexport { read } from \"../../lib/transitive\";\n",
            ),
            // An all-action Server Action module is the sanctioned boundary.
            (
                "lib/action.ts",
                "\"use server\";\nimport fs from \"node:fs\";\nexport async function action() { return fs.readFileSync(\"x\", \"utf8\"); }\n",
            ),
            (
                "app/action/page.tsx",
                "\"use client\";\n\nimport { action } from \"../../lib/action\";\nexport default function Page() { return <button onClick={() => action()}>run</button>; }\n",
            ),
            // A non-action export keeps the server module in the client cone.
            (
                "lib/action-mixed.ts",
                "\"use server\";\nimport fs from \"node:fs\";\nexport async function action() { return fs.readFileSync(\"x\", \"utf8\"); }\nexport const value = 1;\n",
            ),
            (
                "app/action-mixed/page.tsx",
                "\"use client\";\n\nimport { action, value } from \"../../lib/action-mixed\";\nexport default function Page() { return <button onClick={() => action()}>{value}</button>; }\n",
            ),
            // An async arrow export is action-shaped too.
            (
                "lib/action-arrow.ts",
                "\"use server\";\nimport fs from \"node:fs\";\nexport const action = async () => fs.readFileSync(\"x\", \"utf8\");\n",
            ),
            (
                "app/action-arrow/page.tsx",
                "\"use client\";\n\nimport { action } from \"../../lib/action-arrow\";\nexport default function Page() { return <button onClick={() => action()}>run</button>; }\n",
            ),
            // One non-action export keeps an otherwise all-arrow module in the
            // client cone.
            (
                "lib/action-arrow-mixed.ts",
                "\"use server\";\nimport fs from \"node:fs\";\nexport const action = async () => fs.readFileSync(\"x\", \"utf8\");\nexport const value = 1;\n",
            ),
            (
                "app/action-arrow-mixed/page.tsx",
                "\"use client\";\n\nimport { action, value } from \"../../lib/action-arrow-mixed\";\nexport default function Page() { return <button onClick={() => action()}>{value}</button>; }\n",
            ),
            // Type-only edges are erased and cannot leak.
            (
                "lib/type-server.ts",
                "import fs from \"node:fs\";\nexport type ServerType = string;\nexport function read() { return fs.readFileSync(\"x\", \"utf8\"); }\n",
            ),
            (
                "app/type-only/page.tsx",
                "\"use client\";\n\nimport type { ServerType } from \"../../lib/type-server\";\nexport default function Page() { const value: ServerType = \"ok\"; return <main>{value}</main>; }\n",
            ),
            (
                "app/type-reexport/page.tsx",
                "\"use client\";\n\nexport type { ServerType } from \"../../lib/type-server\";\n",
            ),
            // The sanctioned next/dynamic client-only escape hatch.
            (
                "app/dynamic-ssr/page.tsx",
                "\"use client\";\n\nimport dynamic from \"next/dynamic\";\nconst Chart = dynamic(() => import(\"../../lib/transitive\"), { ssr: false });\nexport default function Page() { return <Chart />; }\n",
            ),
            // Without ssr:false the dynamic import is an ordinary client edge.
            (
                "app/dynamic-no-ssr/page.tsx",
                "\"use client\";\n\nimport dynamic from \"next/dynamic\";\nconst Chart = dynamic(() => import(\"../../lib/transitive\"));\nexport default function Page() { return <Chart />; }\n",
            ),
            // A static edge of the same specifier is not an escape hatch.
            (
                "app/dynamic-static/page.tsx",
                "\"use client\";\n\nimport { read } from \"../../lib/transitive\";\nimport dynamic from \"next/dynamic\";\nconst Chart = dynamic(() => import(\"../../lib/transitive\"), { ssr: false });\nexport default function Page() { return <main>{read()}<Chart /></main>; }\n",
            ),
            // A server component is not a client root.
            (
                "app/server-component/page.tsx",
                "import { read } from \"../../lib/transitive\";\nexport default function Page() { return <main>{read()}</main>; }\n",
            ),
            // A namespace next/headers import is conservative; a side-effect
            // import and a default import are not server APIs.
            (
                "app/headers-namespace/page.tsx",
                "\"use client\";\n\nimport * as headers from \"next/headers\";\nexport default function Page() { return <main>{headers.headers().get(\"host\")}</main>; }\n",
            ),
            (
                "app/headers-side-effect/page.tsx",
                "\"use client\";\n\nimport \"next/headers\";\nexport default function Page() { return null; }\n",
            ),
            (
                "app/server-only/page.tsx",
                "\"use client\";\n\nimport \"server-only\";\nexport default function Page() { return null; }\n",
            ),
        ]);
        let mut leaks: Vec<(String, u32)> = next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "client_server_leak")
            .map(|diagnostic| {
                (
                    diagnostic.file.clone().unwrap_or_default(),
                    diagnostic.span.map_or(0, |span| span.line),
                )
            })
            .collect();
        leaks.sort();
        assert_eq!(
            leaks,
            vec![
                ("app/action-arrow-mixed/page.tsx".to_string(), 3),
                ("app/action-mixed/page.tsx".to_string(), 3),
                ("app/direct/page.tsx".to_string(), 3),
                ("app/dynamic-no-ssr/page.tsx".to_string(), 4),
                ("app/dynamic-static/page.tsx".to_string(), 3),
                ("app/headers-namespace/page.tsx".to_string(), 3),
                ("app/reexport/page.tsx".to_string(), 3),
                ("app/server-only/page.tsx".to_string(), 3),
                ("app/star/page.tsx".to_string(), 3),
                ("app/transitive/page.tsx".to_string(), 3),
            ],
            "{:?}",
            next_snapshot.diagnostics
        );
        let transitive = next_snapshot
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code == "client_server_leak"
                    && diagnostic.file.as_deref() == Some("app/transitive/page.tsx")
            })
            .expect("transitive leak");
        assert_eq!(transitive.severity, shrike_core::Severity::Warning);
        assert!(
            !transitive.message.contains("lib/chain")
                && transitive.message.contains("lib/transitive.ts")
                && transitive.message.contains("node:fs"),
            "the warning names the sink and path: {}",
            transitive.message
        );
        let dynamic = next_snapshot
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code == "client_server_leak"
                    && diagnostic.file.as_deref() == Some("app/dynamic-no-ssr/page.tsx")
            })
            .expect("dynamic leak");
        assert_eq!(dynamic.span.map(|span| span.line), Some(4));

        // Without an RSC bundler the directives carry no special meaning.
        let react_only = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "app/client.tsx",
                "\"use client\";\nimport \"server-only\";\nexport function Widget() { return null; }\n",
            ),
        ]);
        assert!(
            react_only
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "client_server_leak"),
            "no RSC bundler evidence: {:?}",
            react_only.diagnostics
        );

        // Any framework-agnostic RSC bundler enables the rule.
        let waku = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"waku":"^0.21.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/client.tsx",
                "\"use client\";\nimport \"server-only\";\nexport function Widget() { return null; }\n",
            ),
        ]);
        assert_eq!(
            waku.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "client_server_leak")
                .count(),
            1,
            "{:?}",
            waku.diagnostics
        );
    }

    #[test]
    fn next_route_collisions_and_dynamic_segment_conflicts_are_reported() {
        let routes = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "packages/plain/package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            // Route groups are transparent: both pages own /about.
            (
                "app/(marketing)/about/page.tsx",
                "export default function A() { return null; }\n",
            ),
            (
                "app/(shop)/about/page.tsx",
                "export default function B() { return null; }\n",
            ),
            // A page and a route handler share the URL-owner namespace.
            (
                "app/api/thing/page.tsx",
                "export default function C() { return null; }\n",
            ),
            (
                "app/api/thing/route.ts",
                "export async function GET() { return new Response('ok'); }\n",
            ),
            // A grouped root page collides with the root page.
            (
                "app/page.tsx",
                "export default function D() { return null; }\n",
            ),
            (
                "app/(group)/page.tsx",
                "export default function E() { return null; }\n",
            ),
            // Parallel slots fork the URL namespace.
            (
                "app/@team/members/page.tsx",
                "export default function F() { return null; }\n",
            ),
            (
                "app/@analytics/members/page.tsx",
                "export default function G() { return null; }\n",
            ),
            // Different dynamic names at the same position conflict.
            (
                "app/shop/[id]/page.tsx",
                "export default function H() { return null; }\n",
            ),
            (
                "app/shop/[id]/layout.tsx",
                "export default function Layout() { return null; }\n",
            ),
            (
                "app/shop/[slug]/page.tsx",
                "export default function I() { return null; }\n",
            ),
            // Private and intercepting subtrees are not routable participants.
            (
                "app/_private/about/page.tsx",
                "export default function J() { return null; }\n",
            ),
            (
                "app/(.)photo/[id]/page.tsx",
                "export default function K() { return null; }\n",
            ),
            // A React-only package with an app-shaped directory is not a Next app.
            (
                "packages/plain/app/(a)/about/page.tsx",
                "export default function L() { return null; }\n",
            ),
            (
                "packages/plain/app/(b)/about/page.tsx",
                "export default function M() { return null; }\n",
            ),
        ]);
        let findings: Vec<(&str, &str)> = routes
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "route_collision"
                    || diagnostic.code == "dynamic_segment_name_conflict"
            })
            .map(|diagnostic| {
                (
                    diagnostic.code.as_str(),
                    diagnostic.file.as_deref().unwrap_or(""),
                )
            })
            .collect();
        assert_eq!(
            findings,
            vec![
                ("route_collision", "app/(group)/page.tsx"),
                ("route_collision", "app/page.tsx"),
                ("route_collision", "app/(marketing)/about/page.tsx"),
                ("route_collision", "app/(shop)/about/page.tsx"),
                ("route_collision", "app/api/thing/page.tsx"),
                ("route_collision", "app/api/thing/route.ts"),
                // Every file proving the conflicting position is reported,
                // including the layout that shares the [id] spelling.
                ("dynamic_segment_name_conflict", "app/shop/[id]/layout.tsx"),
                ("dynamic_segment_name_conflict", "app/shop/[id]/page.tsx"),
                ("dynamic_segment_name_conflict", "app/shop/[slug]/page.tsx"),
            ],
            "{:?}",
            routes.diagnostics
        );
        assert!(
            routes
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == "route_collision"
                        || diagnostic.code == "dynamic_segment_name_conflict"
                })
                .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Error),
            "routing conflicts are build/runtime errors: {:?}",
            routes.diagnostics
        );

        // Without a manifest that declares Next.js, an app-shaped directory is
        // not a Next app and produces no routing diagnostics.
        let no_manifest = snapshot(&[
            (
                "apps/web/app/(a)/about/page.tsx",
                "export default function A() { return null; }\n",
            ),
            (
                "apps/web/app/(b)/about/page.tsx",
                "export default function B() { return null; }\n",
            ),
        ]);
        assert!(
            no_manifest.diagnostics.iter().all(|diagnostic| {
                diagnostic.code != "route_collision"
                    && diagnostic.code != "dynamic_segment_name_conflict"
            }),
            "{:?}",
            no_manifest.diagnostics
        );
    }

    #[test]
    fn next_route_conflicts_honor_src_app_and_dotted_page_extensions() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0"}}"#,
            ),
            (
                "next.config.ts",
                "const config = { pageExtensions: ['page.tsx'] };\nexport default config;\n",
            ),
            (
                "app/docs/page.page.tsx",
                "export default function A() { return null; }\n",
            ),
            (
                "app/docs/(alt)/page.page.tsx",
                "export default function B() { return null; }\n",
            ),
            (
                "src/app/shop/[id]/page.page.tsx",
                "export default function C() { return null; }\n",
            ),
            (
                "src/app/shop/[slug]/page.page.tsx",
                "export default function D() { return null; }\n",
            ),
        ]);
        let findings: Vec<(&str, &str)> = snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "route_collision"
                    || diagnostic.code == "dynamic_segment_name_conflict"
            })
            .map(|diagnostic| {
                (
                    diagnostic.code.as_str(),
                    diagnostic.file.as_deref().unwrap_or(""),
                )
            })
            .collect();
        assert_eq!(
            findings,
            vec![
                ("route_collision", "app/docs/(alt)/page.page.tsx"),
                ("route_collision", "app/docs/page.page.tsx"),
                (
                    "dynamic_segment_name_conflict",
                    "src/app/shop/[id]/page.page.tsx"
                ),
                (
                    "dynamic_segment_name_conflict",
                    "src/app/shop/[slug]/page.page.tsx"
                ),
            ],
            "{:?}",
            snapshot.diagnostics
        );
    }

    #[test]
    fn next_segment_config_conflicts_are_build_errors() {
        let routes = snapshot(&[
            (
                "package.json",
                r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            (
                "app/edge-gsp/page.tsx",
                "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
            ),
            (
                "app/edge-param/page.tsx",
                "export const runtime = 'edge';\nexport const unstable_paramMatching = {};\nexport default function Page() { return null; }\n",
            ),
            (
                "app/client-gsp/page.tsx",
                "\"use client\";\nexport const generateStaticParams = () => [];\nexport default function Page() { return null; }\n",
            ),
            (
                "app/client-instant/page.tsx",
                "\"use client\";\nexport const instant = true;\nexport default function Page() { return null; }\n",
            ),
            (
                "app/layout-stale/layout.tsx",
                "export const unstable_dynamicStaleTime = 60;\nexport default function Layout() { return null; }\n",
            ),
            (
                "app/stale-instant/page.tsx",
                "export const unstable_dynamicStaleTime = 60;\nexport const instant = true;\nexport default function Page() { return null; }\n",
            ),
            (
                "app/route-param/route.ts",
                "export const unstable_paramMatching = {};\nexport function GET() { return new Response('ok'); }\n",
            ),
            (
                "app/both-param/page.tsx",
                "export const unstable_paramMatching = {};\nexport const unstable_generateParamMatching = {};\nexport default function Page() { return null; }\n",
            ),
            (
                "app/asserted-runtime/page.tsx",
                "export const runtime = 'edge' as const;\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
            ),
            (
                "app/type-reexport-gsp/page.tsx",
                "\"use client\";\nexport type { generateStaticParams } from './types';\nexport default function Page() { return null; }\n",
            ),
            (
                "src/app/src-edge/page.tsx",
                "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
            ),
            // A function named `instant` is not a segment-config value.
            (
                "app/function-instant/page.tsx",
                "\"use client\";\nexport function instant() { return true; }\nexport default function Page() { return null; }\n",
            ),
            // A re-exported `runtime` is not read as segment config.
            (
                "app/runtime-reexport/page.tsx",
                "export { runtime } from './runtime';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
            ),
            // Parentheses defeat Next.js's const-value extractor.
            (
                "app/paren-runtime/page.tsx",
                "export const runtime = ('edge');\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
            ),
            // A non-Next package with an app-shaped directory is not a Next app.
            (
                "packages/plain/package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "packages/plain/app/plain/page.tsx",
                "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
            ),
        ]);
        let findings: Vec<&str> = routes
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_route_segment_config")
            .map(|diagnostic| diagnostic.file.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(
            findings,
            vec![
                "app/asserted-runtime/page.tsx",
                "app/both-param/page.tsx",
                "app/client-gsp/page.tsx",
                "app/client-instant/page.tsx",
                "app/edge-gsp/page.tsx",
                "app/edge-param/page.tsx",
                "app/layout-stale/layout.tsx",
                "app/route-param/route.ts",
                "app/stale-instant/page.tsx",
                "app/type-reexport-gsp/page.tsx",
                "src/app/src-edge/page.tsx",
            ],
            "{:?}",
            routes.diagnostics
        );
        assert!(
            routes
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "invalid_route_segment_config")
                .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Error),
            "segment-config conflicts are Next.js build errors: {:?}",
            routes.diagnostics
        );
        let messages: Vec<&str> = routes
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_route_segment_config")
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert!(
            messages
                .iter()
                .any(|message| message.contains("generateStaticParams")),
            "{messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("parameter matching")),
            "{messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("unstable_dynamicStaleTime")),
            "{messages:?}"
        );

        // Without a manifest that declares Next.js, the same exports are inert.
        let no_manifest = snapshot(&[(
            "app/page.tsx",
            "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        )]);
        assert!(
            no_manifest
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "invalid_route_segment_config"),
            "{:?}",
            no_manifest.diagnostics
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
    fn next_param_matching_exports_are_public_on_pages_and_layouts() {
        let snapshot = snapshot(&[
            (
                "app/layout.tsx",
                "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport function unusedLayoutHelper() { return 1; }\nexport default function Layout() { return null; }\n",
            ),
            (
                "app/page.tsx",
                "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport function unusedPageHelper() { return 1; }\nexport default function Page() { return null; }\n",
            ),
            (
                "app/route.ts",
                "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport function GET() { return new Response('ok'); }\n",
            ),
            (
                "app/loading.tsx",
                "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport default function Loading() { return null; }\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for file in ["app/layout.tsx", "app/page.tsx"] {
            for name in ["unstable_paramMatching", "unstable_generateParamMatching"] {
                let key = format!("{file}#{name}");
                assert!(
                    entries.contains(&key.as_str()),
                    "Next.js reads {name} from {file}: {entries:?}"
                );
            }
        }
        for file in ["app/route.ts", "app/loading.tsx"] {
            for name in ["unstable_paramMatching", "unstable_generateParamMatching"] {
                let key = format!("{file}#{name}");
                assert!(
                    !entries.contains(&key.as_str()),
                    "{name} is not a Next.js export on {file}: {entries:?}"
                );
                assert!(
                    snapshot
                        .candidates
                        .iter()
                        .any(|candidate| candidate == &key),
                    "{key} should remain a candidate: {:?}",
                    snapshot.candidates
                );
            }
        }
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

    fn ladle_app(package_json: &str) -> GraphSnapshot {
        snapshot(&[
            ("package.json", package_json),
            (
                ".ladle/config.mjs",
                r#"export default {
  stories: ["src/**/*.stories.tsx", "src/**/*.demo.tsx"],
  viteConfig: "./ladle-vite.ts",
};
"#,
            ),
            (
                ".ladle/components.tsx",
                "import { LadleShell } from \"./shell\";\nexport const Provider = ({ children }) => <LadleShell>{children}</LadleShell>;\nexport function unusedComponentsHelper() { return 0; }\n",
            ),
            (
                ".ladle/shell.tsx",
                "export function LadleShell({ children }) { return <div>{children}</div>; }\nexport function unusedShellHelper() { return 1; }\n",
            ),
            (
                "ladle-vite.ts",
                "import { plugin } from \"./ladle-plugin\";\nexport default { plugins: [plugin()] };\nexport function unusedViteHelper() { return 2; }\n",
            ),
            (
                "ladle-plugin.ts",
                "export function plugin() { return { name: \"ladle-fixture\" }; }\n",
            ),
            (
                "src/components/button.tsx",
                "export function Button() { return <button />; }\nexport function unusedButtonHelper() { return 3; }\n",
            ),
            (
                "src/button.stories.tsx",
                "import { Button } from \"./components/button\";\nexport const Primary = () => <Button />;\nexport const unusedStoryHelper = () => 4;\n",
            ),
            (
                "src/widget.demo.tsx",
                "export function DemoWidget() { return <section />; }\nexport function unusedDemoHelper() { return 5; }\n",
            ),
            (
                "src/dead.tsx",
                "export function DeadThing() { return 6; }\n",
            ),
        ])
    }

    #[test]
    fn ladle_components_config_and_custom_stories_are_entries() {
        let snapshot =
            ladle_app(r#"{"name":"ladle-app","devDependencies":{"@ladle/react":"^4.1.2"}}"#);
        let entries = entry_symbols(&snapshot);
        for key in [
            ".ladle/config.mjs#__module__",
            ".ladle/components.tsx#__module__",
            ".ladle/components.tsx#Provider",
            ".ladle/components.tsx#unusedComponentsHelper",
            "ladle-vite.ts#__module__",
            "ladle-vite.ts#unusedViteHelper",
            "src/button.stories.tsx#__module__",
            "src/button.stories.tsx#Primary",
            "src/button.stories.tsx#unusedStoryHelper",
            "src/widget.demo.tsx#__module__",
            "src/widget.demo.tsx#DemoWidget",
            "src/widget.demo.tsx#unusedDemoHelper",
        ] {
            assert!(
                entries.contains(&key),
                "{key} is part of Ladle's entry surface: {entries:?}"
            );
        }
        for key in [
            ".ladle/shell.tsx#LadleShell",
            "ladle-plugin.ts#plugin",
            "src/components/button.tsx#Button",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} is imported by a Ladle entry: {:?}",
                snapshot.live
            );
        }
        for key in [
            ".ladle/shell.tsx#unusedShellHelper",
            "src/components/button.tsx#unusedButtonHelper",
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
    fn ladle_entries_require_the_dependency() {
        let snapshot = ladle_app(r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#);
        for key in [
            ".ladle/components.tsx#Provider",
            ".ladle/components.tsx#unusedComponentsHelper",
            ".ladle/shell.tsx#LadleShell",
            "ladle-vite.ts#unusedViteHelper",
            "ladle-plugin.ts#plugin",
            "src/widget.demo.tsx#DemoWidget",
            "src/widget.demo.tsx#unusedDemoHelper",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "without Ladle the entry surface must not protect {key}: {:?}",
                snapshot.candidates
            );
        }
    }

    fn ladle_default_app(package_json: &str) -> GraphSnapshot {
        snapshot(&[
            ("package.json", package_json),
            (
                ".ladle/components.tsx",
                "import { LadleShell } from \"./shell\";\nexport const Provider = ({ children }) => <LadleShell>{children}</LadleShell>;\n",
            ),
            (
                ".ladle/shell.tsx",
                "export function LadleShell({ children }) { return <div>{children}</div>; }\n",
            ),
            (
                "src/MdxCard.tsx",
                "export function MdxCard() { return <div>docs</div>; }\nexport function unusedMdxHelper() { return 1; }\n",
            ),
            (
                "src/Intro.stories.mdx",
                "import { MdxCard } from \"./MdxCard\";\n\n# Introduction\n\n<MdxCard />\n",
            ),
        ])
    }

    #[test]
    fn ladle_default_story_glob_includes_mdx_pages() {
        let snapshot = ladle_default_app(
            r#"{"name":"ladle-app","devDependencies":{"@ladle/react":"^4.1.2"}}"#,
        );
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"src/Intro.stories.mdx#__module__"),
            "Ladle's default story glob includes MDX under src: {entries:?}"
        );
        assert!(
            snapshot.is_live("src/MdxCard.tsx#MdxCard"),
            "the component rendered by the MDX story must be live: {:?}",
            snapshot.live
        );
        assert!(
            snapshot.is_candidate("src/MdxCard.tsx#unusedMdxHelper"),
            "unused helper exports stay candidates: {:?}",
            snapshot.candidates
        );
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
    fn dotted_page_extensions_keep_next_routes_live() {
        let snapshot = discovered_snapshot(&[
            (
                "package.json",
                r#"{"name":"next-app","dependencies":{"next":"^16.4.2","react":"^19.1.0"}}"#,
            ),
            (
                "next.config.ts",
                "const config = { pageExtensions: ['page.tsx'] };\nexport default config;\n",
            ),
            (
                "app/page.page.tsx",
                "import { Widget } from '../components/widget';\n\
                 export default function Page() { return <Widget />; }\n\
                 export const unusedPageHelper = 1;\n",
            ),
            (
                "app/layout.page.tsx",
                "export default function Layout() { return null; }\n\
                 export const unusedLayoutHelper = 2;\n",
            ),
            (
                "app/route.page.tsx",
                "export function GET() { return new Response('ok'); }\n\
                 export const unusedRouteHelper = 3;\n",
            ),
            (
                "components/widget.tsx",
                "export function Widget() { return <div />; }\n\
                 export const unusedWidgetHelper = 4;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        for key in [
            "app/page.page.tsx#default",
            "app/layout.page.tsx#default",
            "app/route.page.tsx#GET",
        ] {
            assert!(
                entries.contains(&key),
                "the configured dotted extension is one extension, not a new stem: {key} missing from {entries:?}"
            );
        }
        assert!(
            !snapshot.is_candidate("components/widget.tsx#Widget"),
            "a component imported by a dotted-extension route stays live: {:?}",
            snapshot.candidates
        );
        for key in [
            "app/page.page.tsx#unusedPageHelper",
            "app/layout.page.tsx#unusedLayoutHelper",
            "app/route.page.tsx#unusedRouteHelper",
            "components/widget.tsx#unusedWidgetHelper",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "only genuinely unused exports are candidates: {key} missing from {:?}",
                snapshot.candidates
            );
        }
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
    fn mdx_component_map_requires_next_mdx_dependency() {
        let map = "export function useMDXComponents() { return {}; }\nexport const unusedMdxHelper = 1;\n";
        let plain = snapshot(&[
            (
                "package.json",
                r#"{"name":"next-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            ("mdx-components.tsx", map),
        ]);
        assert!(
            plain.is_candidate("mdx-components.tsx#useMDXComponents"),
            "Next core never loads the component map: {:?}",
            plain.candidates
        );
        assert!(
            plain.is_candidate("mdx-components.tsx#unusedMdxHelper"),
            "{:?}",
            plain.candidates
        );

        let mdx = snapshot(&[
            (
                "package.json",
                r#"{"name":"next-app","dependencies":{"next":"^16.0.0","@next/mdx":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            ("mdx-components.tsx", map),
        ]);
        assert!(
            !mdx.is_candidate("mdx-components.tsx#useMDXComponents"),
            "@next/mdx aliases the component map: {:?}",
            mdx.candidates
        );
        assert!(
            mdx.is_candidate("mdx-components.tsx#unusedMdxHelper"),
            "only the framework API is protected: {:?}",
            mdx.candidates
        );

        // An incomplete tree has no manifest to consult; keep the convention.
        let unknown = snapshot(&[("mdx-components.tsx", map)]);
        assert!(
            !unknown.is_candidate("mdx-components.tsx#useMDXComponents"),
            "a missing manifest fails open: {:?}",
            unknown.candidates
        );

        // A workspace can hoist the MDX plugin to the root while its apps only
        // declare `next`. Keep the map live rather than reporting a false orphan.
        let hoisted = snapshot(&[
            (
                "package.json",
                r#"{"name":"workspace","devDependencies":{"@next/mdx":"^16.0.0"}}"#,
            ),
            (
                "apps/web/package.json",
                r#"{"name":"web","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
            ),
            ("apps/web/mdx-components.tsx", map),
        ]);
        assert!(
            !hoisted.is_candidate("apps/web/mdx-components.tsx#useMDXComponents"),
            "a hoisted @next/mdx keeps the component map live: {:?}",
            hoisted.candidates
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
    fn unused_barrel_aliases_are_candidates_even_when_their_target_is_dead() {
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
            snapshot.is_candidate("src/barrel.ts#dead"),
            "a dead alias must be reported so its barrel line is removed: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn unused_barrel_alias_chains_report_every_link_to_a_dead_target() {
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
            snapshot.is_candidate("src/barrel.ts#dead"),
            "dead inner alias must be reported so the barrel line is removable: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/outer.ts#dead"),
            "dead outer alias must be reported so the barrel line is removable: {:?}",
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
            snapshot.is_candidate("src/barrel.ts#dead"),
            "a dead local alias must be reported so its export line is removable: {:?}",
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
    fn remix_custom_server_module_is_an_entry() {
        let remix = snapshot(&[
            (
                "package.json",
                r#"{"name":"remix-app","dependencies":{"@remix-run/node":"^2.15.0","@remix-run/react":"^2.15.0","react":"^18.3.1"}}"#,
            ),
            (
                "app/root.tsx",
                "import { Outlet } from '@remix-run/react';\nexport default function App() { return <Outlet />; }\n",
            ),
            (
                "server.ts",
                "import { startServer } from './server-lib';\nstartServer();\nexport function createServer() { return 1; }\nexport const unusedServerHelper = 2;\n",
            ),
            (
                "server-lib.ts",
                "export function startServer() { return 1; }\n",
            ),
            (
                "packages/other/server.ts",
                "export function otherServer() { return 1; }\n",
            ),
        ]);
        let entries = entry_symbols(&remix);
        assert!(
            entries.contains(&"server.ts#__module__"),
            "a Remix custom server is executed directly: {entries:?}"
        );
        assert!(
            !remix.is_candidate("server-lib.ts#startServer"),
            "the server module's top-level call keeps its import live: {:?}",
            remix.candidates
        );
        assert!(
            remix.is_candidate("server.ts#createServer")
                && remix.is_candidate("server.ts#unusedServerHelper"),
            "nothing reads the server module's exports: {:?}",
            remix.candidates
        );
        assert!(
            remix.is_candidate("packages/other/server.ts#otherServer"),
            "the convention is scoped to the app package root: {:?}",
            remix.candidates
        );

        let plain = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain-react","dependencies":{"react":"^18.3.1"}}"#,
            ),
            (
                "server.ts",
                "export function createServer() { return 1; }\n",
            ),
        ]);
        assert!(
            plain.is_candidate("server.ts#createServer"),
            "a plain React app has no Remix custom server: {:?}",
            plain.candidates
        );
    }

    #[test]
    fn next_custom_server_module_is_an_entry() {
        let next = snapshot(&[
            (
                "package.json",
                r#"{"name":"next-app","type":"module","dependencies":{"next":"^16.4.2","react":"^19.1.0"}}"#,
            ),
            (
                "app/page.tsx",
                "export default function Page() { return null; }\n",
            ),
            (
                "server.js",
                "import { boot } from './server-lib';\nboot();\nexport function start() { return 1; }\nexport const unusedServerHelper = 2;\n",
            ),
            ("server-lib.js", "export function boot() { return 1; }\n"),
        ]);
        let entries = entry_symbols(&next);
        assert!(
            entries.contains(&"server.js#__module__"),
            "a Next custom server is executed directly: {entries:?}"
        );
        assert!(
            !next.is_candidate("server-lib.js#boot"),
            "the server module's top-level call keeps its import live: {:?}",
            next.candidates
        );
        assert!(
            next.is_candidate("server.js#start")
                && next.is_candidate("server.js#unusedServerHelper"),
            "nothing reads the custom server's exports: {:?}",
            next.candidates
        );

        let plain = snapshot(&[
            (
                "package.json",
                r#"{"name":"plain-react","type":"module","dependencies":{"react":"^19.1.0"}}"#,
            ),
            ("server.js", "export function start() { return 1; }\n"),
        ]);
        assert!(
            plain.is_candidate("server.js#start"),
            "a plain React app has no Next custom server: {:?}",
            plain.candidates
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

    #[test]
    fn cosmos_fixtures_and_decorators_are_entries() {
        let snapshot = discovered_snapshot(&[
            (
                "package.json",
                r#"{"name":"cosmos-app","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"},"devDependencies":{"react-cosmos":"^7.0.0"}}"#,
            ),
            (
                "cosmos.config.json",
                "{\"plugins\":[\"react-cosmos-plugin-vite\"]}",
            ),
            (
                "components/Button.tsx",
                "export function Button() { return <button>ok</button>; }\nexport function unusedButtonHelper() { return 1; }\n",
            ),
            (
                "components/Card.tsx",
                "export function Card() { return <div>card</div>; }\nexport function unusedCardHelper() { return 2; }\n",
            ),
            (
                "components/Button.fixture.tsx",
                "import { Button } from './Button';\nimport { Card } from './Card';\nexport default function ButtonFixture() { return <Button />; }\nexport const CardFixture = () => <Card />;\n",
            ),
            (
                "components/__fixtures__/Nested.tsx",
                "import { NestedThing } from '../NestedThing';\nimport { Card } from '../Card';\nexport default function NestedFixture() { return <NestedThing card={<Card />} />; }\n",
            ),
            (
                "components/NestedThing.tsx",
                "export function NestedThing({ card }: { card: React.ReactNode }) { return <div>{card}</div>; }\nexport function unusedNestedHelper() { return 3; }\n",
            ),
            (
                "decorators/cosmos.decorator.tsx",
                "export default function CosmosDecorator({ children }: { children: React.ReactNode }) { return <div>{children}</div>; }\nexport function unusedDecoratorHelper() { return 4; }\n",
            ),
            ("dead.tsx", "export function DeadThing() { return 5; }\n"),
        ]);
        for key in [
            "components/Button.fixture.tsx#default",
            "components/Button.fixture.tsx#CardFixture",
            "components/__fixtures__/Nested.tsx#default",
            "components/Button.tsx#Button",
            "components/Card.tsx#Card",
            "components/NestedThing.tsx#NestedThing",
            "decorators/cosmos.decorator.tsx#default",
        ] {
            assert!(
                snapshot.is_live(key) || snapshot.entries.iter().any(|entry| entry.symbol == key),
                "{key} is a Cosmos user module or imported by one: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "components/Button.tsx#unusedButtonHelper",
            "components/Card.tsx#unusedCardHelper",
            "components/NestedThing.tsx#unusedNestedHelper",
            // Cosmos invokes only the decorator default export.
            "decorators/cosmos.decorator.tsx#unusedDecoratorHelper",
            "dead.tsx#DeadThing",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot.entries.iter().any(|entry| {
                entry.symbol == "components/Button.fixture.tsx#default"
                    && entry.framework == "react-cosmos"
            }),
            "fixture exports are Cosmos entries: {:?}",
            snapshot.entries
        );
        assert!(
            snapshot.entries.iter().any(|entry| {
                entry.symbol == "decorators/cosmos.decorator.tsx#default"
                    && entry.kind == "decorator-default"
            }),
            "the decorator default is a Cosmos entry: {:?}",
            snapshot.entries
        );

        let without_dependency = discovered_snapshot(&[
            (
                "package.json",
                r#"{"name":"plain-app","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"}}"#,
            ),
            (
                "components/Button.tsx",
                "export function Button() { return <button>ok</button>; }\n",
            ),
            (
                "components/Button.fixture.tsx",
                "import { Button } from './Button';\nexport default function ButtonFixture() { return <Button />; }\n",
            ),
            (
                "decorators/cosmos.decorator.tsx",
                "export default function CosmosDecorator({ children }: { children: React.ReactNode }) { return <div>{children}</div>; }\n",
            ),
        ]);
        for key in [
            "components/Button.fixture.tsx#default",
            "decorators/cosmos.decorator.tsx#default",
        ] {
            assert!(
                without_dependency.is_candidate(key),
                "without react-cosmos {key} is not framework-invoked: {:?}",
                without_dependency.candidates
            );
        }
    }

    #[test]
    fn cosmos_config_controls_directory_suffix_and_ignore() {
        let snapshot = discovered_snapshot(&[
            (
                "package.json",
                r#"{"name":"cosmos-custom","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"},"devDependencies":{"react-cosmos":"^7.0.0"}}"#,
            ),
            (
                "cosmos.config.json",
                r#"{"fixturesDir":"src/examples","fixtureFileSuffix":"example","ignore":["**/*.skip.*"]}"#,
            ),
            (
                "src/Widget.tsx",
                "export function Widget() { return <div>widget</div>; }\nexport function unusedWidgetHelper() { return 1; }\n",
            ),
            (
                "src/Widget.example.tsx",
                "import { Widget } from './Widget';\nexport default function WidgetFixture() { return <Widget />; }\nexport const AnotherFixture = () => <Widget />;\n",
            ),
            (
                "src/Widget.fixture.tsx",
                "import { Legacy } from './Legacy';\nexport default function LegacyFixture() { return <Legacy />; }\n",
            ),
            (
                "src/Legacy.tsx",
                "export function Legacy() { return <div>legacy</div>; }\nexport function unusedLegacyHelper() { return 2; }\n",
            ),
            (
                "src/examples/Showcase.tsx",
                "import { Widget } from '../Widget';\nexport const Showcase = () => <Widget />;\n",
            ),
            (
                "src/examples/Thing.skip.tsx",
                "import { IgnoredThing } from '../IgnoredThing';\nexport const Skipped = () => <IgnoredThing />;\n",
            ),
            (
                "src/IgnoredThing.tsx",
                "export function IgnoredThing() { return <div>ignored</div>; }\n",
            ),
            (
                "src/cosmos.decorator.js",
                "import { Frame } from './Frame';\nexport default function Decorator({ children }) { return <Frame>{children}</Frame>; }\nexport function unusedDecoratorHelper() { return 3; }\n",
            ),
            (
                "src/Frame.tsx",
                "export function Frame({ children }) { return <div>{children}</div>; }\nexport function unusedFrameHelper() { return 4; }\n",
            ),
            (
                "src/dead.tsx",
                "export function DeadThing() { return 5; }\n",
            ),
        ]);
        for key in [
            "src/Widget.example.tsx#default",
            "src/Widget.example.tsx#AnotherFixture",
            "src/examples/Showcase.tsx#Showcase",
            "src/Widget.tsx#Widget",
            "src/cosmos.decorator.js#default",
            "src/Frame.tsx#Frame",
        ] {
            assert!(
                snapshot.is_live(key) || snapshot.entries.iter().any(|entry| entry.symbol == key),
                "{key} uses the configured Cosmos conventions: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            // The configured suffix replaces `fixture`.
            "src/Widget.fixture.tsx#default",
            "src/Legacy.tsx#Legacy",
            "src/Legacy.tsx#unusedLegacyHelper",
            "src/Widget.tsx#unusedWidgetHelper",
            // Cosmos ignores the fixture directory entry.
            "src/examples/Thing.skip.tsx#Skipped",
            "src/IgnoredThing.tsx#IgnoredThing",
            "src/cosmos.decorator.js#unusedDecoratorHelper",
            "src/Frame.tsx#unusedFrameHelper",
            "src/dead.tsx#DeadThing",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }

    fn docusaurus_app(package_json: &str) -> GraphSnapshot {
        discovered_snapshot(&[
            ("package.json", package_json),
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"baseUrl":".","jsx":"react-jsx","paths":{"@site/*":["./*"]}}}"#,
            ),
            (
                "docs/intro.mdx",
                "import Callout from \"@site/src/components/Callout\";\n\n# Introduction\n\n<Callout>Read this.</Callout>\n",
            ),
            (
                "blog/hello.mdx",
                "import AuthorCard from \"@site/src/components/AuthorCard\";\n\n# Hello\n\n<AuthorCard name=\"Mark\" />\n",
            ),
            (
                "versioned_docs/version-1/guide.mdx",
                "import VersionNote from \"@site/src/components/VersionNote\";\n\n<VersionNote />\n",
            ),
            (
                "src/components/Callout.tsx",
                "export default function Callout({ children }: { children: React.ReactNode }) { return <aside>{children}</aside>; }\nexport function unusedCalloutHelper() { return 1; }\n",
            ),
            (
                "src/components/AuthorCard.tsx",
                "export default function AuthorCard({ name }: { name: string }) { return <div>{name}</div>; }\nexport function unusedAuthorHelper() { return 2; }\n",
            ),
            (
                "src/components/VersionNote.tsx",
                "export default function VersionNote() { return <p>version</p>; }\nexport function unusedVersionHelper() { return 3; }\n",
            ),
            (
                "src/components/dead.tsx",
                "export default function DeadComponent() { return <div>dead</div>; }\n",
            ),
        ])
    }

    #[test]
    fn docusaurus_docs_and_blog_pages_are_entries() {
        let snapshot = docusaurus_app(
            r#"{"name":"docs-site","private":true,"dependencies":{"@docusaurus/core":"^3.9.0","@docusaurus/preset-classic":"^3.9.0","react":"^19.0.0","react-dom":"^19.0.0"}}"#,
        );
        let entries = entry_symbols(&snapshot);
        for key in [
            "docs/intro.mdx#__module__",
            "blog/hello.mdx#__module__",
            "versioned_docs/version-1/guide.mdx#__module__",
        ] {
            assert!(
                entries.contains(&key),
                "Docusaurus pages are entries: {entries:?}"
            );
        }
        for key in [
            "src/components/Callout.tsx#default",
            "src/components/AuthorCard.tsx#default",
            "src/components/VersionNote.tsx#default",
        ] {
            assert!(
                snapshot.is_live(key),
                "the component rendered by a docs page must be live: {key}: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "src/components/Callout.tsx#unusedCalloutHelper",
            "src/components/AuthorCard.tsx#unusedAuthorHelper",
            "src/components/VersionNote.tsx#unusedVersionHelper",
            "src/components/dead.tsx#default",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is unused and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot.entries.iter().any(|entry| {
                entry.symbol == "docs/intro.mdx#__module__" && entry.framework == "docusaurus"
            }),
            "the page records its framework: {:?}",
            snapshot.entries
        );
    }

    #[test]
    fn docusaurus_pages_require_the_dependency() {
        let snapshot = docusaurus_app(
            r#"{"name":"plain-docs","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        );
        for key in [
            "src/components/Callout.tsx#default",
            "src/components/AuthorCard.tsx#default",
            "src/components/VersionNote.tsx#default",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "without @docusaurus/core {key} is not framework-invoked: {:?}",
                snapshot.candidates
            );
        }
    }

    fn gatsby_app(package_json: &str, node_source: &str) -> GraphSnapshot {
        discovered_snapshot(&[
            ("package.json", package_json),
            (
                "gatsby-config.ts",
                "export default { siteMetadata: { title: \"fixture\" }, plugins: [] };\n",
            ),
            ("gatsby-node.ts", node_source),
            (
                "src/pages/index.tsx",
                "import { PostBody } from \"../components/post-body\";\nexport default function Home() { return <PostBody title=\"home\" />; }\nexport function unusedHomeHelper() { return 4; }\n",
            ),
            (
                "src/components/post-body.tsx",
                "export function PostBody({ title }: { title: string }) { return <article>{title}</article>; }\nexport function unusedPostBodyHelper() { return 1; }\n",
            ),
            (
                "src/templates/blog-post.tsx",
                "import { PostBody } from \"../components/post-body\";\nexport default function BlogPost() { return <PostBody title=\"post\" />; }\nexport const query = `query BlogPost { post { title } }`;\nexport function Head() { return <title>Post</title>; }\nexport function unusedTemplateHelper() { return 2; }\n",
            ),
            (
                "src/templates/dead-template.tsx",
                "export default function DeadTemplate() { return <div>dead</div>; }\nexport const query = `query Dead { post { title } }`;\nexport function unusedDeadHelper() { return 3; }\n",
            ),
            (
                "src/other/dead.tsx",
                "export default function DeadComponent() { return <div>dead</div>; }\n",
            ),
        ])
    }

    #[test]
    fn gatsby_create_page_templates_are_entries() {
        let snapshot = gatsby_app(
            r#"{"name":"gatsby-site","private":true,"dependencies":{"gatsby":"^5.14.0","react":"^18.3.1","react-dom":"^18.3.1"}}"#,
            "import path from \"path\";\n\nexport async function createPages({ actions }: { actions: any }) {\n  actions.createPage({\n    path: \"/blog/hello\",\n    component: path.resolve(\"./src/templates/blog-post.tsx\"),\n    context: { slug: \"hello\" },\n  });\n}\n",
        );
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"src/templates/blog-post.tsx#__module__"),
            "a template named by createPage is an entry: {entries:?}"
        );
        for key in [
            "src/templates/blog-post.tsx#default",
            "src/templates/blog-post.tsx#query",
            "src/templates/blog-post.tsx#Head",
            "src/components/post-body.tsx#PostBody",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} is invoked by Gatsby: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "src/components/post-body.tsx#unusedPostBodyHelper",
            "src/templates/blog-post.tsx#unusedTemplateHelper",
            "src/templates/dead-template.tsx#default",
            "src/templates/dead-template.tsx#query",
            "src/templates/dead-template.tsx#unusedDeadHelper",
            "src/other/dead.tsx#default",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is not registered by createPage and must remain a candidate: {:?}",
                snapshot.candidates
            );
        }
        assert!(
            snapshot.entries.iter().any(|entry| {
                entry.symbol == "src/templates/blog-post.tsx#default"
                    && entry.framework == "gatsby"
                    && entry.kind == "page-template-export"
            }),
            "the template records its framework: {:?}",
            snapshot.entries
        );
    }

    #[test]
    fn gatsby_pages_are_entries_only_with_the_dependency() {
        let snapshot = gatsby_app(
            r#"{"name":"gatsby-site","private":true,"dependencies":{"gatsby":"^5.14.0","react":"^18.3.1"}}"#,
            "export {};\n",
        );
        assert!(
            snapshot.is_live("src/pages/index.tsx#default"),
            "Gatsby renders src/pages defaults: {:?}",
            snapshot.candidates
        );
        assert!(
            snapshot.is_candidate("src/pages/index.tsx#unusedHomeHelper"),
            "only the page API is framework-invoked: {:?}",
            snapshot.candidates
        );

        let plain = gatsby_app(
            r#"{"name":"plain-site","private":true,"dependencies":{"react":"^18.3.1"}}"#,
            "export {};\n",
        );
        assert!(
            plain.is_candidate("src/pages/index.tsx#default"),
            "without Gatsby, src/pages is not a framework page directory: {:?}",
            plain.candidates
        );
    }

    #[test]
    fn gatsby_dynamic_components_fall_back_to_the_template_directory() {
        let snapshot = gatsby_app(
            r#"{"name":"gatsby-dynamic","private":true,"dependencies":{"gatsby":"^5.14.0"}}"#,
            "export async function createPages({ actions, getTemplate }) {\n  actions.createPage({\n    path: \"/dynamic\",\n    component: getTemplate(),\n  });\n}\n",
        );
        for key in [
            "src/templates/blog-post.tsx#default",
            "src/templates/blog-post.tsx#query",
            "src/templates/blog-post.tsx#Head",
            "src/templates/dead-template.tsx#default",
            "src/templates/dead-template.tsx#query",
        ] {
            assert!(
                snapshot.is_live(key),
                "an opaque component protects the conventional directory: {key}: {:?}",
                snapshot.candidates
            );
        }
        for key in [
            "src/templates/blog-post.tsx#unusedTemplateHelper",
            "src/templates/dead-template.tsx#unusedDeadHelper",
            "src/other/dead.tsx#default",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "{key} is not a Gatsby page API export: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn gatsby_templates_require_the_dependency() {
        let snapshot = gatsby_app(
            r#"{"name":"plain-node","private":true,"dependencies":{"react":"^18.3.1"}}"#,
            "import path from \"path\";\nexport const createPages = ({ actions }: { actions: any }) => {\n  actions.createPage({ path: \"/blog\", component: path.resolve(\"./src/templates/blog-post.tsx\") });\n};\n",
        );
        for key in [
            "src/templates/blog-post.tsx#default",
            "src/templates/blog-post.tsx#query",
            "src/templates/blog-post.tsx#Head",
        ] {
            assert!(
                snapshot.is_candidate(key),
                "without gatsby {key} is not framework-invoked: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn gatsby_plugin_templates_resolve_from_the_site_and_the_plugin() {
        let snapshot = discovered_snapshot(&[
            (
                "package.json",
                r#"{"name":"gatsby-plugin-site","private":true,"dependencies":{"gatsby":"^5.14.0"}}"#,
            ),
            (
                "plugins/local/gatsby-node.js",
                "const path = require(\"path\");\nexports.createPages = ({ actions }) => {\n  actions.createPage({ path: \"/site\", component: path.resolve(\"./src/templates/site-post.tsx\") });\n  actions.createPage({ path: \"/plugin\", component: path.resolve(__dirname, \"templates/plugin-post.tsx\") });\n};\n",
            ),
            (
                "src/templates/site-post.tsx",
                "export default function SitePost() { return <div>site</div>; }\n",
            ),
            (
                "plugins/local/templates/plugin-post.tsx",
                "export default function PluginPost() { return <div>plugin</div>; }\n",
            ),
        ]);
        for key in [
            "src/templates/site-post.tsx#default",
            "plugins/local/templates/plugin-post.tsx#default",
        ] {
            assert!(
                snapshot.is_live(key),
                "{key} is named by the plugin's createPage call: {:?}",
                snapshot.candidates
            );
        }
    }

    #[test]
    fn gatsby_nested_node_files_are_not_registrations() {
        let snapshot = discovered_snapshot(&[
            (
                "package.json",
                r#"{"name":"gatsby-nested-node","private":true,"dependencies":{"gatsby":"^5.14.0"}}"#,
            ),
            (
                "src/gatsby-node.ts",
                "import path from \"path\";\nexport const createPages = ({ actions }: { actions: any }) => {\n  actions.createPage({ path: \"/blog\", component: path.resolve(\"./src/templates/blog-post.tsx\") });\n};\n",
            ),
            (
                "src/templates/blog-post.tsx",
                "export default function BlogPost() { return <div>post</div>; }\n",
            ),
        ]);
        assert!(
            snapshot.is_candidate("src/templates/blog-post.tsx#default"),
            "Gatsby does not execute src/gatsby-node.ts: {:?}",
            snapshot.candidates
        );
    }
}
