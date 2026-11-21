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

use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind, PropertyKey};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SourceType;

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
    let Some(source_type) = source_type(path) else {
        return ConfigScan::default();
    };
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return ConfigScan::default();
    }
    let mut config = Config::default();
    config.visit_program(&parsed.program);
    config.scan
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
        .filter_map(|pattern| anchor(package_directory, pattern))
        .collect();
    anchored.sort();
    anchored.dedup();
    anchored
}

/// Anchor a literal Vite-config path at the package root.
#[must_use]
pub(crate) fn vite_config(package_directory: &str, scan: &ConfigScan) -> Option<String> {
    let path = scan.vite_config.as_deref()?;
    anchor(package_directory, path)
}

fn source_type(path: &str) -> Option<SourceType> {
    if path.ends_with(".mts") || path.ends_with(".cts") {
        Some(SourceType::ts().with_module(true))
    } else {
        SourceType::from_path(path).ok()
    }
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

fn string_value(expression: &Expression<'_>) -> Option<String> {
    match unwrap(expression) {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(ToString::to_string),
        _ => None,
    }
}

fn unwrap<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::ParenthesizedExpression(inner) => unwrap(&inner.expression),
        Expression::TSAsExpression(inner) => unwrap(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap(&inner.expression),
        _ => expression,
    }
}

fn find_property<'a>(object: &'a ObjectExpression<'a>, name: &str) -> Option<&'a Expression<'a>> {
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.computed {
            return None;
        }
        let matches = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name == name,
            PropertyKey::StringLiteral(literal) => literal.value == name,
            _ => false,
        };
        matches.then_some(&property.value)
    })
}

/// Anchor one Ladle pattern at the package root. Absolute, negated, and empty
/// patterns are not entries.
fn anchor(package_directory: &str, pattern: &str) -> Option<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.starts_with('/') || pattern.starts_with('!') {
        return None;
    }
    let joined = if package_directory.is_empty() {
        pattern.to_string()
    } else {
        format!("{package_directory}/{pattern}")
    };
    let normalized = normalize(&joined);
    (!normalized.is_empty()).then_some(normalized)
}

/// Collapse `.`/`..` segments lexically. Leading `..` segments are kept; they
/// name a path outside the repository and match nothing.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
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
        assert_eq!(anchor("", "/tmp/stories/**/*.tsx"), None);
        assert_eq!(anchor("", "!src/**/*.tsx"), None);
        assert_eq!(
            anchor("apps/web", "../shared/**/*.tsx").as_deref(),
            Some("apps/shared/**/*.tsx")
        );
    }
}
