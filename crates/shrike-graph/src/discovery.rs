use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use shrike_core::glob::is_included;
use shrike_core::{EngineConfig, content_hash};
use shrike_syntax::{FileSummary, parse_source};

use crate::build_snapshot;
use crate::next_config;
use crate::types::GraphSnapshot;

/// App Router files whose stem is significant. Next.js only treats these
/// names as routes; `app/blog/post.md` is content, not a page. Keep this in
/// sync with the app-entry stem list in `builder::entries`.
const APP_ROUTE_STEMS: &[&str] = &[
    "page",
    "layout",
    "template",
    "loading",
    "error",
    "global-error",
    "not-found",
    "global-not-found",
    "forbidden",
    "unauthorized",
    "route",
    "default",
    "sitemap",
    "robots",
    "manifest",
    "icon",
    "apple-icon",
    "opengraph-image",
    "twitter-image",
];

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

/// The discovered source set plus non-source files whose contents change entry
/// classification. Cache keys must include the latter even though they are not
/// parsed as modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub files: Vec<String>,
    pub cache_inputs: Vec<String>,
}

/// Walk the configured include set. Deterministic order, ignores honoured.
///
/// `.gitignore` is authoritative for generated output. A repository may put
/// its build output in any directory (`distribution`, `esm`, `.output`, ...),
/// so the walker reads the repository's own ignore rules instead of guessing
/// every possible output name. Untracked source files that are not ignored are
/// still scanned.
pub fn discover(root: &Path, config: &EngineConfig) -> Discovery {
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
        // Storybook and Ladle keep configuration and provider modules in a
        // hidden directory. They are executable project files, not editor or
        // tooling metadata, so the walker must descend into them.
        if name.starts_with('.')
            && name != ".well-known"
            && name != ".storybook"
            && name != ".rnstorybook"
            && name != ".ladle"
        {
            return false;
        }
        !OUTPUT_DIRECTORIES.contains(&name)
    });
    let mut files = Vec::new();
    let mut cache_inputs = Vec::new();
    let mut next_configs = Vec::new();
    let mut markdown = Vec::new();
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
        if !is_included(&relative, &[], &config.ignore) {
            continue;
        }
        if next_config::is_config_path(&relative) {
            next_configs.push(relative.clone());
        }
        // Cosmos config is JSON and is not parsed as a module, but it decides
        // which fixture files are entries. Its content must participate in the
        // snapshot cache key even though it is not in the analysis set.
        if relative == "cosmos.config.json" || relative.ends_with("/cosmos.config.json") {
            cache_inputs.push(relative.clone());
        }
        let markdown_file = relative.ends_with(".md") || relative.ends_with(".mdx");
        if markdown_file {
            markdown.push(relative.clone());
        }
        if is_included(&relative, &config.include, &config.ignore) {
            files.push(relative);
        }
    }
    // Next.js `pageExtensions` makes additional suffixes routable. `.md` is not
    // in the default include, so add only the files the configuration turns
    // into routes. Scoping each config to its own directory keeps a root config
    // from absorbing a nested app.
    if !next_configs.is_empty() && !markdown.is_empty() {
        let mut discovered: BTreeSet<String> = files.iter().cloned().collect();
        for config_path in &next_configs {
            let Ok(source) = fs::read_to_string(root.join(config_path)) else {
                continue;
            };
            let extensions = next_config::page_extensions(&source);
            if !extensions.contains("md") && !extensions.contains("mdx") {
                continue;
            }
            let prefix = config_path
                .rsplit_once('/')
                .map_or("", |(directory, _)| directory);
            for path in &markdown {
                if is_next_markdown_route(&extensions, prefix, path) {
                    discovered.insert(path.clone());
                }
            }
        }
        files = discovered.into_iter().collect();
    }
    files.sort();
    files.dedup();
    cache_inputs.sort();
    cache_inputs.dedup();
    Discovery {
        files,
        cache_inputs,
    }
}

/// The analysis source set without the cache-input side channel.
#[must_use]
pub fn discover_files(root: &Path, config: &EngineConfig) -> Vec<String> {
    discover(root, config).files
}

/// Whether `path` is a Next.js route under the config directory `prefix` for
/// one of the configured `pageExtensions`. The Pages Router treats every file
/// as a route; the App Router only recognizes convention stems.
fn is_next_markdown_route(extensions: &BTreeSet<String>, prefix: &str, path: &str) -> bool {
    let relative = if prefix.is_empty() {
        path
    } else {
        let Some(rest) = path.strip_prefix(prefix) else {
            return false;
        };
        let Some(rest) = rest.strip_prefix('/') else {
            return false;
        };
        rest
    };
    let extension = path.rsplit('.').next().unwrap_or("");
    if !extensions.contains(extension) {
        return false;
    }
    if relative.starts_with("pages/") || relative.starts_with("src/pages/") {
        return true;
    }
    for base in ["app/", "src/app/"] {
        if let Some(rest) = relative.strip_prefix(base) {
            let file = rest.rsplit('/').next().unwrap_or(rest);
            let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
            return APP_ROUTE_STEMS.contains(&stem);
        }
    }
    false
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

    #[test]
    fn next_page_extensions_discover_markdown_routes() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            "next.config.ts",
            "const nextConfig = { pageExtensions: ['js', 'jsx', 'ts', 'tsx', 'md', 'mdx'] };\nexport default nextConfig;\n",
        );
        write(root, "app/page.md", "# Home\n");
        write(root, "app/blog/post.md", "# Post\n");
        write(root, "app/dashboard/layout.md", "# Layout\n");
        write(root, "pages/about.md", "# About\n");
        write(root, "src/app/docs/page.md", "# Docs\n");
        write(root, "src/pages/help.md", "# Help\n");
        write(root, "docs/guide.md", "# Guide\n");
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(
            files,
            vec![
                "app/dashboard/layout.md",
                "app/page.md",
                "next.config.ts",
                "pages/about.md",
                "src/app/docs/page.md",
                "src/pages/help.md",
            ]
        );
    }

    #[test]
    fn markdown_routes_require_page_extensions() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            "next.config.ts",
            "const nextConfig = { reactStrictMode: true };\nexport default nextConfig;\n",
        );
        write(root, "app/page.md", "# Home\n");
        write(root, "pages/about.md", "# About\n");
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(files, vec!["next.config.ts"]);

        write(
            root,
            "next.config.ts",
            "const nextConfig = { pageExtensions: ['ts', 'tsx'] };\nexport default nextConfig;\n",
        );
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(
            files,
            vec!["next.config.ts"],
            "markdown must stay out unless its extension is configured"
        );
    }

    #[test]
    fn next_page_extensions_are_scoped_to_the_config_directory() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            "next.config.js",
            "module.exports = { pageExtensions: ['js', 'md'] };\n",
        );
        write(
            root,
            "apps/web/next.config.ts",
            "const nextConfig = { pageExtensions: ['ts', 'tsx', 'md'] };\nexport default nextConfig;\n",
        );
        write(root, "app/page.md", "# Root\n");
        write(root, "apps/web/app/page.md", "# Web\n");
        write(root, "apps/web/pages/about.md", "# About\n");
        write(root, "apps/admin/app/page.md", "# Admin\n");
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(
            files,
            vec![
                "app/page.md",
                "apps/web/app/page.md",
                "apps/web/next.config.ts",
                "apps/web/pages/about.md",
                "next.config.js",
            ]
        );
    }

    #[test]
    fn page_extensions_markdown_honors_config_ignore() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            "next.config.ts",
            "const nextConfig = { pageExtensions: ['ts', 'tsx', 'md'] };\nexport default nextConfig;\n",
        );
        write(root, "app/page.md", "# Home\n");
        write(root, "app/private/page.md", "# Private\n");
        let config = EngineConfig {
            ignore: vec!["app/private/**".into()],
            ..EngineConfig::default()
        };
        let files = discover_files(root, &config);
        assert_eq!(files, vec!["app/page.md", "next.config.ts"]);
    }

    #[test]
    fn storybook_configuration_directories_are_walked() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            ".storybook/main.ts",
            "export default { stories: [\"../src/**/*.stories.tsx\"] };\n",
        );
        write(
            root,
            ".storybook/preview.tsx",
            "export const decorators = [];\n",
        );
        write(
            root,
            ".rnstorybook/main.js",
            "module.exports = { stories: [] };\n",
        );
        write(
            root,
            ".github/workflows/ci.ts",
            "export const ignored = 1;\n",
        );
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(
            files,
            vec![
                ".rnstorybook/main.js",
                ".storybook/main.ts",
                ".storybook/preview.tsx",
            ],
            "Storybook directories are executable project files; other hidden directories stay excluded"
        );
    }

    #[test]
    fn ladle_configuration_directories_are_walked() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            ".ladle/config.mjs",
            "export default { stories: [\"src/**/*.demo.tsx\"] };\n",
        );
        write(
            root,
            ".ladle/components.tsx",
            "export const Provider = () => null;\n",
        );
        write(
            root,
            ".ladle/shell.tsx",
            "export const Shell = () => null;\n",
        );
        write(
            root,
            ".github/workflows/ci.ts",
            "export const ignored = 1;\n",
        );
        let files = discover_files(root, &EngineConfig::default());
        assert_eq!(
            files,
            vec![
                ".ladle/components.tsx",
                ".ladle/config.mjs",
                ".ladle/shell.tsx",
            ],
            "Ladle's provider and config files are executable project files; other hidden directories stay excluded"
        );
    }

    #[test]
    fn cosmos_configuration_is_a_cache_input_not_a_source_file() {
        let directory = tempdir().expect("tempdir");
        let root = directory.path();
        write(
            root,
            "cosmos.config.json",
            r#"{"fixturesDir":"__fixtures__","fixtureFileSuffix":"fixture"}"#,
        );
        write(
            root,
            "apps/web/cosmos.config.json",
            r#"{"fixturesDir":"src/examples"}"#,
        );
        write(root, "src/Button.fixture.tsx", "export default 1;\n");
        write(
            root,
            "apps/web/src/examples/Card.tsx",
            "export const Card = 1;\n",
        );
        let discovery = discover(root, &EngineConfig::default());
        assert_eq!(
            discovery.files,
            vec!["apps/web/src/examples/Card.tsx", "src/Button.fixture.tsx"],
            "Cosmos config is not parsed as a source module"
        );
        assert_eq!(
            discovery.cache_inputs,
            vec!["apps/web/cosmos.config.json", "cosmos.config.json"],
            "config content must participate in the snapshot cache key"
        );
    }
}
