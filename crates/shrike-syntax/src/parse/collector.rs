use oxc_ast::ast::{
    Argument, BindingPatternKind, CallExpression, ComputedMemberExpression, Expression,
    ImportExpression, JSXIdentifier, JSXMemberExpression, JSXMemberExpressionObject,
    MemberExpression, ObjectPattern, PropertyKey, StaticMemberExpression, VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_call_expression, walk_computed_member_expression, walk_import_expression,
    walk_static_member_expression, walk_variable_declarator,
};
use shrike_core::Span;

use crate::summary::{
    DynamicKind, DynamicRecord, DynamicSpecifier, MemberAccess, ModuleRefs, UnknownRecord,
    UnknownScope,
};

#[derive(Default)]
pub(super) struct RefCollector {
    pub(super) identifiers: Vec<String>,
    pub(super) free: Vec<String>,
    pub(super) members: Vec<MemberAccess>,
    pub(super) jsx: Vec<String>,
    pub(super) dynamic: Vec<DynamicRecord>,
    pub(super) unknown: Vec<UnknownRecord>,
    /// Import/require nodes already recorded by the declarator handler. The
    /// generic visitors skip these so one import never produces a precise
    /// record and a widening record at the same time.
    handled_dynamic_spans: Vec<oxc_span::Span>,
}

impl RefCollector {
    pub(super) fn into_parts(self) -> (ModuleRefs, Vec<DynamicRecord>, Vec<UnknownRecord>) {
        (
            ModuleRefs {
                identifiers: self.identifiers,
                free: self.free,
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
        self.free.push(it.name.to_string());
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
            // Record the object as an ordinary reference, but not as a free
            // identifier: this use is precisely resolved through `members`.
            self.identifiers.push(object.name.to_string());
            self.members.push(MemberAccess {
                object: object.name.to_string(),
                property: Some(it.property.name.to_string()),
                computed: false,
            });
            return;
        }
        walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.identifiers.push(object.name.to_string());
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
            // The key expression may contain references (`UI[key]`), but the
            // object itself was handled above.
            self.visit_expression(&it.expression);
            return;
        }
        walk_computed_member_expression(self, it);
    }

    fn visit_jsx_member_expression(&mut self, it: &JSXMemberExpression<'a>) {
        // `<UI.Button />` is a JSX member expression, not a static member
        // expression. Record the root object and the final property so a
        // namespace import resolves to the component export it actually uses.
        match &it.object {
            JSXMemberExpressionObject::IdentifierReference(object) => {
                self.identifiers.push(object.name.to_string());
                self.members.push(MemberAccess {
                    object: object.name.to_string(),
                    property: Some(it.property.name.to_string()),
                    computed: false,
                });
            }
            JSXMemberExpressionObject::MemberExpression(inner) => {
                self.visit_jsx_member_expression(inner);
            }
            JSXMemberExpressionObject::ThisExpression(_) => {}
        }
        // Do not walk the property: it is a JSX identifier, not a reference to
        // a local binding. Walking it would mark an unrelated same-named local
        // as used.
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let Some((kind, specifier, span, imported)) = declaration_dynamic(it) {
            self.dynamic.push(DynamicRecord {
                kind,
                specifier,
                span: Span::default(),
                imported,
            });
            self.handled_dynamic_spans.push(span);
        }
        walk_variable_declarator(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if !self.handled_dynamic_spans.contains(&it.span) {
            self.dynamic.push(DynamicRecord {
                kind: DynamicKind::Import,
                specifier: dynamic_specifier(&it.source),
                span: Span::default(),
                imported: None,
            });
        }
        walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" && !self.handled_dynamic_spans.contains(&it.span) {
                if let Some(argument) = it.arguments.first() {
                    self.dynamic.push(DynamicRecord {
                        kind: DynamicKind::Require,
                        specifier: argument_specifier(argument),
                        span: Span::default(),
                        imported: None,
                    });
                }
            }
        }
        walk_call_expression(self, it);
    }
}

/// A dynamic import whose export use is statically visible at the declaration
/// site, for example `const { getDb } = await import("./db")` or
/// `const editor = (await import("./editor")).default`. `imported` stays
/// `None` for any shape that could use the namespace as a value.
fn declaration_dynamic(
    declarator: &VariableDeclarator<'_>,
) -> Option<(
    DynamicKind,
    DynamicSpecifier,
    oxc_span::Span,
    Option<Vec<String>>,
)> {
    let base = dynamic_base(declarator.init.as_ref()?)?;
    let imported = match &base.property {
        Some(property) => {
            if matches!(property.as_str(), "then" | "catch" | "finally") {
                None
            } else {
                Some(vec![property.clone()])
            }
        }
        None => match &declarator.id.kind {
            // Destructuring a promise is invalid, so only treat an object
            // pattern as an export list when the import is awaited. `require`
            // is synchronous.
            BindingPatternKind::ObjectPattern(pattern)
                if base.kind == DynamicKind::Require || base.awaited =>
            {
                object_pattern_imports(pattern)
            }
            _ => None,
        },
    };
    Some((base.kind, base.specifier, base.span, imported))
}

struct DynamicBase {
    kind: DynamicKind,
    specifier: DynamicSpecifier,
    span: oxc_span::Span,
    /// True when the namespace value is awaited (or produced by `require`), so
    /// member access on it is an export access rather than a Promise property.
    awaited: bool,
    /// The first property after the namespace value, if the value is consumed
    /// directly by a member expression.
    property: Option<String>,
}

fn dynamic_base(expression: &Expression<'_>) -> Option<DynamicBase> {
    match expression {
        Expression::ParenthesizedExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSAsExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSTypeAssertion(inner) => dynamic_base(&inner.expression),
        Expression::TSNonNullExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSInstantiationExpression(inner) => dynamic_base(&inner.expression),
        Expression::AwaitExpression(inner) => {
            let mut base = dynamic_base(&inner.argument)?;
            base.awaited = true;
            Some(base)
        }
        Expression::ImportExpression(import) => Some(DynamicBase {
            kind: DynamicKind::Import,
            specifier: dynamic_specifier(&import.source),
            span: import.span,
            awaited: false,
            property: None,
        }),
        Expression::CallExpression(call) if is_require(call) => {
            let argument = call.arguments.first()?;
            Some(DynamicBase {
                kind: DynamicKind::Require,
                specifier: argument_specifier(argument),
                span: call.span,
                // `require` returns the module exports object directly.
                awaited: true,
                property: None,
            })
        }
        Expression::StaticMemberExpression(member) => {
            let mut base = dynamic_base(&member.object)?;
            if base.property.is_some() {
                return Some(base);
            }
            if base.kind == DynamicKind::Import && !base.awaited {
                return None;
            }
            base.property = Some(member.property.name.to_string());
            Some(base)
        }
        Expression::ComputedMemberExpression(member) => {
            let Expression::StringLiteral(property) = &member.expression else {
                return None;
            };
            let mut base = dynamic_base(&member.object)?;
            if base.property.is_some() {
                return Some(base);
            }
            if base.kind == DynamicKind::Import && !base.awaited {
                return None;
            }
            base.property = Some(property.value.to_string());
            Some(base)
        }
        _ => None,
    }
}

fn is_require(call: &CallExpression<'_>) -> bool {
    matches!(&call.callee, Expression::Identifier(callee) if callee.name == "require")
}

/// Export names bound by an object destructuring pattern. Any rest element,
/// computed key, or non-literal key could read an unknown export, so those
/// shapes widen instead.
fn object_pattern_imports(pattern: &ObjectPattern<'_>) -> Option<Vec<String>> {
    if pattern.rest.is_some() {
        return None;
    }
    let mut names = Vec::new();
    for property in &pattern.properties {
        if property.computed {
            return None;
        }
        let name = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name.to_string(),
            PropertyKey::StringLiteral(literal) => literal.value.to_string(),
            PropertyKey::NumericLiteral(literal) => literal.raw.as_ref().map_or_else(
                || literal.value.to_string(),
                std::string::ToString::to_string,
            ),
            _ => return None,
        };
        names.push(name);
    }
    names.sort();
    names.dedup();
    if names.is_empty() { None } else { Some(names) }
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
