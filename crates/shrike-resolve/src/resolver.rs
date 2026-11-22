use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use shrike_core::{EngineConfig, hash_json, normalize_relative};

use crate::config::{load_tsconfig, load_workspaces};
use crate::types::{PackageInfo, ResolvedSpecifier, Resolver};
use crate::util::{match_alias, parent_dir, split_package};

/// Metro platform suffixes, in resolution order. A neutral import first tries
/// the platform-specific module, then `.native`, then the base module.
const METRO_PLATFORMS: &[&str] = &[
    "ios", "android", "windows", "web", "native", "macos", "default",
];

/// Insert a platform suffix before the source extension:
/// `Button.tsx` -> `Button.ios.tsx`, `Icon` -> `Icon.ios`.
fn platform_candidate(candidate: &str, platform: &str) -> String {
    let Some((stem, extension)) = candidate.rsplit_once('.') else {
        return format!("{candidate}.{platform}");
    };
    if stem.ends_with(".d")
        || !matches!(
            extension,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "json"
        )
    {
        return candidate.to_string();
    }
    format!("{stem}.{platform}.{extension}")
}

/// Whether the specifier already names a platform variant. Metro resolves an
/// explicit `./Button.ios` exactly; only neutral specifiers fan out.
fn names_platform_variant(specifier: &str) -> bool {
    let file = specifier.rsplit('/').next().unwrap_or(specifier);
    let stem = file.rsplit_once('.').map_or(file, |(stem, extension)| {
        if matches!(
            extension,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "json"
        ) {
            stem
        } else {
            file
        }
    });
    METRO_PLATFORMS
        .iter()
        .any(|platform| stem.ends_with(&format!(".{platform}")))
}

/// Remove a platform suffix from a resolved target so all variants of the same
/// base module share one lookup: `Button.ios.tsx` -> `Button.tsx`.
fn platform_base(target: &str) -> String {
    let Some((stem, extension)) = target.rsplit_once('.') else {
        return target.to_string();
    };
    for platform in METRO_PLATFORMS {
        if let Some(base) = stem.strip_suffix(&format!(".{platform}")) {
            return format!("{base}.{extension}");
        }
    }
    target.to_string()
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
                package.directory.is_empty()
                    || file == package.directory
                    || file.starts_with(&format!("{}/", package.directory))
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
            if let Some(resolved) =
                self.resolve_candidate(importer, specifier, &joined, "relative", false)
            {
                return resolved;
            }
            return self.unresolved(importer, specifier, "relative path not found");
        }

        for (pattern, replacements) in &self.paths {
            if let Some(star) = match_alias(pattern, specifier) {
                for replacement in replacements {
                    let candidate = replacement.replace('*', &star);
                    if let Some(resolved) = self.resolve_candidate(
                        importer,
                        specifier,
                        &candidate,
                        "tsconfig-paths",
                        false,
                    ) {
                        return resolved;
                    }
                }
            }
        }

        if let Some(base_url) = &self.base_url {
            let candidate = normalize_relative(&format!("{base_url}/{specifier}"));
            if let Some(resolved) =
                self.resolve_candidate(importer, specifier, &candidate, "tsconfig-baseurl", false)
            {
                return resolved;
            }
        }

        // Framework aliases: `@/x` and `~/x` map to `src/x` by convention.
        for prefix in ["@/", "~/"] {
            if let Some(rest) = specifier.strip_prefix(prefix) {
                let candidate = normalize_relative(&format!("src/{rest}"));
                if let Some(resolved) = self.resolve_candidate(
                    importer,
                    specifier,
                    &candidate,
                    "framework-alias",
                    false,
                ) {
                    return resolved;
                }
            }
        }

        if let Some((name, subpath)) = split_package(specifier) {
            if let Some(package) = self.packages.get(&name) {
                let candidate = if let Some(subpath) = &subpath {
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
                if let Some(resolved) = self.resolve_candidate(
                    importer,
                    specifier,
                    &candidate,
                    "workspace-package",
                    package.metro,
                ) {
                    return resolved;
                }
                // `exports` usually points at published build output, which is
                // absent from the analysis set. Map the specifier back to the
                // conventional source path instead of reporting a false
                // unresolved edge.
                if let Some(subpath) = subpath.as_deref() {
                    for directory in ["src", "source"] {
                        let base = normalize_relative(&format!(
                            "{}/{directory}/{subpath}",
                            package.directory
                        ));
                        if let Some(resolved) = self.resolve_candidate(
                            importer,
                            specifier,
                            &base,
                            "workspace-package-source",
                            package.metro,
                        ) {
                            return resolved;
                        }
                    }
                } else if let Some(entry) = &package.entry {
                    if let Some(resolved) = self.resolve_candidate(
                        importer,
                        specifier,
                        entry,
                        "workspace-package-source",
                        package.metro,
                    ) {
                        return resolved;
                    }
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
                alternatives: Vec::new(),
                external: true,
                reason: "external-package".to_string(),
            };
        }

        self.unresolved(importer, specifier, "bare specifier not found")
    }

    fn resolve_candidate(
        &self,
        importer: &str,
        specifier: &str,
        candidate: &str,
        reason: &str,
        target_metro: bool,
    ) -> Option<ResolvedSpecifier> {
        if let Some(target) = self.probe(candidate) {
            return Some(self.resolved(importer, specifier, target, reason));
        }
        if !target_metro && !self.uses_metro(importer) {
            return None;
        }
        for platform in METRO_PLATFORMS {
            let platform_file = platform_candidate(candidate, platform);
            if let Some(target) = self.probe(&platform_file) {
                return Some(self.resolved(importer, specifier, target, reason));
            }
            // Metro also tries a directory index per platform, so an
            // extensionless import can load `Card/index.ios.tsx` when no base
            // `Card/index.tsx` exists.
            let platform_index = format!("{candidate}/index.{platform}");
            if let Some(target) = self.probe(&platform_index) {
                return Some(self.resolved(importer, specifier, target, reason));
            }
        }
        None
    }

    fn uses_metro(&self, path: &str) -> bool {
        self.package_for(path).is_some_and(|package| package.metro)
    }

    fn resolved(
        &self,
        importer: &str,
        specifier: &str,
        target: String,
        reason: &str,
    ) -> ResolvedSpecifier {
        let alternatives = if names_platform_variant(specifier) {
            Vec::new()
        } else if self.uses_metro(importer) || self.uses_metro(&target) {
            self.platform_variants(&target)
        } else {
            Vec::new()
        };
        ResolvedSpecifier {
            importer: importer.to_string(),
            specifier: specifier.to_string(),
            target: Some(target),
            alternatives,
            external: false,
            reason: reason.to_string(),
        }
    }

    fn platform_variants(&self, target: &str) -> Vec<String> {
        let base = platform_base(target);
        let mut variants = Vec::new();
        for platform in METRO_PLATFORMS {
            let candidate = platform_candidate(&base, platform);
            let Some(path) = self.probe(&candidate) else {
                continue;
            };
            if path != target && !variants.contains(&path) {
                variants.push(path);
            }
        }
        variants
    }

    fn unresolved(&self, importer: &str, specifier: &str, reason: &str) -> ResolvedSpecifier {
        ResolvedSpecifier {
            importer: importer.to_string(),
            specifier: specifier.to_string(),
            target: None,
            alternatives: Vec::new(),
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

    /// Expand a bundler glob pattern (`import.meta.glob`, `require.context`)
    /// to a repository-relative glob. Relative patterns move with the importer;
    /// `/x` is relative to the repository root; tsconfig aliases and the common
    /// `@/`/`~/` source aliases map to their source directories.
    ///
    /// Returns `None` for a pattern that depends on an alias or resolver
    /// feature this resolver does not know, so callers can fail closed instead
    /// of guessing the match set.
    #[must_use]
    pub fn glob_pattern(&self, importer: &str, pattern: &str) -> Option<String> {
        if pattern.starts_with("./")
            || pattern.starts_with("../")
            || pattern == "."
            || pattern == ".."
        {
            return Some(normalize_relative(&format!(
                "{}/{}",
                parent_dir(importer),
                pattern
            )));
        }
        if let Some(rest) = pattern.strip_prefix('/') {
            return Some(normalize_relative(rest));
        }
        // A leading `**` is relative to the glob call's common base. The graph
        // anchors it separately, so keep it repository-relative here.
        if pattern.starts_with("**") {
            return Some(normalize_relative(pattern));
        }
        for (alias, replacements) in &self.paths {
            if let Some(star) = match_alias(alias, pattern) {
                if let Some(replacement) = replacements.first() {
                    return Some(normalize_relative(&replacement.replace('*', &star)));
                }
            }
        }
        for prefix in ["@/", "~/"] {
            if let Some(rest) = pattern.strip_prefix(prefix) {
                return Some(normalize_relative(&format!("src/{rest}")));
            }
        }
        if let Some(base_url) = &self.base_url {
            return Some(normalize_relative(&format!("{base_url}/{pattern}")));
        }
        None
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}
