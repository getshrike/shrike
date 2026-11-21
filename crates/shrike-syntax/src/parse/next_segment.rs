//! Next.js App Router segment-config export shapes.
//!
//! Next.js's `getAppPageStaticInfo` analyzer only recognizes a few top-level
//! export forms when it validates route segment configuration. Mirroring those
//! forms here keeps the graph-side checks free of false positives:
//!
//! * `generateStaticParams`, `unstable_paramMatching`, and
//!   `unstable_generateParamMatching` count as function declarations, as the
//!   first declarator of a variable declaration, or as export specifiers
//!   (including type-only and re-exported specifiers). `export * as name` is
//!   not recognized.
//! * Segment-config values such as `runtime`, `instant`, and
//!   `unstable_dynamicStaleTime` are read only from directly exported `const`
//!   declarations whose initializer Next.js can statically extract.
//!
//! The parser records the shapes and the extracted values. The graph decides
//! whether a file is a Next.js app route and whether a combination is a build
//! failure.

use std::collections::BTreeSet;

use oxc_ast::ast::{
    ArrayExpressionElement, BindingPatternKind, Declaration, Expression, ObjectPropertyKind,
    Program, PropertyKey, PropertyKind, Statement, VariableDeclarationKind,
};
use shrike_core::{LineIndex, Span};

use super::refs::{export_name, span_of};
use crate::summary::{ExportedConstValue, ExportedConstValueRecord, NextSegmentExportRecord};

/// Special exports Next.js's `checkExports` tracks as booleans.
const SPECIAL_EXPORTS: &[&str] = &[
    "generateStaticParams",
    "unstable_paramMatching",
    "unstable_generateParamMatching",
];

/// Keys in Next.js's `AppSegmentConfigSchema`. Values for other names are
/// never read as segment config.
const SEGMENT_CONFIG_NAMES: &[&str] = &[
    "revalidate",
    "dynamicParams",
    "dynamic",
    "fetchCache",
    "instant",
    "prefetch",
    "ensureStatic",
    "unstable_dynamicStaleTime",
    "preferredRegion",
    "runtime",
    "maxDuration",
];

pub(super) fn next_segment_exports(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Vec<NextSegmentExportRecord> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    let mut record = |name: String, span: Span| {
        if SPECIAL_EXPORTS.contains(&name.as_str()) && seen.insert(name.clone()) {
            records.push(NextSegmentExportRecord { name, span });
        }
    };
    for statement in &program.body {
        let Statement::ExportNamedDeclaration(export) = statement else {
            continue;
        };
        if let Some(declaration) = &export.declaration {
            match declaration {
                Declaration::FunctionDeclaration(function) => {
                    if let Some(id) = &function.id {
                        record(
                            id.name.to_string(),
                            span_of(line_index, source, function.span),
                        );
                    }
                }
                Declaration::VariableDeclaration(variable) => {
                    // Next.js checks only the first declarator for the
                    // special-export flags, even though it reads config values
                    // from every declarator.
                    if let Some(declarator) = variable.declarations.first() {
                        if let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind {
                            record(
                                id.name.to_string(),
                                span_of(line_index, source, declarator.span),
                            );
                        }
                    }
                }
                _ => {}
            }
        } else {
            for specifier in &export.specifiers {
                // Next.js matches the original (local or imported) name, not
                // the exported alias.
                record(
                    export_name(&specifier.local),
                    span_of(line_index, source, specifier.span),
                );
            }
        }
    }
    records
}

pub(super) fn exported_const_values(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Vec<ExportedConstValueRecord> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for statement in &program.body {
        let Statement::ExportNamedDeclaration(export) = statement else {
            continue;
        };
        let Some(Declaration::VariableDeclaration(variable)) = &export.declaration else {
            continue;
        };
        if variable.kind != VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &variable.declarations {
            let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
                continue;
            };
            if !SEGMENT_CONFIG_NAMES.contains(&id.name.as_str()) {
                continue;
            }
            let Some(init) = &declarator.init else {
                continue;
            };
            let Some(value) = const_value(init) else {
                continue;
            };
            if seen.insert(id.name.to_string()) {
                records.push(ExportedConstValueRecord {
                    name: id.name.to_string(),
                    value,
                    span: span_of(line_index, source, declarator.span),
                });
            }
        }
    }
    records
}

/// Mirror `extractValue` from `next/dist/build/analysis/extract-const-value`.
///
/// Parentheses are deliberately not unwrapped: Next.js does not handle
/// `ParenthesisExpression`, so `export const runtime = ('edge')` is not read
/// as segment config. TypeScript assertion wrappers are handled.
pub(super) fn const_value(expression: &Expression<'_>) -> Option<ExportedConstValue> {
    match expression {
        Expression::TSAsExpression(inner) => const_value(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => const_value(&inner.expression),
        Expression::TSTypeAssertion(inner) => const_value(&inner.expression),
        Expression::StringLiteral(literal) => {
            Some(ExportedConstValue::String(literal.value.to_string()))
        }
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            let quasi = template.quasis.first()?;
            let value = quasi.value.cooked.as_ref().unwrap_or(&quasi.value.raw);
            Some(ExportedConstValue::String(value.to_string()))
        }
        Expression::NumericLiteral(literal) => {
            Some(ExportedConstValue::Number(literal.value.to_string()))
        }
        Expression::BooleanLiteral(literal) => Some(ExportedConstValue::Boolean(literal.value)),
        Expression::NullLiteral(_) => Some(ExportedConstValue::Null),
        Expression::RegExpLiteral(_) => Some(ExportedConstValue::RegExp),
        Expression::ArrayExpression(array) => array
            .elements
            .iter()
            .all(array_element_extractable)
            .then_some(ExportedConstValue::Array),
        Expression::ObjectExpression(object) => object
            .properties
            .iter()
            .all(object_property_extractable)
            .then_some(ExportedConstValue::Object),
        _ => None,
    }
}

fn array_element_extractable(element: &ArrayExpressionElement<'_>) -> bool {
    match element {
        ArrayExpressionElement::Elision(_) => true,
        ArrayExpressionElement::SpreadElement(_) => false,
        other => other
            .as_expression()
            .is_some_and(|expression| const_value(expression).is_some()),
    }
}

fn object_property_extractable(property: &ObjectPropertyKind<'_>) -> bool {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
        return false;
    };
    if property.kind != PropertyKind::Init || property.method {
        return false;
    }
    let key_ok = matches!(
        &property.key,
        PropertyKey::StaticIdentifier(_) | PropertyKey::StringLiteral(_)
    );
    key_ok && const_value(&property.value).is_some()
}
