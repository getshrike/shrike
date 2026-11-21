//! Static Storybook main-configuration reading.
//!
//! Storybook's `stories` option decides which files are stories. The default
//! glob covers `*.stories.*` and every `.mdx` file, but projects commonly
//! narrow or rename the pattern, for example `../src/**/*.demo.tsx` or an
//! object specifier `{ directory: "../src", files: "**/*.preview.tsx" }`.
//!
//! Knip executes the main configuration in Node. Shrike stays static: it parses
//! the file with oxc, reads literal string arrays, and follows `{ directory,
//! files }` object specifiers. Functions and computed values are skipped, which
//! leaves the conventional story suffix coverage in place rather than inventing
//! entries.
//!
//! Storybook resolves every pattern relative to the directory that holds the
//! main configuration (`.storybook` or `.rnstorybook`). Patterns support
//! extglob alternation such as `*.@(js|ts)`; those are rewritten to the brace
//! alternation understood by `globset`.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind, PropertyKey};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SourceType;

/// Storybook applies this glob to a `{ directory }` specifier without `files`.
const DEFAULT_SPECIFIER_FILES: &str = "**/*.@(mdx|stories.@(mdx|js|jsx|mjs|ts|tsx))";

/// Whether `path` is a Storybook main configuration. Only `main` carries the
/// `stories` option; preview, manager, and test-runner files are separate
/// concerns and are already entries through their directory.
#[must_use]
pub(crate) fn is_config_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    if stem != "main" || !matches!(extension, "js" | "ts" | "mjs" | "cjs" | "mts" | "cts") {
        return false;
    }
    let directory = path.rsplit_once('/').map_or("", |(directory, _)| directory);
    directory.ends_with(".storybook") || directory.ends_with(".rnstorybook")
}

/// Story patterns declared by a main configuration, relative to the repository
/// root. Patterns that are absolute, negated, or computed are skipped.
#[must_use]
pub(crate) fn story_patterns(config_path: &str, source: &str) -> Vec<String> {
    let Some(source_type) = source_type(config_path) else {
        return Vec::new();
    };
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Vec::new();
    }
    let mut stories = Stories::default();
    stories.visit_program(&parsed.program);
    let directory = config_path
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let mut patterns: Vec<String> = stories
        .patterns
        .iter()
        .filter_map(|pattern| anchor(directory, pattern))
        .collect();
    patterns.sort();
    patterns.dedup();
    patterns
}

/// Compile story patterns into one matcher. `None` means no usable pattern.
#[must_use]
pub(crate) fn matcher(patterns: &[String]) -> Option<GlobSet> {
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let rewritten = rewrite_extglob(pattern);
        let Ok(glob) = GlobBuilder::new(&rewritten)
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

fn source_type(path: &str) -> Option<SourceType> {
    if path.ends_with(".mts") || path.ends_with(".cts") {
        Some(SourceType::ts().with_module(true))
    } else {
        SourceType::from_path(path).ok()
    }
}

#[derive(Default)]
struct Stories {
    patterns: Vec<String>,
}

impl<'a> Visit<'a> for Stories {
    fn visit_object_expression(&mut self, object: &ObjectExpression<'a>) {
        if let Some(stories) = find_property(object, "stories") {
            collect_stories(stories, &mut self.patterns, 0);
        }
        walk::walk_object_expression(self, object);
    }
}

fn collect_stories(expression: &Expression<'_>, patterns: &mut Vec<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    match unwrap(expression) {
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(expression) = element.as_expression() {
                    collect_stories(expression, patterns, depth + 1);
                }
            }
        }
        Expression::StringLiteral(literal) => patterns.push(literal.value.to_string()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            if let Some(value) = template
                .quasis
                .first()
                .and_then(|quasi| quasi.value.cooked.as_ref())
            {
                patterns.push(value.to_string());
            }
        }
        Expression::ObjectExpression(object) => {
            let Some(directory) = find_property(object, "directory").and_then(string_value) else {
                return;
            };
            let files = find_property(object, "files")
                .and_then(string_value)
                .unwrap_or_else(|| DEFAULT_SPECIFIER_FILES.to_string());
            patterns.push(format!("{directory}/{files}"));
        }
        _ => {}
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

/// Anchor one Storybook pattern at the main-config directory. Absolute,
/// negated, and empty patterns are not entries.
fn anchor(directory: &str, pattern: &str) -> Option<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.starts_with('/') || pattern.starts_with('!') {
        return None;
    }
    let joined = if directory.is_empty() {
        pattern.to_string()
    } else {
        format!("{directory}/{pattern}")
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

/// Rewrite extglob alternation to brace alternation. Storybook globs commonly
/// use `@(js|tsx)` and may nest it inside another group, as the default glob
/// does. `?(...)` also permits the empty case.
fn rewrite_extglob(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut output = String::with_capacity(pattern.len());
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if matches!(character, '@' | '+' | '?')
            && chars.get(index + 1) == Some(&'(')
            && let Some(close) = matching_paren(&chars, index + 1)
        {
            let inner: String = chars[index + 2..close].iter().collect();
            let alternatives: Vec<String> = split_alternatives(&inner)
                .iter()
                .map(|alternative| rewrite_extglob(alternative))
                .collect();
            output.push('{');
            if character == '?' {
                output.push(',');
            }
            output.push_str(&alternatives.join(","));
            output.push('}');
            index = close + 1;
            continue;
        }
        output.push(character);
        index += 1;
    }
    output
}

fn matching_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, character) in chars.iter().enumerate().skip(open) {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_alternatives(inner: &str) -> Vec<String> {
    let mut alternatives = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for character in inner.chars() {
        match character {
            '(' => {
                depth += 1;
                current.push(character);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            '|' if depth == 0 => {
                alternatives.push(current);
                current = String::new();
            }
            _ => current.push(character),
        }
    }
    alternatives.push(current);
    alternatives
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_string_and_object_story_specifiers() {
        let source = r#"
            const config = {
              stories: [
                "../src/**/*.demo.tsx",
                { directory: "../packages/ui", files: "**/*.preview.tsx" },
                { directory: "../docs" },
              ],
              framework: "@storybook/react-vite",
            };
            export default config;
        "#;
        assert_eq!(
            story_patterns(".storybook/main.ts", source),
            vec![
                "docs/**/*.@(mdx|stories.@(mdx|js|jsx|mjs|ts|tsx))",
                "packages/ui/**/*.preview.tsx",
                "src/**/*.demo.tsx",
            ]
        );
    }

    #[test]
    fn reads_module_exports_and_defined_configs() {
        let source = r#"
            module.exports = {
              stories: ["../stories/**/*.js"],
            };
        "#;
        assert_eq!(
            story_patterns(".storybook/main.js", source),
            vec!["stories/**/*.js"]
        );

        let defined = r#"
            export default defineMain({
              stories: ["../src/**/*.story.tsx"],
            });
        "#;
        assert_eq!(
            story_patterns(".storybook/main.ts", defined),
            vec!["src/**/*.story.tsx"]
        );
    }

    #[test]
    fn skips_computed_and_absolute_patterns() {
        let source = r#"
            const suffix = "demo";
            export default {
              stories: [
                `/srv/${suffix}/**/*.tsx`,
                "/srv/stories/**/*.tsx",
                "!../src/**/*.test.tsx",
                "../src/**/" + "*.demo.tsx",
              ],
            };
        "#;
        assert!(story_patterns(".storybook/main.ts", source).is_empty());
    }

    #[test]
    fn rewrites_nested_extglob_alternation() {
        assert_eq!(
            rewrite_extglob("src/**/*.@(mdx|stories.@(js|tsx))"),
            "src/**/*.{mdx,stories.{js,tsx}}"
        );
        assert_eq!(rewrite_extglob("src/*.?(test.)tsx"), "src/*.{,test.}tsx");
        let matcher = matcher(&["src/**/*.@(mdx|stories.@(js|tsx))".to_string()]).expect("matcher");
        assert!(matcher.is_match("src/stories/Button.stories.tsx"));
        assert!(matcher.is_match("src/intro.mdx"));
        assert!(!matcher.is_match("src/Button.tsx"));
    }

    #[test]
    fn recognizes_storybook_main_configs() {
        assert!(is_config_path(".storybook/main.ts"));
        assert!(is_config_path("apps/web/.storybook/main.mjs"));
        assert!(is_config_path(".rnstorybook/main.js"));
        assert!(!is_config_path(".storybook/preview.ts"));
        assert!(!is_config_path("storybook/main.ts"));
        assert!(!is_config_path(".storybook/main.json"));
    }

    #[test]
    fn anchors_patterns_at_the_config_directory() {
        assert_eq!(
            anchor(".storybook", "../src/**/*.demo.tsx").as_deref(),
            Some("src/**/*.demo.tsx")
        );
        assert_eq!(
            anchor("apps/web/.storybook", "./stories/**/*.tsx").as_deref(),
            Some("apps/web/.storybook/stories/**/*.tsx")
        );
        assert_eq!(anchor(".storybook", "/tmp/stories/**/*.tsx"), None);
        assert_eq!(anchor(".storybook", "!../src/**/*.tsx"), None);
        assert_eq!(
            anchor(".storybook", "../../outside/**/*.tsx").as_deref(),
            Some("../outside/**/*.tsx")
        );
    }
}
