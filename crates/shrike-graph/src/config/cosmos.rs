//! React Cosmos fixture and decorator conventions.
//!
//! Cosmos discovers user modules by convention instead of importing them:
//!
//! - fixture files inside a `fixturesDir` directory (default `__fixtures__`);
//! - files whose name ends with `.<fixtureFileSuffix>.*` or is exactly
//!   `<fixtureFileSuffix>.*` (default `fixture`);
//! - `cosmos.decorator.*` modules.
//!
//! The options live in `cosmos.config.json` at the project root. Knip executes
//! the config-aware plugin in Node; Shrike reads the literal JSON values and
//! stays static. Invalid, dynamic, or unsupported values fall back to the
//! documented defaults, which are also what Cosmos uses.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde_json::Value;

const DEFAULT_FIXTURES_DIR: &str = "__fixtures__";
const DEFAULT_FIXTURE_SUFFIX: &str = "fixture";
const FIXTURE_EXTENSIONS: &str = "{js,jsx,ts,tsx,md,mdx}";

/// Literal Cosmos options that change which modules are user modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CosmosConfig {
    /// `None` when the config sets a value Shrike cannot prove statically.
    fixtures_dir: Option<String>,
    fixture_suffix: String,
    ignore: BTreeSet<String>,
}

impl Default for CosmosConfig {
    fn default() -> Self {
        Self {
            fixtures_dir: Some(DEFAULT_FIXTURES_DIR.to_string()),
            fixture_suffix: DEFAULT_FIXTURE_SUFFIX.to_string(),
            // Cosmos always prepends this pattern to the user's ignore list.
            ignore: BTreeSet::from(["**/node_modules/**".to_string()]),
        }
    }
}

impl CosmosConfig {
    /// Read `cosmos.config.json` from a workspace package. A missing or
    /// malformed file uses the documented defaults.
    #[must_use]
    pub(crate) fn load(root: &Path, package_directory: &str) -> Self {
        let path = root.join(package_directory).join("cosmos.config.json");
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Self::default();
        };
        Self::from_value(&value)
    }

    fn from_value(value: &Value) -> Self {
        let mut config = Self::default();
        if let Some(fixtures_dir) = value.get("fixturesDir").and_then(Value::as_str) {
            config.fixtures_dir = normalize_fixtures_dir(fixtures_dir);
        }
        if let Some(suffix) = value.get("fixtureFileSuffix").and_then(Value::as_str) {
            config.fixture_suffix = normalize_fixture_suffix(suffix)
                .unwrap_or_else(|| DEFAULT_FIXTURE_SUFFIX.to_string());
        }
        if let Some(ignore) = value.get("ignore").and_then(Value::as_array) {
            config.ignore.extend(
                ignore
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(normalize_ignore_pattern),
            );
        }
        config
    }
}

/// A directory name or relative path. Cosmos interpolates this into
/// `**/<fixturesDir>/**`, so a path such as `src/examples` is matched as a
/// path segment sequence at any depth. Absolute paths, parent traversal, and
/// glob syntax are not statically provable and disable the directory rule.
fn normalize_fixtures_dir(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part if part.contains(['*', '?', '[', ']', '{', '}', '!', '\\']) => return None,
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A basename suffix without a dot. Cosmos supports any literal suffix, but
/// path separators and glob metacharacters would turn a literal convention into
/// an unbounded match, so those fall back to the default.
fn normalize_fixture_suffix(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('.');
    if value.is_empty() || value.contains(['/', '\\', '*', '?', '[', ']', '{', '}', '!', ' ']) {
        return None;
    }
    Some(value.to_string())
}

/// Ignore patterns are relative to the Cosmos project root. Negated patterns
/// are skipped: not ignoring a file can only protect more modules, which is the
/// conservative direction.
fn normalize_ignore_pattern(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with(['!', '/']) || value.contains("..") {
        return None;
    }
    Some(value.to_string())
}

/// Compiled Cosmos user-module matchers for one package.
pub(crate) struct Matcher {
    fixtures: Option<GlobSet>,
    decorators: Option<GlobSet>,
    ignore: Option<GlobSet>,
}

impl Matcher {
    #[must_use]
    pub(crate) fn is_fixture(&self, path: &str) -> bool {
        !self.is_ignored(path) && self.fixtures.as_ref().is_some_and(|set| set.is_match(path))
    }

    #[must_use]
    pub(crate) fn is_decorator(&self, path: &str) -> bool {
        !self.is_ignored(path)
            && self
                .decorators
                .as_ref()
                .is_some_and(|set| set.is_match(path))
    }

    fn is_ignored(&self, path: &str) -> bool {
        self.ignore.as_ref().is_some_and(|set| set.is_match(path))
    }
}

/// Build the package-scoped matcher. `package_directory` is repo-relative and
/// empty for the root package.
#[must_use]
pub(crate) fn matcher(package_directory: &str, config: &CosmosConfig) -> Option<Matcher> {
    let prefix = if package_directory.is_empty() {
        String::new()
    } else {
        format!("{package_directory}/")
    };
    let mut fixture_patterns = Vec::new();
    if let Some(directory) = &config.fixtures_dir {
        // Cosmos's own pattern: **/<fixturesDir>/**/*.ext. Anchoring the
        // leading ** at the package root keeps a repository-level package from
        // absorbing a sibling package.
        fixture_patterns.push(format!("{prefix}**/{directory}/**/*.{FIXTURE_EXTENSIONS}"));
    }
    fixture_patterns.push(format!(
        "{prefix}**/*.{}.{FIXTURE_EXTENSIONS}",
        config.fixture_suffix
    ));
    fixture_patterns.push(format!(
        "{prefix}**/{}.{FIXTURE_EXTENSIONS}",
        config.fixture_suffix
    ));
    let fixtures = glob_set(&fixture_patterns);
    let decorators = glob_set(&[format!("{prefix}**/cosmos.decorator.{{js,jsx,ts,tsx}}")]);
    let ignore_patterns: Vec<String> = config
        .ignore
        .iter()
        .map(|pattern| format!("{prefix}{pattern}"))
        .collect();
    let ignore = glob_set(&ignore_patterns);
    if fixtures.is_none() && decorators.is_none() {
        return None;
    }
    Some(Matcher {
        fixtures,
        decorators,
        ignore,
    })
}

fn glob_set(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let Ok(glob) = GlobBuilder::new(pattern)
            .literal_separator(true)
            .empty_alternates(true)
            .build()
        else {
            continue;
        };
        builder.add(glob);
    }
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_cosmos_conventions() {
        let matcher = matcher("", &CosmosConfig::default()).expect("matcher");
        for path in [
            "src/Button.fixture.tsx",
            "src/Button/fixture.tsx",
            "src/__fixtures__/Button.tsx",
            "src/components/__fixtures__/Card/blank.tsx",
            "src/Button.fixture.mdx",
            "src/cosmos.decorator.tsx",
            "src/cosmos.decorator.js",
        ] {
            assert!(
                matcher.is_fixture(path) || matcher.is_decorator(path),
                "{path} is a Cosmos user module"
            );
        }
        for path in [
            "src/Button.tsx",
            "src/fixtures/Button.tsx",
            "src/stories/Button.stories.tsx",
        ] {
            assert!(
                !matcher.is_fixture(path) && !matcher.is_decorator(path),
                "{path} is not a Cosmos user module"
            );
        }
    }

    #[test]
    fn custom_directory_and_suffix_replace_the_defaults() {
        let config = CosmosConfig::from_value(&serde_json::json!({
            "fixturesDir": "src/examples",
            "fixtureFileSuffix": "example",
        }));
        let matcher = matcher("", &config).expect("matcher");
        for path in [
            "src/examples/Widget.tsx",
            "src/deep/src/examples/Widget.tsx",
            "src/Widget.example.tsx",
            "src/example.tsx",
            "src/cosmos.decorator.jsx",
        ] {
            assert!(
                matcher.is_fixture(path) || matcher.is_decorator(path),
                "{path} uses the configured conventions"
            );
        }
        for path in [
            "src/__fixtures__/Widget.tsx",
            "src/Widget.fixture.tsx",
            "src/fixture.tsx",
        ] {
            assert!(
                !matcher.is_fixture(path) && !matcher.is_decorator(path),
                "{path} uses a convention the config replaced"
            );
        }
    }

    #[test]
    fn fixture_files_are_package_scoped() {
        let matcher = matcher("apps/web", &CosmosConfig::default()).expect("matcher");
        assert!(matcher.is_fixture("apps/web/src/Button.fixture.tsx"));
        assert!(!matcher.is_fixture("apps/admin/src/Button.fixture.tsx"));
        assert!(!matcher.is_fixture("src/Button.fixture.tsx"));
    }

    #[test]
    fn ignore_patterns_are_honored() {
        let config = CosmosConfig::from_value(&serde_json::json!({
            "ignore": ["**/*.skip.*"],
        }));
        let matcher = matcher("", &config).expect("matcher");
        assert!(matcher.is_fixture("src/Button.fixture.tsx"));
        assert!(!matcher.is_fixture("src/Button.skip.fixture.tsx"));
        assert!(!matcher.is_fixture("src/examples/Thing.skip.tsx"));
    }

    #[test]
    fn unprovable_values_fall_back_or_disable_the_directory_rule() {
        let config = CosmosConfig::from_value(&serde_json::json!({
            "fixturesDir": "../shared/fixtures",
            "fixtureFileSuffix": "bad/suffix",
        }));
        assert_eq!(config.fixtures_dir, None);
        assert_eq!(config.fixture_suffix, DEFAULT_FIXTURE_SUFFIX);
        let matcher = matcher("", &config).expect("matcher");
        assert!(matcher.is_fixture("src/Button.fixture.tsx"));
        assert!(!matcher.is_fixture("src/__fixtures__/Button.tsx"));
    }

    #[test]
    fn config_reads_a_package_file_and_defaults_on_bad_json() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path();
        std::fs::create_dir_all(root.join("apps/web")).expect("package dir");
        std::fs::write(
            root.join("apps/web/cosmos.config.json"),
            r#"{"fixturesDir":"examples","fixtureFileSuffix":"demo"}"#,
        )
        .expect("config");
        let config = CosmosConfig::load(root, "apps/web");
        assert_eq!(config.fixtures_dir.as_deref(), Some("examples"));
        assert_eq!(config.fixture_suffix, "demo");

        std::fs::write(root.join("apps/web/cosmos.config.json"), "{broken").expect("bad config");
        assert_eq!(
            CosmosConfig::load(root, "apps/web"),
            CosmosConfig::default()
        );
    }
}
