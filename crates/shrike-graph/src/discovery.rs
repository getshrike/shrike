use std::path::Path;

use shrike_core::glob::is_included;
use shrike_core::{EngineConfig, content_hash};
use shrike_syntax::{FileSummary, parse_source};

use crate::build_snapshot;
use crate::types::GraphSnapshot;

/// Analyze source files from disk. Used by the engine and tests.
pub fn analyze_paths(root: &Path, config: &EngineConfig, paths: &[String]) -> GraphSnapshot {
    let summaries: Vec<FileSummary> = paths
        .iter()
        .filter_map(|path| {
            let full = root.join(path);
            std::fs::read_to_string(full)
                .ok()
                .map(|source| parse_source(path, &source))
        })
        .collect();
    build_snapshot(root, summaries, config)
}

/// Walk the configured include set. Deterministic order, ignores honoured.
///
/// `.gitignore` is authoritative for generated output. A repository may put
/// its build output in any directory (`distribution`, `esm`, `.output`, ...),
/// so the walker reads the repository's own ignore rules instead of guessing
/// every possible output name. Untracked source files that are not ignored are
/// still scanned.
pub fn discover_files(root: &Path, config: &EngineConfig) -> Vec<String> {
    const OUTPUT_DIRECTORIES: &[&str] = &[
        "node_modules",
        ".git",
        ".next",
        "dist",
        "build",
        "out",
        "coverage",
        ".turbo",
        ".vercel",
        ".shrike",
    ];
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false);
    builder.filter_entry(|entry| {
        let Some(name) = entry.file_name().to_str() else {
            return true;
        };
        if name.starts_with('.') && name != ".well-known" {
            return false;
        }
        !OUTPUT_DIRECTORIES.contains(&name)
    });
    let mut files = Vec::new();
    for entry in builder.build().flatten() {
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            continue;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        if is_included(&relative, &config.include, &config.ignore) {
            files.push(relative);
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Compute a content hash over all file hashes (cache key).
#[must_use]
pub fn input_hash(summaries: &[FileSummary]) -> String {
    let mut parts: Vec<String> = summaries
        .iter()
        .map(|summary| format!("{}:{}", summary.path, summary.content_hash))
        .collect();
    parts.sort();
    content_hash(&parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(root: &Path, path: &str, content: &str) {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(full, content).expect("write fixture");
    }

    #[test]
    fn honors_root_gitignore_for_build_output() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(root, ".gitignore", "/distribution\n");
        write(root, "src/a.ts", "export const a = 1;\n");
        write(root, "distribution/index.js", "export const built = 2;\n");
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(files, vec!["src/a.ts"]);
    }

    #[test]
    fn honors_nested_gitignore_and_negation() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(root, ".gitignore", "*.ts\n!src/keep.ts\n");
        write(root, "src/keep.ts", "export const keep = 1;\n");
        write(root, "src/drop.ts", "export const drop = 1;\n");
        write(root, "src/.gitignore", "generated/\n");
        write(root, "src/generated/b.ts", "export const generated = 1;\n");
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(files, vec!["src/keep.ts"]);
    }

    #[test]
    fn keeps_common_output_directories_even_without_gitignore() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(root, "src/a.ts", "export const a = 1;\n");
        write(root, "dist/a.js", "export const built = 2;\n");
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(files, vec!["src/a.ts"]);
    }
}
