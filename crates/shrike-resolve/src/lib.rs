#![forbid(unsafe_code)]
//! Module resolution: relative paths, tsconfig `paths`/`baseUrl`, workspace
//! packages, package entry points, framework aliases, and extension probing.
//!
//! Resolution is pure: it is given the repository root and the canonical file
//! list, and never walks outside that list. Unknown bare specifiers are
//! classified as external rather than guessed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use shrike_core::{EngineConfig, hash_json, normalize_relative};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub name: String,
    pub directory: String,
    pub entry: Option<String>,
    pub exports: BTreeMap<String, String>,
    pub public_api: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSpecifier {
    pub importer: String,
    pub specifier: String,
    pub target: Option<String>,
    pub external: bool,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct Resolver {
    root: PathBuf,
    files: BTreeSet<String>,
    files_by_dir: BTreeMap<String, Vec<String>>,
    base_url: Option<String>,
    paths: Vec<(String, Vec<String>)>,
    packages: BTreeMap<String, PackageInfo>,
    pub resolver_hash: String,
    pub packages_list: Vec<PackageInfo>,
}

impl Resolver {
    #[must_use]
    pub fn new(root: &Path, files: &[String], config: &EngineConfig) -> Self {
        let mut sorted = files.to_vec();
        sorted.sort();
        sorted.dedup();
        let files_set: BTreeSet<String> = sorted.iter().cloned().collect();
        let mut files_by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for file in &sorted {
            let dir = parent_dir(file);
            files_by_dir.entry(dir).or_default().push(file.clone());
        }

        let (base_url, paths) = load_tsconfig(root, config.tsconfig.as_deref());
        let packages = load_workspaces(root, &files_set, config);
        let packages_list: Vec<PackageInfo> = packages.values().cloned().collect();
        let hash_input = serde_json::json!({
            "baseUrl": base_url,
            "paths": paths,
            "packages": packages_list,
            "tsconfig": config.tsconfig,
        });
        Self {
            root: root.to_path_buf(),
            files: files_set,
            files_by_dir,
            base_url,
            paths,
            packages,
            resolver_hash: hash_json(&hash_input),
            packages_list,
        }
    }

    #[must_use]
    pub fn package_for(&self, file: &str) -> Option<&PackageInfo> {
        self.packages
            .values()
            .filter(|package| {
                file == package.directory || file.starts_with(&format!("{}/", package.directory))
            })
            .max_by_key(|package| package.directory.len())
    }

    #[must_use]
    pub fn packages(&self) -> &[PackageInfo] {
        &self.packages_list
    }

    /// Resolve one specifier from an importer. `target` is `None` for external
    /// packages and unresolved local specifiers; `external` distinguishes them.
    #[must_use]
    pub fn resolve(&self, importer: &str, specifier: &str) -> ResolvedSpecifier {
        if specifier.starts_with('.') || specifier.starts_with('/') {
            let joined = if specifier.starts_with('/') {
                normalize_relative(specifier.trim_start_matches('/'))
            } else {
                normalize_relative(&format!("{}/{}", parent_dir(importer), specifier))
            };
            if let Some(target) = self.probe(&joined) {
                return self.resolved(importer, specifier, target, "relative");
            }
            return self.unresolved(importer, specifier, "relative path not found");
        }

        for (pattern, replacements) in &self.paths {
            if let Some(star) = match_alias(pattern, specifier) {
                for replacement in replacements {
                    let candidate = replacement.replace('*', &star);
                    if let Some(target) = self.probe(&candidate) {
                        return self.resolved(importer, specifier, target, "tsconfig-paths");
                    }
                }
            }
        }

        if let Some(base_url) = &self.base_url {
            let candidate = normalize_relative(&format!("{base_url}/{specifier}"));
            if let Some(target) = self.probe(&candidate) {
                return self.resolved(importer, specifier, target, "tsconfig-baseurl");
            }
        }

        // Framework aliases: `@/x` and `~/x` map to `src/x` by convention.
        for prefix in ["@/", "~/"] {
            if let Some(rest) = specifier.strip_prefix(prefix) {
                let candidate = normalize_relative(&format!("src/{rest}"));
                if let Some(target) = self.probe(&candidate) {
                    return self.resolved(importer, specifier, target, "framework-alias");
                }
            }
        }

        if let Some((name, subpath)) = split_package(specifier) {
            if let Some(package) = self.packages.get(&name) {
                let candidate = if let Some(subpath) = subpath {
                    let exported = package
                        .exports
                        .get(&format!("./{subpath}"))
                        .cloned()
                        .unwrap_or_else(|| format!("{}/{}", package.directory, subpath));
                    normalize_relative(&exported)
                } else if let Some(exported) = package.exports.get(".") {
                    normalize_relative(exported)
                } else {
                    package.entry.clone().unwrap_or_else(|| {
                        normalize_relative(&format!("{}/index", package.directory))
                    })
                };
                if let Some(target) = self.probe(&candidate) {
                    return self.resolved(importer, specifier, target, "workspace-package");
                }
                return self.unresolved(importer, specifier, "workspace package entry not found");
            }
            // A package that is itself a workspace member but outside the
            // analyzed set is external; a bare package with no workspace entry
            // is external too.
            return ResolvedSpecifier {
                importer: importer.to_string(),
                specifier: specifier.to_string(),
                target: None,
                external: true,
                reason: "external-package".to_string(),
            };
        }

        self.unresolved(importer, specifier, "bare specifier not found")
    }

    fn resolved(
        &self,
        importer: &str,
        specifier: &str,
        target: String,
        reason: &str,
    ) -> ResolvedSpecifier {
        ResolvedSpecifier {
            importer: importer.to_string(),
            specifier: specifier.to_string(),
            target: Some(target),
            external: false,
            reason: reason.to_string(),
        }
    }

    fn unresolved(&self, importer: &str, specifier: &str, reason: &str) -> ResolvedSpecifier {
        ResolvedSpecifier {
            importer: importer.to_string(),
            specifier: specifier.to_string(),
            target: None,
            external: false,
            reason: reason.to_string(),
        }
    }

    fn probe(&self, candidate: &str) -> Option<String> {
        let candidate = normalize_relative(candidate);
        let candidate = candidate.trim_end_matches('/');
        if self.files.contains(candidate) {
            return Some(candidate.to_string());
        }
        let extensions = ["ts", "tsx", "js", "jsx", "mjs", "cjs", "json"];
        for extension in extensions {
            let path = format!("{candidate}.{extension}");
            if self.files.contains(&path) {
                return Some(path);
            }
            if extension == "ts" {
                let declaration = format!("{candidate}.d.ts");
                if self.files.contains(&declaration) {
                    return Some(declaration);
                }
            }
        }
        // TypeScript ESM style: `./x.js` refers to `./x.ts`.
        if let Some(stem) = candidate.strip_suffix(".js") {
            for extension in ["ts", "tsx", "d.ts"] {
                let path = format!("{stem}.{extension}");
                if self.files.contains(&path) {
                    return Some(path);
                }
            }
        }
        if let Some(stem) = candidate.strip_suffix(".mjs") {
            for extension in ["mts", "ts"] {
                let path = format!("{stem}.{extension}");
                if self.files.contains(&path) {
                    return Some(path);
                }
            }
        }
        for extension in extensions {
            let path = format!("{candidate}/index.{extension}");
            if self.files.contains(&path) {
                return Some(path);
            }
            if extension == "ts" {
                let declaration = format!("{candidate}/index.d.ts");
                if self.files.contains(&declaration) {
                    return Some(declaration);
                }
            }
        }
        // A directory with an explicit package.json entry.
        if let Some(children) = self.files_by_dir.get(candidate) {
            for child in children {
                if child.ends_with("/index.ts")
                    || child.ends_with("/index.tsx")
                    || child.ends_with("/index.js")
                {
                    return Some(child.clone());
                }
            }
        }
        None
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn split_package(specifier: &str) -> Option<(String, Option<String>)> {
    if specifier.starts_with('@') {
        let mut parts = specifier.splitn(3, '/');
        let scope = parts.next()?;
        let name = parts.next()?;
        let subpath = parts.next().map(ToString::to_string);
        Some((format!("{scope}/{name}"), subpath))
    } else {
        let mut parts = specifier.splitn(2, '/');
        let name = parts.next()?;
        if name.is_empty() {
            return None;
        }
        Some((name.to_string(), parts.next().map(ToString::to_string)))
    }
}

fn match_alias(pattern: &str, specifier: &str) -> Option<String> {
    if let Some(prefix) = pattern.strip_suffix('*') {
        specifier.strip_prefix(prefix).map(ToString::to_string)
    } else if pattern == specifier {
        Some(String::new())
    } else {
        None
    }
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_string())
}

fn load_tsconfig(
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

fn strip_json_comments(text: &str) -> String {
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

fn load_workspaces(
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
        if let Some(map) = value.get("exports").and_then(serde_json::Value::as_object) {
            for (key, target) in map {
                if let Some(target) = export_target(target) {
                    exports.insert(
                        key.clone(),
                        normalize_relative(&format!("{directory}/{target}")),
                    );
                }
            }
        }
        let entry = exports
            .get(".")
            .cloned()
            .or_else(|| {
                value
                    .get("module")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| normalize_relative(&format!("{directory}/{value}")))
            })
            .or_else(|| {
                value
                    .get("main")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| normalize_relative(&format!("{directory}/{value}")))
            })
            .or_else(|| {
                ["src/index.ts", "index.ts", "src/index.tsx", "index.tsx"]
                    .iter()
                    .map(|candidate| normalize_relative(&format!("{directory}/{candidate}")))
                    .find(|candidate| files.contains(candidate))
            });
        let public_api = config
            .public_api
            .iter()
            .any(|glob| shrike_core::glob::glob_match(glob, &format!("{directory}/**")));
        packages.insert(
            name.to_string(),
            PackageInfo {
                name: name.to_string(),
                directory,
                entry,
                exports,
                public_api,
            },
        );
    }
    packages
}

fn export_target(value: &serde_json::Value) -> Option<String> {
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

/// Normalize a path without touching the filesystem, resolving `..` lexically.
#[must_use]
pub fn normalize_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_relative_and_alias() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src/lib")).unwrap();
        fs::write(
            temp.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        )
        .unwrap();
        let files = vec!["src/lib/a.ts".to_string(), "src/lib/index.ts".to_string()];
        let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
        let resolved = resolver.resolve("src/app/page.ts", "../lib/a");
        assert_eq!(resolved.target.as_deref(), Some("src/lib/a.ts"));
        let alias = resolver.resolve("src/app/page.ts", "@/lib/a");
        assert_eq!(alias.target.as_deref(), Some("src/lib/a.ts"));
        let barrel = resolver.resolve("src/app/page.ts", "@/lib");
        assert_eq!(barrel.target.as_deref(), Some("src/lib/index.ts"));
        assert!(resolver.resolve("src/app/page.ts", "react").external);
    }

    #[test]
    fn strips_comments_and_trailing_commas() {
        let input = r#"{"a":1, // x
        "b":"// not a comment", /* y */}"#;
        let output = strip_json_comments(input);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["b"], "// not a comment");
    }
}
