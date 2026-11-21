//! Static Cypress configuration reading.
//!
//! Cypress 10+ discovers spec files from `e2e.specPattern` (default
//! `cypress/e2e/**/*.cy.{js,jsx,ts,tsx}`) and loads the configured
//! `supportFile` (default `cypress/support/e2e.{js,jsx,ts,tsx}`) before each
//! spec. Neither file is imported by application code, so without this module
//! they look like dead files and every helper only they import looks dead too.
//!
//! The configuration is executable JavaScript. Only literal string values are
//! trusted: a computed pattern falls back to Cypress's documented default
//! rather than inventing entries. Component testing patterns apply only when
//! the config declares a `component` object, because the component default
//! (`**/*.cy.*`) is broad enough to protect unrelated files in an e2e-only
//! project.

use std::collections::BTreeSet;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind, PropertyKey};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SourceType;

const E2E_SPEC: &str = "cypress/e2e/**/*.cy.{js,jsx,ts,tsx}";
const E2E_SUPPORT: &str = "cypress/support/e2e.{js,jsx,ts,tsx}";
const COMPONENT_SPEC: &str = "**/*.cy.{js,jsx,ts,tsx}";
const COMPONENT_SUPPORT: &str = "cypress/support/component.{js,jsx,ts,tsx}";
const E2E_EXCLUDE: &str = "*.hot-update.js";
const COMPONENT_EXCLUDES: &[&str] = &["/snapshots/*", "/image_snapshots/*"];

/// Files Cypress executes by convention: discovered specs and the support
/// module loaded before them.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CypressEntries {
    pub specs: Vec<String>,
    pub support: Vec<String>,
}

/// Whether a path is a Cypress configuration file. Cypress supports `.js`,
/// `.ts`, `.mjs`, `.cjs`, `.mts`, and `.cts`.
#[must_use]
pub(crate) fn is_config_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    stem == "cypress.config" && matches!(extension, "js" | "ts" | "mjs" | "cjs" | "mts" | "cts")
}

/// Files Cypress loads from one configuration. `analyzed` limits the result to
/// files the analysis contains; patterns that match nothing are ignored.
#[must_use]
pub(crate) fn entries(
    analyzed: &BTreeSet<&str>,
    config_path: &str,
    source: &str,
) -> CypressEntries {
    let scan = scan(config_path, source);
    let directory = config_path
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let mut specs = BTreeSet::new();
    let mut support = BTreeSet::new();

    // E2E is Cypress's default testing type even when the config has no `e2e`
    // key. Its documented defaults are the safe fallback for computed values.
    let e2e = scan.e2e.unwrap_or_default();
    let e2e_patterns = e2e
        .spec_patterns
        .unwrap_or_else(|| vec![E2E_SPEC.to_string()]);
    let e2e_excludes = e2e
        .exclude_patterns
        .unwrap_or_else(|| vec![E2E_EXCLUDE.to_string()]);
    specs.extend(matching_files(
        directory,
        &e2e_patterns,
        &e2e_excludes,
        analyzed,
    ));
    match e2e.support_file {
        Some(SupportFile::Disabled) => {}
        Some(SupportFile::Path(path)) => {
            if let Some(path) = resolve_file(directory, &path, analyzed) {
                support.insert(path);
            }
        }
        None => support.extend(matching_files(
            directory,
            &[E2E_SUPPORT.to_string()],
            &[],
            analyzed,
        )),
    }

    // Component testing only exists when the config declares it. Its default
    // spec glob matches every `.cy.*` file, which must not leak into e2e-only
    // projects.
    if scan.has_component {
        let component = scan.component.unwrap_or_default();
        let patterns = component
            .spec_patterns
            .unwrap_or_else(|| vec![COMPONENT_SPEC.to_string()]);
        let excludes = component
            .exclude_patterns
            .unwrap_or_else(|| COMPONENT_EXCLUDES.iter().map(ToString::to_string).collect());
        specs.extend(matching_files(directory, &patterns, &excludes, analyzed));
        match component.support_file {
            Some(SupportFile::Disabled) => {}
            Some(SupportFile::Path(path)) => {
                if let Some(path) = resolve_file(directory, &path, analyzed) {
                    support.insert(path);
                }
            }
            None => support.extend(matching_files(
                directory,
                &[COMPONENT_SUPPORT.to_string()],
                &[],
                analyzed,
            )),
        }
    }

    CypressEntries {
        specs: specs.into_iter().collect(),
        support: support.into_iter().collect(),
    }
}

fn matching_files(
    directory: &str,
    patterns: &[String],
    excludes: &[String],
    analyzed: &BTreeSet<&str>,
) -> Vec<String> {
    let Some(include) = matcher(directory, patterns) else {
        return Vec::new();
    };
    let exclude = matcher(directory, excludes);
    analyzed
        .iter()
        .filter(|path| include.is_match(path))
        .filter(|path| !exclude.as_ref().is_some_and(|set| set.is_match(path)))
        .map(|path| (*path).to_string())
        .collect()
}

fn matcher(directory: &str, patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let mut added = false;
    for pattern in patterns {
        let Some(anchored) = anchor(directory, pattern) else {
            continue;
        };
        let Ok(glob) = GlobBuilder::new(&anchored).literal_separator(true).build() else {
            continue;
        };
        builder.add(glob);
        added = true;
    }
    added.then(|| builder.build().ok()).flatten()
}

/// Anchor a Cypress glob at the project root (the config file's directory).
/// Patterns without a slash use minimatch's `matchBase` behavior and match a
/// basename at any depth. A leading slash is root-relative. Negated and
/// absolute patterns are skipped.
fn anchor(directory: &str, pattern: &str) -> Option<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.starts_with('!') {
        return None;
    }
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let pattern = if pattern.contains('/') {
        pattern.to_string()
    } else {
        format!("**/{pattern}")
    };
    let joined = if directory.is_empty() {
        pattern
    } else {
        format!("{directory}/{pattern}")
    };
    let normalized = normalize(&joined);
    (!normalized.is_empty() && !normalized.starts_with("../")).then_some(normalized)
}

fn resolve_file(directory: &str, path: &str, analyzed: &BTreeSet<&str>) -> Option<String> {
    let path = path.trim();
    if path.is_empty() || path.starts_with('!') {
        return None;
    }
    let joined = if path.starts_with('/') {
        path.trim_start_matches('/').to_string()
    } else if directory.is_empty() {
        path.to_string()
    } else {
        format!("{directory}/{path}")
    };
    let normalized = normalize(&joined);
    analyzed.contains(normalized.as_str()).then_some(normalized)
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

fn scan(config_path: &str, source: &str) -> ConfigScan {
    let Some(source_type) = source_type(config_path) else {
        return ConfigScan::default();
    };
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return ConfigScan::default();
    }
    let mut scan = ConfigScan::default();
    scan.visit_program(&parsed.program);
    scan
}

fn source_type(path: &str) -> Option<SourceType> {
    if path.ends_with(".mts") || path.ends_with(".cts") {
        Some(SourceType::ts().with_module(true))
    } else {
        SourceType::from_path(path).ok()
    }
}

#[derive(Debug, Default)]
struct ConfigScan {
    e2e: Option<TestingType>,
    component: Option<TestingType>,
    has_component: bool,
    seen_e2e: bool,
    seen_component: bool,
}

impl<'a> Visit<'a> for ConfigScan {
    fn visit_object_expression(&mut self, object: &ObjectExpression<'a>) {
        if !self.seen_e2e
            && let Some(value) = find_property(object, "e2e")
        {
            self.seen_e2e = true;
            self.e2e = object_expression(value).map(testing_type);
        }
        if !self.seen_component
            && let Some(value) = find_property(object, "component")
        {
            self.seen_component = true;
            self.has_component = true;
            self.component = object_expression(value).map(testing_type);
        }
        walk::walk_object_expression(self, object);
    }
}

#[derive(Debug, Default)]
struct TestingType {
    spec_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    support_file: Option<SupportFile>,
}

#[derive(Debug)]
enum SupportFile {
    Path(String),
    Disabled,
}

fn testing_type(object: &ObjectExpression<'_>) -> TestingType {
    TestingType {
        spec_patterns: find_property(object, "specPattern").and_then(string_list),
        exclude_patterns: find_property(object, "excludeSpecPattern").and_then(string_list),
        support_file: find_property(object, "supportFile").and_then(|expression| {
            match unwrap(expression) {
                Expression::BooleanLiteral(literal) if !literal.value => {
                    Some(SupportFile::Disabled)
                }
                _ => string_value(expression).map(SupportFile::Path),
            }
        }),
    }
}

fn object_expression<'a>(expression: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
    match unwrap(expression) {
        Expression::ObjectExpression(object) => Some(object),
        _ => None,
    }
}

fn string_list(expression: &Expression<'_>) -> Option<Vec<String>> {
    match unwrap(expression) {
        Expression::ArrayExpression(array) => Some(
            array
                .elements
                .iter()
                .filter_map(|element| element.as_expression())
                .filter_map(string_value)
                .collect(),
        ),
        _ => string_value(expression).map(|value| vec![value]),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{CypressEntries, entries, is_config_path};

    fn analyzed<'a>(paths: &'a [&'a str]) -> BTreeSet<&'a str> {
        paths.iter().copied().collect()
    }

    fn run(source: &str, paths: &[&str]) -> CypressEntries {
        entries(&analyzed(paths), "cypress.config.ts", source)
    }

    #[test]
    fn recognizes_cypress_configs() {
        for path in [
            "cypress.config.ts",
            "cypress.config.js",
            "apps/web/cypress.config.mjs",
            "cypress.config.cjs",
            "cypress.config.mts",
            "cypress.config.cts",
            // Monorepo packages keep their own project root and config.
            "src/cypress.config.ts",
        ] {
            assert!(is_config_path(path), "{path}");
        }
        for path in [
            "cypress.config.json",
            "vitest.config.ts",
            "cypress.config.ts.bak",
        ] {
            assert!(!is_config_path(path), "{path}");
        }
    }

    #[test]
    fn defaults_match_e2e_specs_and_support() {
        let result = run(
            "export default { e2e: { baseUrl: 'http://localhost:3000' } };",
            &[
                "cypress/e2e/home.cy.ts",
                "cypress/e2e/admin.cy.jsx",
                "cypress/support/e2e.ts",
                "cypress/support/commands.ts",
                "src/dead.ts",
            ],
        );
        assert_eq!(
            result,
            CypressEntries {
                specs: vec![
                    "cypress/e2e/admin.cy.jsx".to_string(),
                    "cypress/e2e/home.cy.ts".to_string()
                ],
                support: vec!["cypress/support/e2e.ts".to_string()],
            }
        );
    }

    #[test]
    fn reads_custom_spec_and_support_patterns() {
        let result = run(
            r#"
            export default defineConfig({
              e2e: {
                specPattern: ["e2e/**/*.cy.ts", "smoke/**/*.smoke.ts"],
                excludeSpecPattern: ["**/skip.cy.ts"],
                supportFile: "e2e/support.ts",
              },
            });
            "#,
            &[
                "e2e/home.cy.ts",
                "e2e/skip.cy.ts",
                "smoke/login.smoke.ts",
                "e2e/support.ts",
                "cypress/support/e2e.ts",
            ],
        );
        assert_eq!(
            result,
            CypressEntries {
                specs: vec![
                    "e2e/home.cy.ts".to_string(),
                    "smoke/login.smoke.ts".to_string()
                ],
                support: vec!["e2e/support.ts".to_string()],
            }
        );
    }

    #[test]
    fn support_file_false_disables_the_support_entry() {
        let result = run(
            "export default { e2e: { supportFile: false } };",
            &["cypress/e2e/home.cy.ts", "cypress/support/e2e.ts"],
        );
        assert_eq!(
            result,
            CypressEntries {
                specs: vec!["cypress/e2e/home.cy.ts".to_string()],
                support: Vec::new(),
            }
        );
    }

    #[test]
    fn component_patterns_only_apply_when_component_is_declared() {
        let e2e_only = run(
            "export default { e2e: {} };",
            &["cypress/e2e/home.cy.ts", "src/widget.cy.tsx"],
        );
        assert_eq!(
            e2e_only,
            CypressEntries {
                specs: vec!["cypress/e2e/home.cy.ts".to_string()],
                support: Vec::new(),
            }
        );

        let component = run(
            "export default { component: { devServer: { framework: 'next', bundler: 'webpack' } } };",
            &[
                "cypress/e2e/home.cy.ts",
                "src/widget.cy.tsx",
                "cypress/support/e2e.ts",
                "cypress/support/component.ts",
            ],
        );
        assert_eq!(
            component,
            CypressEntries {
                specs: vec![
                    "cypress/e2e/home.cy.ts".to_string(),
                    "src/widget.cy.tsx".to_string()
                ],
                support: vec![
                    "cypress/support/component.ts".to_string(),
                    "cypress/support/e2e.ts".to_string()
                ],
            }
        );
    }

    #[test]
    fn computed_values_fall_back_to_defaults() {
        let result = run(
            r#"
            const pattern = makePattern();
            export default { e2e: { specPattern: pattern, supportFile: chooseSupport() } };
            "#,
            &[
                "cypress/e2e/home.cy.ts",
                "cypress/support/e2e.ts",
                "cypress/support/other.ts",
            ],
        );
        assert_eq!(
            result,
            CypressEntries {
                specs: vec!["cypress/e2e/home.cy.ts".to_string()],
                support: vec!["cypress/support/e2e.ts".to_string()],
            }
        );
    }

    #[test]
    fn patterns_are_anchored_at_the_config_directory() {
        let result = entries(
            &analyzed(&[
                "apps/web/cypress/e2e/home.cy.ts",
                "apps/admin/cypress/e2e/admin.cy.ts",
                "apps/web/cypress/support/e2e.ts",
            ]),
            "apps/web/cypress.config.ts",
            "export default { e2e: {} };",
        );
        assert_eq!(
            result,
            CypressEntries {
                specs: vec!["apps/web/cypress/e2e/home.cy.ts".to_string()],
                support: vec!["apps/web/cypress/support/e2e.ts".to_string()],
            }
        );
    }
}
