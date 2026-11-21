//! Normalized engine configuration.
//!
//! The TypeScript host evaluates `shrike.config.ts` and sends this shape to the
//! engine. Defaults are documented in `schemas/config.schema.json` and are
//! applied here as well so the engine is usable directly.

use serde::{Deserialize, Serialize};

use crate::hash::hash_json;

/// Whether test, storybook, and fixture files are entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryPolicy {
    /// Tests, fixtures, and local tooling are entries. Safe for scans and hooks.
    #[default]
    Local,
    /// Production reachability. Test-only entries are excluded and their
    /// dependencies may become candidates.
    Production,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BatchPolicy {
    pub max_symbols: u32,
    pub max_files: u32,
    pub max_live_graph_percent: f64,
    pub allow_oversized: bool,
}

impl Default for BatchPolicy {
    fn default() -> Self {
        Self {
            max_symbols: 50,
            max_files: 10,
            max_live_graph_percent: 5.0,
            allow_oversized: false,
        }
    }
}

/// How a `"use client"` directive affects entry seeding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientBoundaryPolicy {
    /// The directive marks a boundary, but liveness comes from the module
    /// graph like any other file. Unused client boundaries are reported.
    #[default]
    Graph,
    /// Every export in a client boundary file is an entry. Maximum caution for
    /// frameworks that discover boundaries outside the import graph; hides
    /// genuinely unused client-boundary exports.
    Protect,
}

/// How a module-level `"use server"` directive affects entry seeding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerActionPolicy {
    /// Server actions are ordinary exports. An action that no project module
    /// imports or references is reported, matching knip and fallow. Build
    /// evidence can still mark a referenced action live.
    #[default]
    Graph,
    /// Every export in a `"use server"` module is an entry, because Next.js
    /// registers actions independently of imports. Hides actions that have no
    /// project reference but remain POST-able if their id leaks.
    Protect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineConfig {
    /// Repository-relative globs to analyze.
    pub include: Vec<String>,
    /// Repository-relative globs to skip. Ignore wins over include.
    pub ignore: Vec<String>,
    /// Path to `tsconfig.json`, relative to the repository root.
    pub tsconfig: Option<String>,
    /// Entry policy for tests and local tooling.
    pub entry_policy: EntryPolicy,
    /// Include test, storybook, and fixture entries.
    pub test_entries: bool,
    /// Treat generated files as deletion candidates. Default false: generated
    /// files are parsed for edges but never proposed for deletion.
    pub include_generated: bool,
    /// Client/server boundary entry policy.
    pub client_boundary_policy: ClientBoundaryPolicy,
    /// Server-action entry policy.
    pub server_action_policy: ServerActionPolicy,
    /// Extra globs whose exports are public API and never deletion candidates.
    pub public_api: Vec<String>,
    /// Extra globs treated as generated.
    pub generated: Vec<String>,
    /// Batch caps and blast-radius policy.
    pub batch_policy: BatchPolicy,
    /// Framework adapters to enable. Unknown names are ignored with a warning.
    pub entry_adapters: Vec<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            include: vec![
                "**/*.ts".into(),
                "**/*.tsx".into(),
                "**/*.js".into(),
                "**/*.jsx".into(),
                "**/*.mjs".into(),
                "**/*.cjs".into(),
                "**/*.mdx".into(),
                "**/*.svelte".into(),
                "**/*.vue".into(),
                "**/*.astro".into(),
                // Vite and static hosts load module scripts from the document.
                // The parser records those script sources as imports.
                "**/index.html".into(),
            ],
            ignore: vec![
                "node_modules/**".into(),
                ".git/**".into(),
                ".next/**".into(),
                ".shrike/**".into(),
                "dist/**".into(),
                "build/**".into(),
                "out/**".into(),
                "coverage/**".into(),
                ".turbo/**".into(),
                ".vercel/**".into(),
                "*.min.js".into(),
                "*.generated.*".into(),
            ],
            tsconfig: Some("tsconfig.json".into()),
            entry_policy: EntryPolicy::Local,
            test_entries: true,
            include_generated: false,
            client_boundary_policy: ClientBoundaryPolicy::Graph,
            server_action_policy: ServerActionPolicy::Graph,
            public_api: Vec::new(),
            generated: Vec::new(),
            batch_policy: BatchPolicy::default(),
            entry_adapters: vec![
                "next-app".into(),
                "next-pages".into(),
                "react".into(),
                "packages".into(),
                "tests".into(),
                "configs".into(),
            ],
        }
    }
}

impl EngineConfig {
    /// Validate the configuration and return a human-readable problem.
    pub fn validate(&self) -> Result<(), String> {
        if self.include.is_empty() {
            return Err("include must contain at least one glob".into());
        }
        if self.batch_policy.max_symbols == 0 {
            return Err("batchPolicy.maxSymbols must be greater than zero".into());
        }
        if self.batch_policy.max_files == 0 {
            return Err("batchPolicy.maxFiles must be greater than zero".into());
        }
        if !(0.0..=100.0).contains(&self.batch_policy.max_live_graph_percent) {
            return Err("batchPolicy.maxLiveGraphPercent must be between 0 and 100".into());
        }
        if self.tsconfig.as_deref().is_some_and(str::is_empty) {
            return Err("tsconfig must be a path or null".into());
        }
        Ok(())
    }

    /// Content hash of the normalized configuration.
    #[must_use]
    pub fn config_hash(&self) -> String {
        hash_json(self)
    }

    #[must_use]
    pub fn adapter_enabled(&self, name: &str) -> bool {
        self.entry_adapters.iter().any(|adapter| adapter == name)
    }
}

/// Normalize a repository-relative path to `/` separators, remove `.` segments
/// and resolve `..` lexically. The function never touches the filesystem.
#[must_use]
pub fn normalize_relative(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_stable() {
        let config = EngineConfig::default();
        assert_eq!(config.batch_policy.max_symbols, 50);
        assert_eq!(config.batch_policy.max_files, 10);
        assert!(config.test_entries);
        assert!(!config.include_generated);
        assert_eq!(config.client_boundary_policy, ClientBoundaryPolicy::Graph);
        assert_eq!(config.server_action_policy, ServerActionPolicy::Graph);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_hash_changes_with_policy() {
        let a = EngineConfig::default();
        let mut b = a.clone();
        b.batch_policy.max_symbols = 10;
        assert_ne!(a.config_hash(), b.config_hash());
        assert_eq!(a.config_hash(), a.clone().config_hash());
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize_relative("./src//a/../b.ts"), "src/b.ts");
        assert_eq!(normalize_relative("src\\lib\\x.ts"), "src/lib/x.ts");
        assert_eq!(normalize_relative("../outside.ts"), "../outside.ts");
    }
}
