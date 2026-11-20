use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use shrike_core::{EngineConfig, hash_json, normalize_relative};

use crate::config::{load_tsconfig, load_workspaces};
use crate::types::{PackageInfo, ResolvedSpecifier, Resolver};
use crate::util::{match_alias, parent_dir, split_package};

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
                if let Some(target) = self.probe(&candidate) {
                    return self.resolved(importer, specifier, target, "workspace-package");
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
                        if let Some(target) = self.probe(&base) {
                            return self.resolved(
                                importer,
                                specifier,
                                target,
                                "workspace-package-source",
                            );
                        }
                    }
                } else if let Some(entry) = &package.entry {
                    if let Some(target) = self.probe(entry) {
                        return self.resolved(
                            importer,
                            specifier,
                            target,
                            "workspace-package-source",
                        );
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
