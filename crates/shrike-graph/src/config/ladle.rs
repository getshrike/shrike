//! Static Ladle configuration reading.
//!
//! Ladle discovers stories from `src/**/*.stories.{js,jsx,ts,tsx,mdx}` by
//! default, and loads `.ladle/components.{js,jsx,ts,tsx}` as the provider entry.
//! A `.ladle/config.*` file can replace the story globs with a literal string or
//! an array of strings, and can point at a Vite config that Ladle executes.
//!
//! Knip executes the config in Node. Shrike stays static: it parses the file
//! with oxc and reads literal string values. Computed story values fall back to
//! the default glob so a dynamic config cannot erase story protection, while
//! computed Vite paths are skipped.

use oxc_ast::ast::{Expression, ObjectExpression};
use oxc_ast_visit::{Visit, walk};

use super::{anchor_pattern, find_property, source_type, string_value, unwrap, with_config_ast};

/// Ladle's default story glob, relative to the package root.
pub(crate) const DEFAULT_STORIES: &str = "src/**/*.stories.{js,jsx,ts,tsx,mdx}";

/// Literal configuration values that affect Ladle's module graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConfigScan {
    /// `stories` patterns when the option is a literal string or array.
    pub stories: Option<Vec<String>>,
    /// Literal `viteConfig` path relative to the package root.
    pub vite_config: Option<String>,
}

/// Whether `path` is a Ladle config file.
#[must_use]
pub(crate) fn is_config_path(path: &str) -> bool {
    let Some((directory, file)) = path.rsplit_once('/') else {
        return false;
    };
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    directory.ends_with(".ladle")
        && stem == "config"
        && matches!(extension, "js" | "mjs" | "cjs" | "ts" | "mts" | "cts")
}

/// Whether `path` is Ladle's components file. Ladle imports every export of
/// this module as part of the story provider surface.
#[must_use]
pub(crate) fn is_components_path(path: &str) -> bool {
    let Some((directory, file)) = path.rsplit_once('/') else {
        return false;
    };
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    directory.ends_with(".ladle")
        && stem == "components"
        && matches!(extension, "js" | "jsx" | "ts" | "tsx")
}

/// The package directory that owns a `.ladle` file, or `None` when `path` is
/// not inside a Ladle directory.
#[must_use]
pub(crate) fn package_directory(path: &str) -> Option<&str> {
    let (directory, _) = path.rsplit_once('/')?;
    if directory == ".ladle" {
        Some("")
    } else {
        directory.strip_suffix("/.ladle")
    }
}

/// Read literal `stories` and `viteConfig` values from a Ladle config.
#[must_use]
pub(crate) fn scan(path: &str, source: &str) -> ConfigScan {
    let Some(kind) = source_type(path) else {
        return ConfigScan::default();
    };
    with_config_ast(kind, source, |program| {
        let mut config = Config::default();
        config.visit_program(program);
        config.scan
    })
    .unwrap_or_default()
}

/// Story patterns for one package: the config's literal `stories` values when
/// present, otherwise Ladle's default glob. Patterns are anchored at the
/// package root.
#[must_use]
pub(crate) fn story_patterns(package_directory: &str, scan: &ConfigScan) -> Vec<String> {
    let patterns: Vec<&str> = scan.stories.as_ref().map_or_else(
        || vec![DEFAULT_STORIES],
        |stories| stories.iter().map(String::as_str).collect(),
    );
    let mut anchored: Vec<String> = patterns
        .iter()
        .filter_map(|pattern| anchor_pattern(package_directory, pattern))
        .collect();
    anchored.sort();
    anchored.dedup();
    anchored
}

/// Anchor a literal Vite-config path at the package root.
#[must_use]
pub(crate) fn vite_config(package_directory: &str, scan: &ConfigScan) -> Option<String> {
    let path = scan.vite_config.as_deref()?;
    anchor_pattern(package_directory, path)
}

#[derive(Default)]
struct Config {
    scan: ConfigScan,
}

impl<'a> Visit<'a> for Config {
    fn visit_object_expression(&mut self, object: &ObjectExpression<'a>) {
        if self.scan.stories.is_none()
            && let Some(stories) = find_property(object, "stories")
        {
            self.scan.stories = collect_stories(stories);
        }
        if self.scan.vite_config.is_none()
            && let Some(vite_config) = find_property(object, "viteConfig")
        {
            self.scan.vite_config = string_value(vite_config);
        }
        walk::walk_object_expression(self, object);
    }
}

fn collect_stories(expression: &Expression<'_>) -> Option<Vec<String>> {
    match unwrap(expression) {
        Expression::StringLiteral(literal) => Some(vec![literal.value.to_string()]),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(|value| vec![value.to_string()]),
        Expression::ArrayExpression(array) => Some(
            array
                .elements
                .iter()
                .filter_map(|element| element.as_expression())
                .filter_map(string_value)
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_string_and_array_story_globs() {
        let source = r#"
            export default {
              stories: ["src/**/*.stories.tsx", "src/**/*.demo.tsx"],
            };
        "#;
        let config_scan = scan(".ladle/config.ts", source);
        assert_eq!(
            story_patterns("", &config_scan),
            vec!["src/**/*.demo.tsx", "src/**/*.stories.tsx"]
        );

        let single = r#"
            const config = { stories: "packages/ui/src/**/*.demo.tsx" };
            export default config;
        "#;
        let config_scan = scan("apps/web/.ladle/config.mjs", single);
        assert_eq!(
            story_patterns("apps/web", &config_scan),
            vec!["apps/web/packages/ui/src/**/*.demo.tsx"]
        );
    }

    #[test]
    fn missing_or_computed_stories_fall_back_to_the_default_glob() {
        let empty = scan(".ladle/config.ts", "export default { addons: {} };");
        assert_eq!(empty.stories, None);
        assert_eq!(
            story_patterns("", &empty),
            vec!["src/**/*.stories.{js,jsx,ts,tsx,mdx}"]
        );

        let computed = scan(
            ".ladle/config.ts",
            "const stories = makeStories();\nexport default { stories };",
        );
        assert_eq!(computed.stories, None);
        assert_eq!(
            story_patterns("", &computed),
            vec!["src/**/*.stories.{js,jsx,ts,tsx,mdx}"]
        );
    }

    #[test]
    fn reads_literal_vite_config_paths() {
        let config_scan = scan(
            "apps/web/.ladle/config.mjs",
            "export default { viteConfig: './vite.config.ts' };",
        );
        assert_eq!(
            vite_config("apps/web", &config_scan).as_deref(),
            Some("apps/web/vite.config.ts")
        );

        let dynamic = scan(
            ".ladle/config.ts",
            "export default { viteConfig: pickConfig() };",
        );
        assert_eq!(vite_config("", &dynamic), None);
    }

    #[test]
    fn recognizes_ladle_files_and_package_roots() {
        assert!(is_config_path(".ladle/config.mjs"));
        assert!(is_config_path("apps/web/.ladle/config.ts"));
        assert!(!is_config_path(".ladle/config.json"));
        assert!(!is_config_path("config.ts"));

        assert!(is_components_path(".ladle/components.tsx"));
        assert!(is_components_path("apps/web/.ladle/components.js"));
        assert!(!is_components_path(".ladle/shell.tsx"));

        assert_eq!(package_directory(".ladle/config.mjs"), Some(""));
        assert_eq!(
            package_directory("apps/web/.ladle/components.tsx"),
            Some("apps/web")
        );
        assert_eq!(package_directory("src/button.stories.tsx"), None);
    }

    #[test]
    fn skips_absolute_and_negated_patterns() {
        assert_eq!(anchor_pattern("", "/tmp/stories/**/*.tsx"), None);
        assert_eq!(anchor_pattern("", "!src/**/*.tsx"), None);
        assert_eq!(
            anchor_pattern("apps/web", "../shared/**/*.tsx").as_deref(),
            Some("apps/shared/**/*.tsx")
        );
    }
}
