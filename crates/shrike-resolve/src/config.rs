use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use shrike_core::{EngineConfig, normalize_relative};

use crate::types::PackageInfo;
use crate::util::{normalize_path, parent_dir};

pub(crate) fn load_tsconfig(
    root: &Path,
    relative: Option<&str>,
) -> (Option<String>, Vec<(String, Vec<String>)>) {
    let Some(relative) = relative else {
        return (None, Vec::new());
    };
    let path = root.join(relative);
    let Ok(text) = std::fs::read_to_string(path) else {
        return (None, Vec::new());
    };
    let stripped = strip_json_comments(&text);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped) else {
        return (None, Vec::new());
    };
    let options = value.get("compilerOptions").cloned().unwrap_or_default();
    let base_url = options
        .get("baseUrl")
        .and_then(serde_json::Value::as_str)
        .map(|value| {
            let config_dir = relative.rsplit_once('/').map_or("", |(dir, _)| dir);
            normalize_relative(&format!("{config_dir}/{value}"))
        });
    let mut paths = Vec::new();
    if let Some(map) = options.get("paths").and_then(serde_json::Value::as_object) {
        let config_dir = relative.rsplit_once('/').map_or("", |(dir, _)| dir);
        for (pattern, targets) in map {
            let Some(targets) = targets.as_array() else {
                continue;
            };
            let replacements: Vec<String> = targets
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|target| {
                    if target.starts_with('.') {
                        normalize_relative(&format!("{config_dir}/{target}"))
                    } else {
                        normalize_relative(target)
                    }
                })
                .collect();
            if !replacements.is_empty() {
                paths.push((pattern.clone(), replacements));
            }
        }
        // Longest literal prefix wins.
        paths.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    }
    (base_url, paths)
}

pub(crate) fn strip_json_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            output.push(byte as char);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            output.push('"');
            index += 1;
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        output.push(byte as char);
        index += 1;
    }
    // Remove trailing commas, which tsconfig permits but JSON does not.
    let mut cleaned = String::with_capacity(output.len());
    let chars: Vec<char> = output.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == ',' {
            let mut lookahead = index + 1;
            while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                lookahead += 1;
            }
            if lookahead < chars.len() && (chars[lookahead] == '}' || chars[lookahead] == ']') {
                index += 1;
                continue;
            }
        }
        cleaned.push(chars[index]);
        index += 1;
    }
    cleaned
}

pub(crate) fn load_workspaces(
    root: &Path,
    files: &BTreeSet<String>,
    config: &EngineConfig,
) -> BTreeMap<String, PackageInfo> {
    let mut packages = BTreeMap::new();
    let mut directories = BTreeSet::new();
    for file in files {
        if file.ends_with("/package.json") || file == "package.json" {
            let dir = parent_dir(file);
            if !dir.is_empty() {
                directories.insert(dir);
            }
        }
    }
    // Discover workspace packages by walking shallow directories. Analysis
    // sets contain source files, not package.json, so this cannot rely on the
    // file list alone.
    fn discover_package_dirs(
        root: &Path,
        directory: &Path,
        depth: usize,
        output: &mut BTreeSet<String>,
    ) {
        if depth > 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                "node_modules"
                    | ".git"
                    | ".next"
                    | "dist"
                    | "build"
                    | "out"
                    | "coverage"
                    | ".shrike"
            ) {
                continue;
            }
            if path.join("package.json").is_file() {
                if let Ok(relative) = path.strip_prefix(root) {
                    let normalized = normalize_path(relative);
                    if !normalized.is_empty() {
                        output.insert(normalized);
                    }
                }
            }
            discover_package_dirs(root, &path, depth + 1, output);
        }
    }
    discover_package_dirs(root, root, 0, &mut directories);
    // The repository root is itself a package when it has a package.json. This
    // matters for self-references such as importing your own package name from
    // an example or app directory inside the same repository.
    if root.join("package.json").is_file() {
        directories.insert(String::new());
    }
    // Also discover package.json files from the workspace globs, even when the
    // host did not include them in the analysis set.
    if let Ok(text) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(workspaces) = value.get("workspaces") {
                let globs: Vec<String> = match workspaces {
                    serde_json::Value::Array(items) => items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(ToString::to_string)
                        .collect(),
                    serde_json::Value::Object(map) => map
                        .get("packages")
                        .and_then(serde_json::Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                for glob in globs {
                    let base = glob.trim_end_matches("/*").trim_end_matches("/**");
                    if base.contains('*') {
                        continue;
                    }
                    if let Ok(entries) = std::fs::read_dir(root.join(base)) {
                        for entry in entries.flatten() {
                            if entry.path().is_dir() {
                                let relative = normalize_relative(&format!(
                                    "{base}/{}",
                                    entry.file_name().to_string_lossy()
                                ));
                                directories.insert(relative);
                            }
                        }
                    }
                }
            }
        }
    }
    // pnpm workspace members.
    if let Ok(text) = std::fs::read_to_string(root.join("pnpm-workspace.yaml")) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("- ") {
                let base = rest
                    .trim_matches('\'')
                    .trim_matches('"')
                    .trim_end_matches("/*")
                    .trim_end_matches("/**");
                if base.contains('*') || base.starts_with('#') {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(root.join(base)) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            let relative = normalize_relative(&format!(
                                "{base}/{}",
                                entry.file_name().to_string_lossy()
                            ));
                            directories.insert(relative);
                        }
                    }
                }
            }
        }
    }

    for directory in directories {
        let path = root.join(&directory).join("package.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(name) = value.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let mut exports = BTreeMap::new();
        let mut export_target_paths = Vec::new();
        let mut conditional_root = None;
        if let Some(map) = value.get("exports").and_then(serde_json::Value::as_object) {
            // `exports` is either a subpath map (`"."`, `"./react"`, ...) or a
            // bare conditions object (`types`, `import`, `default`, ...). The
            // bare form describes the package root and is common in small
            // libraries.
            if !map.keys().any(|key| key.starts_with('.')) {
                let root = serde_json::Value::Object(map.clone());
                conditional_root = export_target(&root);
                export_target_paths.extend(export_targets(&root));
            }
            for (key, target) in map {
                let targets = export_targets(target);
                if let Some(target) = targets.first() {
                    exports.insert(key.clone(), join_package_path(&directory, target));
                }
                // Every condition can name a distinct public source file
                // (`react-server`, `worker`, `types`, ...). A package that
                // publishes the same root through several conditions still
                // has all of those source files as public API.
                if key == "." || key.starts_with("./") {
                    export_target_paths.extend(targets);
                }
            }
        }
        let declared_entry = exports
            .get(".")
            .cloned()
            .or_else(|| conditional_root.map(|target| join_package_path(&directory, &target)))
            .or_else(|| {
                value
                    .get("module")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| join_package_path(&directory, value))
            })
            .or_else(|| {
                value
                    .get("main")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| join_package_path(&directory, value))
            });
        // Published packages point at build output (`distribution/index.js`,
        // `esm/index.mjs`), which is not part of the analysis set. Map that
        // back to the source entry so its exports stay public API instead of
        // looking dead.
        let entry = match declared_entry {
            Some(declared) if files.contains(&declared) => Some(declared),
            Some(declared) => source_entry(&directory, files).or(Some(declared)),
            None => source_entry(&directory, files),
        };
        let public_api_pattern = if directory.is_empty() {
            "**".to_string()
        } else {
            format!("{directory}/**")
        };
        let public_api = config
            .public_api
            .iter()
            .any(|glob| shrike_core::glob::glob_match(glob, &public_api_pattern));
        let mut public_sources = BTreeSet::new();
        if let Some(entry) = &entry {
            public_sources.insert(entry.clone());
        }
        for target in &export_target_paths {
            let target = join_package_path(&directory, target);
            public_sources.extend(export_source_files(&directory, &target, files));
        }
        let script_files = package_script_files(&directory, &value, files);
        // A nested example may reuse the root package's name. Keep the
        // shallowest declaration so self-references resolve to the real
        // package, not the example that consumes it.
        if packages.contains_key(name) {
            continue;
        }
        packages.insert(
            name.to_string(),
            PackageInfo {
                name: name.to_string(),
                directory,
                entry,
                exports,
                public_api,
                public_sources,
                script_files,
            },
        );
    }
    packages
}

pub(crate) fn join_package_path(directory: &str, target: &str) -> String {
    if directory.is_empty() {
        normalize_relative(target)
    } else {
        normalize_relative(&format!("{directory}/{target}"))
    }
}

/// Find the source entry for a package whose declared entry is build output.
///
/// Candidates are intentionally conventional rather than exhaustive: they
/// cover the common `source/`, `src/`, and flat layouts. A package with an
/// unusual layout keeps its declared entry and is not guessed.
fn source_entry(directory: &str, files: &BTreeSet<String>) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        "source/index.ts",
        "source/index.tsx",
        "source/index.js",
        "source/index.mjs",
        "source/index.jsx",
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
        "src/index.mjs",
        "src/index.jsx",
        "index.ts",
        "index.tsx",
        "index.js",
        "index.mjs",
        "index.jsx",
    ];
    CANDIDATES
        .iter()
        .map(|candidate| join_package_path(directory, candidate))
        .find(|candidate| files.contains(candidate))
}

/// Map one declared `exports` target (usually build output) to the source
/// files that implement it. Wildcard targets (`./*`) match every analyzed
/// source file at the corresponding location.
fn export_source_files(
    directory: &str,
    target: &str,
    files: &BTreeSet<String>,
) -> BTreeSet<String> {
    const BUILD_PREFIXES: &[&str] = &[
        "esm/",
        "mjs/",
        "cjs/",
        "dist/",
        "distribution/",
        "lib/",
        "build/",
        "out/",
    ];
    let relative = if directory.is_empty() {
        target.to_string()
    } else {
        target
            .strip_prefix(&format!("{directory}/"))
            .unwrap_or(target)
            .to_string()
    };
    let mut stripped = relative.as_str();
    loop {
        let next = BUILD_PREFIXES
            .iter()
            .find_map(|prefix| stripped.strip_prefix(prefix));
        match next {
            Some(rest) => stripped = rest,
            None => break,
        }
    }
    let Some(stem) = code_stem(stripped) else {
        return BTreeSet::new();
    };
    let stem = strip_build_flavor(stem);
    let wildcard = stem.contains('*');
    // A bare `*` describes every published root module. Its source equivalents
    // live under `src/` or `source/`, so do not let it match tests/examples at
    // the repository root.
    let roots: &[&str] = if wildcard && stem.starts_with('*') {
        &["src", "source"]
    } else {
        &["", "src", "source"]
    };
    let mut patterns = Vec::new();
    for root in roots {
        let prefix = if root.is_empty() {
            String::new()
        } else {
            format!("{root}/")
        };
        patterns.push(format!("{prefix}{stem}"));
        // Conditional entries often use a flavor name for a file whose source
        // is `index.<flavor>` (`dist/react-server.esm.mjs` is built from
        // `src/index.react-server.ts`).
        if !wildcard && stem != "index" && !stem.starts_with("index.") {
            patterns.push(format!("{prefix}index.{stem}"));
        }
    }
    let mut sources = BTreeSet::new();
    for pattern in patterns {
        let pattern = join_package_path(directory, &pattern);
        for file in files {
            let Some(file_stem) = code_stem(file) else {
                continue;
            };
            if wildcard_match(&pattern, file_stem) {
                sources.insert(file.clone());
            }
        }
    }
    sources
}

/// Remove bundler flavor suffixes that describe a build variant rather than
/// the source module name (`index.esm`, `index.cjs`, `index.min`, ...).
fn strip_build_flavor(mut stem: &str) -> &str {
    const FLAVORS: &[&str] = &[
        ".esm",
        ".esnext",
        ".cjs",
        ".umd",
        ".min",
        ".production",
        ".development",
        ".modern",
        ".legacy",
        ".browser",
        ".node",
        ".module",
        ".main",
    ];
    while let Some(next) = FLAVORS.iter().find_map(|flavor| stem.strip_suffix(flavor)) {
        stem = next;
    }
    stem
}

fn code_stem(path: &str) -> Option<&str> {
    for extension in [
        ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".mjs", ".cjs", ".jsx", ".js",
    ] {
        if let Some(stem) = path.strip_suffix(extension) {
            return Some(stem);
        }
    }
    None
}

/// Match a path against a pattern where `*` matches any substring, including
/// `/`. Package export patterns use this broader meaning; the repository glob
/// matcher deliberately does not.
fn wildcard_match(pattern: &str, candidate: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == candidate;
    }
    let Some(mut rest) = candidate.strip_prefix(parts[0]) else {
        return false;
    };
    for part in &parts[1..parts.len() - 1] {
        let Some(index) = rest.find(part) else {
            return false;
        };
        rest = &rest[index + part.len()..];
    }
    rest.ends_with(parts[parts.len() - 1])
}

/// Files invoked by package scripts, and files exposed through `bin`.
/// A script or CLI entry runs without being imported, so the module graph
/// cannot reach it. Missing this class would propose deleting working tools.
fn package_script_files(
    directory: &str,
    package_json: &serde_json::Value,
    files: &BTreeSet<String>,
) -> BTreeSet<String> {
    const SHELL_WORDS: &[&str] = &[
        "node",
        "npx",
        "npm",
        "pnpm",
        "yarn",
        "bun",
        "bunx",
        "tsx",
        "ts-node",
        "ts-node-esm",
        "cross-env",
        "env",
        "sh",
        "bash",
        "zsh",
        "run",
        "run-s",
        "run-p",
        "exec",
        "turbo",
        "vitest",
        "jest",
        "playwright",
        "eslint",
        "prettier",
        "next",
        "drizzle-kit",
        "tsc",
        "rimraf",
        "del",
        "shx",
        "cd",
        "echo",
        "xargs",
        "export",
    ];
    let mut output = BTreeSet::new();
    let mut commands = Vec::new();
    if let Some(scripts) = package_json
        .get("scripts")
        .and_then(serde_json::Value::as_object)
    {
        for command in scripts.values().filter_map(serde_json::Value::as_str) {
            commands.push(command.to_string());
        }
    }
    match package_json.get("bin") {
        Some(serde_json::Value::String(target)) => commands.push(target.clone()),
        Some(serde_json::Value::Object(map)) => {
            for target in map.values().filter_map(serde_json::Value::as_str) {
                commands.push(target.to_string());
            }
        }
        _ => {}
    }
    for command in commands {
        for raw in shell_tokens(&command) {
            let token = raw.trim_matches(|character: char| {
                character.is_whitespace()
                    || matches!(
                        character,
                        '\'' | '"' | '(' | ')' | ';' | '|' | '&' | '[' | ']'
                    )
            });
            if token.is_empty()
                || token.starts_with('-')
                || token.starts_with('/')
                || token.starts_with('$')
                || token.contains('*')
                || token.contains('=')
                || SHELL_WORDS.contains(&token)
            {
                continue;
            }
            let relative = if directory.is_empty() {
                normalize_relative(token)
            } else {
                normalize_relative(&format!("{directory}/{token}"))
            };
            if files.contains(&relative) {
                output.insert(relative);
                continue;
            }
            let mut matched = false;
            for extension in [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"] {
                let candidate = format!("{relative}{extension}");
                if files.contains(&candidate) {
                    output.insert(candidate);
                    matched = true;
                }
            }
            if !matched && !relative.contains('.') {
                let prefix = format!("{relative}/");
                for file in files.iter().filter(|file| file.starts_with(&prefix)) {
                    output.insert(file.clone());
                }
            }
        }
    }
    output
}

fn shell_tokens(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quote != Some('\'') => escaped = true,
            '\'' | '"' => {
                if quote == Some(character) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(character);
                } else {
                    current.push(character);
                }
            }
            ' ' | '\t' | '\n' if quote.is_none() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            '&' | '|' | ';' if quote.is_none() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(crate) fn export_target(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(target) => Some(target.clone()),
        serde_json::Value::Object(map) => map
            .get("import")
            .or_else(|| map.get("default"))
            .or_else(|| map.get("require"))
            .and_then(export_target),
        serde_json::Value::Array(items) => items.iter().find_map(export_target),
        _ => None,
    }
}

/// Every file target named by an export condition, including condition
/// branches that do not apply to a normal import (`types`, `react-server`,
/// `worker`, ...). Public source detection uses these to keep all published
/// entrypoints alive.
fn export_targets(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(target) => vec![target.clone()],
        serde_json::Value::Object(map) => map.values().flat_map(export_targets).collect(),
        serde_json::Value::Array(items) => items.iter().flat_map(export_targets).collect(),
        _ => Vec::new(),
    }
}
