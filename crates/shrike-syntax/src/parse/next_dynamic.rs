//! Detect the `next/dynamic` client-only escape hatch.
//!
//! `next/dynamic(() => import("./chart"), { ssr: false })` tells Next.js that
//! the target never renders on the server. The target still ships to the
//! browser, but the static client/server leak scan treats it as intentional
//! client-only code instead of a server boundary crossing, matching fallow
//! 3.45.0's `client_only_dynamic_import_spans` exclusion.

use std::collections::BTreeSet;

use oxc_ast::ast::{
    Argument, CallExpression, Expression, ImportExpression, ObjectPropertyKind, Program,
    PropertyKey,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{walk_call_expression, walk_import_expression};
use shrike_core::{LineIndex, Span};

use crate::summary::{BindingKind, DynamicSpecifier, FileSummary, ImportRecord};

use super::refs::span_of;

/// The specifiers of `import()` calls inside a literal
/// `next/dynamic(..., { ssr: false })` call. The caller stores them on the
/// summary so the graph builder can skip that edge when it is the only way the
/// target is reached.
pub(super) fn client_only_dynamic_specifiers(
    program: &Program<'_>,
    imports: &[ImportRecord],
) -> Vec<String> {
    let mut aliases = BTreeSet::new();
    let mut namespaces = BTreeSet::new();
    for import in imports {
        if import.source != "next/dynamic" || import.type_only {
            continue;
        }
        for binding in &import.bindings {
            if binding.type_only {
                continue;
            }
            match binding.kind {
                BindingKind::Default => {
                    aliases.insert(binding.local.clone());
                }
                BindingKind::Named
                    if matches!(binding.imported.as_str(), "default" | "dynamic") =>
                {
                    aliases.insert(binding.local.clone());
                }
                BindingKind::Namespace => {
                    namespaces.insert(binding.local.clone());
                }
                BindingKind::Named => {}
            }
        }
    }
    if aliases.is_empty() && namespaces.is_empty() {
        return Vec::new();
    }

    let mut visitor = ClientOnlyDynamicVisitor {
        aliases,
        namespaces,
        specifiers: Vec::new(),
    };
    visitor.visit_program(program);
    visitor.specifiers.sort();
    visitor.specifiers.dedup();
    visitor.specifiers
}

/// Fill the source span of literal dynamic-import records. The collector
/// records the specifier but not the AST span, so diagnostics and leak anchors
/// would otherwise point at line 0. Records are matched in source order so two
/// imports of the same specifier keep distinct locations.
pub(super) fn assign_dynamic_spans(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
    summary: &mut FileSummary,
) {
    let mut spans: Vec<(String, Span)> = Vec::new();
    let mut visitor = DynamicImportSpanVisitor {
        spans: &mut spans,
        source,
        line_index,
    };
    visitor.visit_program(program);
    for record in &mut summary.dynamic {
        if record.span.line != 0 {
            continue;
        }
        let DynamicSpecifier::Literal(specifier) = &record.specifier else {
            continue;
        };
        if let Some(index) = spans
            .iter()
            .position(|(candidate, _)| candidate == specifier)
        {
            record.span = spans.remove(index).1;
        }
    }
}

struct DynamicImportSpanVisitor<'a> {
    spans: &'a mut Vec<(String, Span)>,
    source: &'a str,
    line_index: &'a LineIndex,
}

impl<'a> Visit<'a> for DynamicImportSpanVisitor<'_> {
    fn visit_import_expression(&mut self, import: &ImportExpression<'a>) {
        if let Expression::StringLiteral(literal) = &import.source {
            self.spans.push((
                literal.value.to_string(),
                span_of(self.line_index, self.source, import.span),
            ));
        }
        walk_import_expression(self, import);
    }
}

struct ClientOnlyDynamicVisitor {
    aliases: BTreeSet<String>,
    namespaces: BTreeSet<String>,
    specifiers: Vec<String>,
}

impl<'a> Visit<'a> for ClientOnlyDynamicVisitor {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if self.is_client_only_dynamic(call) {
            if let Some(argument) = call.arguments.first() {
                let mut finder = ImportSpecifierFinder {
                    specifiers: &mut self.specifiers,
                };
                finder.visit_argument(argument);
            }
        }
        walk_call_expression(self, call);
    }
}

impl ClientOnlyDynamicVisitor {
    fn is_client_only_dynamic(&self, call: &CallExpression<'_>) -> bool {
        let is_dynamic = match &call.callee {
            Expression::Identifier(identifier) => self.aliases.contains(identifier.name.as_str()),
            Expression::StaticMemberExpression(member) => {
                member.property.name == "default"
                    && matches!(
                        &member.object,
                        Expression::Identifier(identifier)
                            if self.namespaces.contains(identifier.name.as_str())
                    )
            }
            _ => false,
        };
        if !is_dynamic {
            return false;
        }
        let Some(Expression::ObjectExpression(options)) =
            call.arguments.get(1).and_then(Argument::as_expression)
        else {
            return false;
        };
        options.properties.iter().any(|property| {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return false;
            };
            if property.computed {
                return false;
            }
            let key = match &property.key {
                PropertyKey::StaticIdentifier(identifier) => identifier.name.as_str(),
                PropertyKey::StringLiteral(literal) => literal.value.as_str(),
                _ => return false,
            };
            key == "ssr"
                && matches!(&property.value, Expression::BooleanLiteral(literal) if !literal.value)
        })
    }
}

struct ImportSpecifierFinder<'a> {
    specifiers: &'a mut Vec<String>,
}

impl<'a> Visit<'a> for ImportSpecifierFinder<'_> {
    fn visit_import_expression(&mut self, import: &ImportExpression<'a>) {
        if let Expression::StringLiteral(literal) = &import.source {
            self.specifiers.push(literal.value.to_string());
        }
        walk_import_expression(self, import);
    }
}
