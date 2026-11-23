//! Framework config parsers and the helpers they share.
//!
//! Every parser here is lexical and side-effect free. Parsers use
//! [`with_config_ast`] so the oxc allocator owns the program and no AST node
//! can escape the callback. Path handling goes through
//! [`shrike_core::normalize_relative`], which is the single lexical path
//! normalizer shared by the whole engine.

use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind, Program, PropertyKey};
use oxc_span::SourceType;

pub(crate) mod cosmos;
pub(crate) mod cypress;
pub(crate) mod docusaurus;
pub(crate) mod gatsby;
pub(crate) mod ladle;
pub(crate) mod next;
pub(crate) mod storybook;
pub(crate) mod test;
pub(crate) mod webpack;

/// Parse a config file and run `visit` on the AST when parsing succeeds.
/// `None` means either an unknown source type or a parse error; callers treat
/// that as "no entries from this config". The allocator is local to this
/// function, so the borrowed `Program` cannot outlive it.
pub(crate) fn with_config_ast<T>(
    source_type: SourceType,
    source: &str,
    visit: impl FnOnce(&Program<'_>) -> T,
) -> Option<T> {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return None;
    }
    Some(visit(&parsed.program))
}

/// Source type for a config file. `.mts`/`.cts` are ESM TypeScript even though
/// oxc does not infer that from the extension alone; everything else falls
/// back to oxc's path inference.
#[must_use]
pub(crate) fn source_type(path: &str) -> Option<SourceType> {
    if path.ends_with(".mts") || path.ends_with(".cts") {
        Some(SourceType::ts().with_module(true))
    } else {
        SourceType::from_path(path).ok()
    }
}

/// Look up a non-computed object property by static identifier or string key.
/// Spread properties and computed keys are ignored.
pub(crate) fn find_property<'a>(
    object: &'a ObjectExpression<'a>,
    name: &str,
) -> Option<&'a Expression<'a>> {
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

/// Strip TypeScript wrappers and parentheses around an expression.
pub(crate) fn unwrap<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::ParenthesizedExpression(inner) => unwrap(&inner.expression),
        Expression::TSAsExpression(inner) => unwrap(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap(&inner.expression),
        _ => expression,
    }
}

/// Read a config value that is a plain string or an interpolation-free
/// template literal, unwrapping TypeScript wrappers first.
pub(crate) fn string_value(expression: &Expression<'_>) -> Option<String> {
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

/// Anchor a Ladle or Storybook pattern at a package/config directory. The two
/// frameworks share the same semantics: absolute and negated patterns are not
/// entries, and leading `..` segments are kept because they match nothing.
/// Cypress has different anchoring rules and keeps its own helper.
#[must_use]
pub(crate) fn anchor_pattern(directory: &str, pattern: &str) -> Option<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.starts_with('/') || pattern.starts_with('!') {
        return None;
    }
    let joined = if directory.is_empty() {
        pattern.to_string()
    } else {
        format!("{directory}/{pattern}")
    };
    let normalized = shrike_core::normalize_relative(&joined);
    (!normalized.is_empty()).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    // Parse an object and apply `f` while the allocator is alive.
    fn with_object<T>(source: &str, f: impl FnOnce(&ObjectExpression<'_>) -> T) -> T {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
        assert!(parsed.errors.is_empty(), "{source}: {:?}", parsed.errors);
        let oxc_ast::ast::Statement::ExpressionStatement(statement) = &parsed.program.body[0]
        else {
            panic!("expected expression statement");
        };
        let Expression::ObjectExpression(object) = unwrap(&statement.expression) else {
            panic!("expected object expression");
        };
        f(object)
    }

    #[test]
    fn find_property_matches_static_identifier_string_key_and_skips_computed() {
        with_object("({ named: 1, 'quoted': 2, [computed]: 3 })", |object| {
            let numeric = |value: Option<&Expression<'_>>| match value {
                Some(Expression::NumericLiteral(literal)) => Some(literal.value),
                _ => None,
            };
            assert_eq!(numeric(find_property(object, "named")), Some(1.0));
            assert_eq!(numeric(find_property(object, "quoted")), Some(2.0));
            assert!(find_property(object, "computed").is_none());
        });
    }

    #[test]
    fn source_type_handles_mts_cts_and_unknown_extensions() {
        assert_eq!(
            source_type("config.mts"),
            Some(SourceType::ts().with_module(true))
        );
        assert_eq!(
            source_type("config.cts"),
            Some(SourceType::ts().with_module(true))
        );
        assert_eq!(
            source_type("config.js"),
            SourceType::from_path("config.js").ok()
        );
        assert_eq!(
            source_type("config.unknown"),
            SourceType::from_path("config.unknown").ok()
        );
    }

    #[test]
    fn normalize_relative_handles_dots_duplicate_slashes_and_backslashes() {
        assert_eq!(shrike_core::normalize_relative("a/./b//c"), "a/b/c");
        assert_eq!(shrike_core::normalize_relative("a/b/../c"), "a/c");
        assert_eq!(shrike_core::normalize_relative("../a/../../b"), "../../b");
        assert_eq!(shrike_core::normalize_relative("a\\b\\c"), "a/b/c");
        assert_eq!(shrike_core::normalize_relative("./"), "");
    }

    #[test]
    fn anchor_pattern_shared_semantics() {
        assert_eq!(
            anchor_pattern("pkg", "src/*.ts").as_deref(),
            Some("pkg/src/*.ts")
        );
        assert_eq!(anchor_pattern("pkg", "/abs.ts"), None);
        assert_eq!(anchor_pattern("pkg", "!skip.ts"), None);
        assert_eq!(anchor_pattern("", "src/*.ts").as_deref(), Some("src/*.ts"));
    }
}
