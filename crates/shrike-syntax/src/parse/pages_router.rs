//! Next.js Pages Router page metadata that its build reads from the loaded
//! page module.
//!
//! The Pages Router does not validate data-fetching exports from static
//! analysis. `next build` imports each page and checks:
//!
//! * `getStaticProps`, `getServerSideProps`, and `getStaticPaths` on the
//!   module namespace;
//! * `getInitialProps` on the default-exported component itself, so
//!   `Page.getInitialProps = ...` and `static getInitialProps` on the default
//!   class count, while a named `export const getInitialProps` does not;
//! * `config.runtime` from the `config` export.
//!
//! This module records exactly those shapes. The graph decides whether the
//! file is a Pages Router page and whether a combination fails the build.

use oxc_ast::ast::{
    AssignmentTarget, BindingPatternKind, Class, ClassElement, Declaration,
    ExportDefaultDeclarationKind, Expression, ObjectPropertyKind, Program, PropertyKey,
    PropertyKind, Statement, VariableDeclarationKind,
};
use oxc_syntax::operator::UnaryOperator;
use shrike_core::LineIndex;

use super::next_segment::const_value;
use super::refs::{export_name, span_of};
use crate::summary::{ExportedConstValueRecord, PagesRouterInfo};

pub(super) fn pages_router_info(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> PagesRouterInfo {
    let default_export_local = default_export_local(program);
    PagesRouterInfo {
        default_export_has_get_initial_props: default_export_local
            .as_deref()
            .is_some_and(|name| component_has_get_initial_props(program, name))
            || anonymous_default_class_has_get_initial_props(program),
        pages_config_runtime: config_runtime(program, source, line_index),
        default_export_local,
    }
}

/// The local binding that backs `export default`, when it has a name.
///
/// `export default Page` and `export { Page as default }` both expose the
/// value of `Page`; an anonymous default has no local name to attach a static
/// property to.
fn default_export_local(program: &Program<'_>) -> Option<String> {
    for statement in &program.body {
        match statement {
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                    return function.id.as_ref().map(|id| id.name.to_string());
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    return class.id.as_ref().map(|id| id.name.to_string());
                }
                other => {
                    if let Some(expression) = other.as_expression() {
                        return identifier_name(expression);
                    }
                }
            },
            Statement::ExportNamedDeclaration(export) if export.source.is_none() => {
                for specifier in &export.specifiers {
                    if export_name(&specifier.exported) == "default" {
                        return Some(export_name(&specifier.local));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn identifier_name(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::ParenthesizedExpression(inner) => identifier_name(&inner.expression),
        Expression::TSAsExpression(inner) => identifier_name(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => identifier_name(&inner.expression),
        Expression::TSTypeAssertion(inner) => identifier_name(&inner.expression),
        Expression::TSNonNullExpression(inner) => identifier_name(&inner.expression),
        _ => None,
    }
}

/// `Page.getInitialProps = ...` at module scope, or a static class member on
/// the default-exported class. Next.js checks the property on the loaded
/// component, so only the default export's local binding counts.
fn component_has_get_initial_props(program: &Program<'_>, default_local: &str) -> bool {
    for statement in &program.body {
        match statement {
            Statement::ExpressionStatement(expression) => {
                if let Expression::AssignmentExpression(assignment) = &expression.expression {
                    if let AssignmentTarget::StaticMemberExpression(member) = &assignment.left {
                        if member.property.name == "getInitialProps"
                            && matches!(
                                &member.object,
                                Expression::Identifier(object)
                                    if object.name.as_str() == default_local
                            )
                            && !expression_is_definitely_falsy(&assignment.right)
                        {
                            return true;
                        }
                    }
                }
            }
            Statement::ClassDeclaration(class) => {
                if class.id.as_ref().is_some_and(|id| id.name == default_local)
                    && class_has_static_get_initial_props(class)
                {
                    return true;
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    if class.id.as_ref().is_some_and(|id| id.name == default_local)
                        && class_has_static_get_initial_props(class)
                    {
                        return true;
                    }
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = &export.declaration {
                    if class.id.as_ref().is_some_and(|id| id.name == default_local)
                        && class_has_static_get_initial_props(class)
                    {
                        return true;
                    }
                }
            }
            Statement::VariableDeclaration(variable) => {
                for declarator in &variable.declarations {
                    let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
                        continue;
                    };
                    if id.name != default_local {
                        continue;
                    }
                    if let Some(Expression::ClassExpression(class)) = &declarator.init {
                        if class_has_static_get_initial_props(class) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// `export default class { static getInitialProps() {} }` exposes the static
/// property even though there is no local name to reference.
fn anonymous_default_class_has_get_initial_props(program: &Program<'_>) -> bool {
    program.body.iter().any(|statement| {
        let Statement::ExportDefaultDeclaration(export) = statement else {
            return false;
        };
        let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration else {
            return false;
        };
        class.id.is_none() && class_has_static_get_initial_props(class)
    })
}

fn class_has_static_get_initial_props(class: &Class<'_>) -> bool {
    class.body.body.iter().any(|element| match element {
        ClassElement::MethodDefinition(method) => {
            method.r#static
                && !method.computed
                && property_key_name(&method.key) == Some("getInitialProps")
        }
        ClassElement::PropertyDefinition(property) => {
            property.r#static
                && !property.computed
                && property_key_name(&property.key) == Some("getInitialProps")
                && property
                    .value
                    .as_ref()
                    .is_some_and(|value| !expression_is_definitely_falsy(value))
        }
        _ => false,
    })
}

/// Whether an expression is statically falsy, so `Page.getInitialProps = ...`
/// leaves Next.js's `!!(Comp?.getInitialProps)` check false. Unknown values
/// count as present: the common forms are functions, and a conservative
/// omission would hide a real build failure.
fn expression_is_definitely_falsy(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NullLiteral(_) => true,
        Expression::BooleanLiteral(literal) => !literal.value,
        Expression::NumericLiteral(literal) => literal.value == 0.0,
        Expression::StringLiteral(literal) => literal.value.is_empty(),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            template.quasis.first().is_some_and(|quasi| {
                quasi
                    .value
                    .cooked
                    .as_ref()
                    .unwrap_or(&quasi.value.raw)
                    .is_empty()
            })
        }
        Expression::Identifier(id) => id.name == "undefined",
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::Void,
        Expression::ParenthesizedExpression(inner) => {
            expression_is_definitely_falsy(&inner.expression)
        }
        Expression::TSAsExpression(inner) => expression_is_definitely_falsy(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => {
            expression_is_definitely_falsy(&inner.expression)
        }
        Expression::TSTypeAssertion(inner) => expression_is_definitely_falsy(&inner.expression),
        Expression::TSNonNullExpression(inner) => expression_is_definitely_falsy(&inner.expression),
        _ => false,
    }
}

/// The `runtime` property of `export const config = {...}`.
///
/// Next.js's Pages Router schema accepts `nodejs`, `edge`, and
/// `experimental-edge`; `edge` fails on a non-API page at build time. Other
/// keys are stripped by the schema, so `revalidate` and friends are ignored.
fn config_runtime(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Option<ExportedConstValueRecord> {
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
            if id.name != "config" {
                continue;
            }
            let Some(object) = declarator.init.as_ref().and_then(config_object) else {
                continue;
            };
            for property in &object.properties {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    continue;
                };
                if property.kind != PropertyKind::Init || property.computed {
                    continue;
                }
                if property_key_name(&property.key) != Some("runtime") {
                    continue;
                }
                let value = const_value(&property.value)?;
                return Some(ExportedConstValueRecord {
                    name: "config.runtime".to_string(),
                    value,
                    span: span_of(line_index, source, property.span),
                });
            }
        }
    }
    None
}

fn property_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

/// The object literal Next.js extracts from `config`, unwrapping the same
/// TypeScript assertions its `extractValue` accepts. Parentheses are not
/// unwrapped, matching Next.js.
fn config_object<'a>(
    expression: &'a Expression<'a>,
) -> Option<&'a oxc_ast::ast::ObjectExpression<'a>> {
    match expression {
        Expression::ObjectExpression(object) => Some(object),
        Expression::TSAsExpression(inner) => config_object(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => config_object(&inner.expression),
        Expression::TSTypeAssertion(inner) => config_object(&inner.expression),
        _ => None,
    }
}
