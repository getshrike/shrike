//! Static references in test-runner configuration.
//!
//! Vitest, Jest, and Playwright load files that are named only in their
//! configuration: setup modules, global setup/teardown scripts, and alias
//! targets. Those files are entries even though no test imports them by
//! module graph. This module reads string literals, skips comments and
//! interpolated templates, and keeps only literals that resolve to an
//! analyzed source file. A literal that does not match a source file (a
//! reporter name, an output directory, a URL) is ignored.

use std::collections::BTreeSet;
use std::path::Path;

const RUNNERS: &[&str] = &["vitest", "vite", "jest", "playwright"];

/// Whether a path is a test-runner configuration file (`vitest.config.*`,
/// `vitest.integration.config.*`, `vite.config.*`, `jest.config.*`,
/// `playwright.config.*`, and their dotted variants).
#[must_use]
pub(crate) fn is_config_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    if !matches!(extension, "js" | "mjs" | "cjs" | "ts" | "mts" | "cts") {
        return false;
    }
    let Some((name, suffix)) = stem.rsplit_once(".config") else {
        return false;
    };
    if !suffix.is_empty() {
        return false;
    }
    RUNNERS.iter().any(|runner| {
        name.strip_prefix(runner)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

/// Source paths referenced by a test configuration. The result is filtered to
/// files the analysis actually contains, so package names, output directories,
/// and URLs never become entries.
#[must_use]
pub(crate) fn referenced_files(
    root: &Path,
    analyzed: &BTreeSet<&str>,
    config_path: &str,
    source: &str,
) -> Vec<String> {
    let config_dir = config_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut files = BTreeSet::new();
    for literal in path_literals(source) {
        if let Some(path) = resolve_candidate(root, config_dir, &literal, analyzed) {
            files.insert(path);
        }
    }
    files.into_iter().collect()
}

fn resolve_candidate(
    root: &Path,
    config_dir: &str,
    literal: &str,
    analyzed: &BTreeSet<&str>,
) -> Option<String> {
    let literal = literal.trim();
    if literal.is_empty() {
        return None;
    }
    if literal.starts_with('/') {
        let relative = Path::new(literal)
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        return analyzed.contains(relative.as_str()).then_some(relative);
    }
    let relative_to_config = if config_dir.is_empty() {
        literal.to_string()
    } else {
        format!("{config_dir}/{literal}")
    };
    // `setupFiles: ["./tests/setup.ts"]` is resolved from the project root by
    // Vitest, even when the config lives in `config/`. Try the config directory
    // first (the common case), then the repository root.
    for candidate in [relative_to_config, literal.to_string()] {
        if let Some(normalized) = normalize(&candidate) {
            if analyzed.contains(normalized.as_str()) {
                return Some(normalized);
            }
        }
    }
    None
}

fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let normalized = path.replace('\\', "/");
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// String literals that can name a local path. Comments and interpolated
/// template literals are skipped; a bare package name or URL is not a path.
fn path_literals(source: &str) -> Vec<String> {
    let mut literals = Vec::new();
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
                    match character {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                value.push(escaped);
                            }
                        }
                        '$' if quote == '`' && chars.peek() == Some(&'{') => {
                            interpolation = true;
                        }
                        character if character == quote => break,
                        character => value.push(character),
                    }
                }
                if !interpolation && looks_like_path(&value) {
                    literals.push(value);
                }
            }
            _ => {}
        }
    }
    literals
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with('/')
        || (value.contains('/')
            && value.rsplit_once('.').is_some_and(|(_, extension)| {
                matches!(
                    extension,
                    "js" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" | "jsx"
                )
            }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::{is_config_path, referenced_files};

    #[test]
    fn recognizes_test_runner_configs() {
        for path in [
            "vitest.config.ts",
            "vitest.integration.config.ts",
            "config/vitest.config.mts",
            "vite.config.js",
            "jest.config.cjs",
            "playwright.config.ts",
            "playwright.screenshots.config.ts",
        ] {
            assert!(is_config_path(path), "{path}");
        }
        for path in [
            "vite-env.d.ts",
            "vitest.shared.ts",
            "next.config.ts",
            "src/testing.ts",
        ] {
            assert!(!is_config_path(path), "{path}");
        }
    }

    #[test]
    fn resolves_config_dir_and_root_references() {
        let analyzed: BTreeSet<&str> = [
            "tests/setup.ts",
            "tests/stubs/server-only.ts",
            "app/config/setup.ts",
        ]
        .into_iter()
        .collect();
        let source = r#"
            // setupFiles: ['./commented-out.ts']
            export default {
              setupFiles: ['./tests/setup.ts', `${dynamic}/setup.ts`],
              alias: { "server-only": path.resolve(__dirname, "tests/stubs/server-only.ts") },
              ignore: ["nope.ts"]
            };
        "#;
        let files = referenced_files(
            Path::new("/repo"),
            &analyzed,
            "config/vitest.config.ts",
            source,
        );
        assert_eq!(
            files,
            vec![
                "tests/setup.ts".to_string(),
                "tests/stubs/server-only.ts".to_string()
            ]
        );
    }
}
