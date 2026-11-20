use oxc_ast::ast::{
    Argument, CallExpression, ComputedMemberExpression, Expression, ImportExpression,
    JSXIdentifier, MemberExpression, StaticMemberExpression,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_call_expression, walk_computed_member_expression, walk_import_expression,
    walk_static_member_expression,
};
use shrike_core::Span;

use crate::summary::{
    DynamicKind, DynamicRecord, DynamicSpecifier, MemberAccess, ModuleRefs, UnknownRecord,
    UnknownScope,
};

#[derive(Default)]
pub(super) struct RefCollector {
    pub(super) identifiers: Vec<String>,
    pub(super) members: Vec<MemberAccess>,
    pub(super) jsx: Vec<String>,
    pub(super) dynamic: Vec<DynamicRecord>,
    pub(super) unknown: Vec<UnknownRecord>,
}

impl RefCollector {
    pub(super) fn into_parts(self) -> (ModuleRefs, Vec<DynamicRecord>, Vec<UnknownRecord>) {
        (
            ModuleRefs {
                identifiers: self.identifiers,
                members: self.members,
                jsx: self.jsx,
            },
            self.dynamic,
            self.unknown,
        )
    }
}

impl<'a> Visit<'a> for RefCollector {
    fn visit_identifier_reference(&mut self, it: &oxc_ast::ast::IdentifierReference<'a>) {
        self.identifiers.push(it.name.to_string());
    }

    fn visit_jsx_identifier(&mut self, it: &JSXIdentifier<'a>) {
        if it
            .name
            .chars()
            .next()
            .is_some_and(|character| character.is_uppercase())
        {
            self.jsx.push(it.name.to_string());
        }
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.members.push(MemberAccess {
                object: object.name.to_string(),
                property: Some(it.property.name.to_string()),
                computed: false,
            });
        }
        walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            let property = match &it.expression {
                Expression::StringLiteral(literal) => Some(literal.value.to_string()),
                Expression::NumericLiteral(literal) => Some(
                    literal
                        .raw
                        .as_ref()
                        .map_or_else(|| literal.value.to_string(), ToString::to_string),
                ),
                _ => None,
            };
            if property.is_none()
                && matches!(
                    object.name.as_str(),
                    "globalThis" | "window" | "self" | "global"
                )
            {
                self.unknown.push(UnknownRecord {
                    reason: "computed access on a global object".to_string(),
                    scope: UnknownScope::Repo,
                    span: Span::default(),
                });
            }
            self.members.push(MemberAccess {
                object: object.name.to_string(),
                property,
                computed: true,
            });
        }
        walk_computed_member_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.dynamic.push(DynamicRecord {
            kind: DynamicKind::Import,
            specifier: dynamic_specifier(&it.source),
            span: Span::default(),
        });
        walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" {
                if let Some(argument) = it.arguments.first() {
                    self.dynamic.push(DynamicRecord {
                        kind: DynamicKind::Require,
                        specifier: argument_specifier(argument),
                        span: Span::default(),
                    });
                }
            }
        }
        walk_call_expression(self, it);
    }
}

fn dynamic_specifier(expression: &Expression<'_>) -> DynamicSpecifier {
    match expression {
        Expression::StringLiteral(literal) => DynamicSpecifier::Literal(literal.value.to_string()),
        Expression::TemplateLiteral(template) => {
            if template.expressions.is_empty() {
                DynamicSpecifier::Literal(template.quasis.first().map_or_else(
                    String::new,
                    |quasi| {
                        quasi
                            .value
                            .cooked
                            .as_ref()
                            .map_or_else(|| quasi.value.raw.to_string(), ToString::to_string)
                    },
                ))
            } else {
                let prefix = template.quasis.first().map_or_else(String::new, |quasi| {
                    quasi
                        .value
                        .cooked
                        .as_ref()
                        .map_or_else(|| quasi.value.raw.to_string(), ToString::to_string)
                });
                if prefix.contains('/') || prefix.starts_with('.') {
                    DynamicSpecifier::TemplatePrefix(prefix)
                } else {
                    DynamicSpecifier::Unknown
                }
            }
        }
        _ => DynamicSpecifier::Unknown,
    }
}

fn argument_specifier(argument: &Argument<'_>) -> DynamicSpecifier {
    match argument {
        Argument::StringLiteral(literal) => DynamicSpecifier::Literal(literal.value.to_string()),
        Argument::TemplateLiteral(template) => {
            if template.expressions.is_empty() {
                DynamicSpecifier::Literal(template.quasis.first().map_or_else(
                    String::new,
                    |quasi| {
                        quasi
                            .value
                            .cooked
                            .as_ref()
                            .map_or_else(|| quasi.value.raw.to_string(), ToString::to_string)
                    },
                ))
            } else {
                let prefix = template.quasis.first().map_or_else(String::new, |quasi| {
                    quasi
                        .value
                        .cooked
                        .as_ref()
                        .map_or_else(|| quasi.value.raw.to_string(), ToString::to_string)
                });
                if prefix.contains('/') || prefix.starts_with('.') {
                    DynamicSpecifier::TemplatePrefix(prefix)
                } else {
                    DynamicSpecifier::Unknown
                }
            }
        }
        _ => DynamicSpecifier::Unknown,
    }
}

// Keep unused imports out of the public surface while making the dependency
// explicit to rustc.
#[allow(dead_code)]
fn _span_is_used(span: oxc_span::Span) -> u32 {
    span.start
}

#[allow(dead_code)]
fn _member_is_used(member: &MemberExpression<'_>) -> bool {
    matches!(member, MemberExpression::StaticMemberExpression(_))
}
