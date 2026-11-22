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

use std::collections::BTreeSet;

use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentTarget, BindingPatternKind, Class, ClassElement,
    Declaration, ExportDefaultDeclarationKind, Expression, FormalParameters, Function,
    FunctionBody, ObjectExpression, ObjectPropertyKind, Program, PropertyKey, PropertyKind,
    Statement, VariableDeclarationKind,
};
use oxc_syntax::operator::UnaryOperator;
use shrike_core::LineIndex;

use super::component_props::{consumed_prop_names, same_file_props_types};
use super::next_segment::const_value;
use super::refs::{export_name, span_of};
use crate::summary::{ExportedConstValueRecord, PageDataKeyRecord, PagesRouterInfo};

pub(super) fn pages_router_info(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> PagesRouterInfo {
    let default_export_local = default_export_local(program);
    let object_types = same_file_props_types(program, source, line_index);
    let consumed = page_component(program, default_export_local.as_deref()).and_then(|component| {
        consumed_prop_names(
            component.params,
            component.body,
            &object_types,
            source,
            line_index,
        )
    });
    let unused_page_data_keys = consumed.map_or_else(Vec::new, |consumed| {
        unused_page_data_keys(
            program,
            source,
            line_index,
            default_export_local.as_deref(),
            &consumed,
        )
    });
    PagesRouterInfo {
        default_export_has_get_initial_props: default_export_local
            .as_deref()
            .is_some_and(|name| component_has_get_initial_props(program, name))
            || anonymous_default_class_has_get_initial_props(program),
        pages_config_runtime: config_runtime(program, source, line_index),
        unused_page_data_keys,
        default_export_local,
    }
}

/// A Pages Router page component whose props can be read from its signature.
struct PageComponent<'a> {
    params: &'a FormalParameters<'a>,
    body: Option<&'a FunctionBody<'a>>,
}

/// The default-exported function component, either declared inline or through
/// a local binding. Class components, HOCs, `memo`/`forwardRef` wrappers, and
/// re-exports abstain.
fn page_component<'a>(
    program: &'a Program<'a>,
    default_local: Option<&str>,
) -> Option<PageComponent<'a>> {
    for statement in &program.body {
        let Statement::ExportDefaultDeclaration(export) = statement else {
            continue;
        };
        match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                return Some(PageComponent {
                    params: function.params.as_ref(),
                    body: function.body.as_deref(),
                });
            }
            ExportDefaultDeclarationKind::ClassDeclaration(_) => return None,
            other => {
                if let Some(component) = other.as_expression().and_then(component_from_expression) {
                    return Some(component);
                }
                break;
            }
        }
    }
    let local = default_local?;
    for statement in &program.body {
        match statement {
            Statement::FunctionDeclaration(function) => {
                if function.id.as_ref().is_some_and(|id| id.name == local) {
                    return Some(PageComponent {
                        params: function.params.as_ref(),
                        body: function.body.as_deref(),
                    });
                }
            }
            Statement::VariableDeclaration(variable) => {
                for declarator in &variable.declarations {
                    let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
                        continue;
                    };
                    if id.name != local {
                        continue;
                    }
                    return declarator.init.as_ref().and_then(component_from_expression);
                }
            }
            _ => {}
        }
    }
    None
}

fn component_from_expression<'a>(expression: &'a Expression<'a>) -> Option<PageComponent<'a>> {
    match expression {
        Expression::ArrowFunctionExpression(arrow) => Some(PageComponent {
            params: arrow.params.as_ref(),
            body: Some(&arrow.body),
        }),
        Expression::FunctionExpression(function) => Some(PageComponent {
            params: function.params.as_ref(),
            body: function.body.as_deref(),
        }),
        Expression::ParenthesizedExpression(inner) => component_from_expression(&inner.expression),
        Expression::TSAsExpression(inner) => component_from_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => component_from_expression(&inner.expression),
        Expression::TSNonNullExpression(inner) => component_from_expression(&inner.expression),
        _ => None,
    }
}

/// Compare the keys returned by Pages Router data functions with the props the
/// page component declares. The comparison abstains unless both sides are
/// statically readable.
fn unused_page_data_keys(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
    default_local: Option<&str>,
    consumed: &BTreeSet<String>,
) -> Vec<PageDataKeyRecord> {
    let exported = exported_names(program);
    let mut records = Vec::new();
    for function in ["getStaticProps", "getServerSideProps"] {
        if !exported.contains(function) {
            continue;
        }
        let Some(value) = top_level_function(program, function) else {
            continue;
        };
        let Some(returned) = returned_expression(&value) else {
            continue;
        };
        let Some(keys) = static_props_keys(returned, false, source, line_index) else {
            continue;
        };
        for (key, span) in keys {
            if !consumed.contains(&key) {
                records.push(PageDataKeyRecord {
                    function: function.to_string(),
                    key,
                    span,
                });
            }
        }
    }
    if let Some(local) = default_local {
        if let Some(value) = get_initial_props_value(program, local) {
            if let Some(returned) = returned_expression(&value) {
                if let Some(keys) = static_props_keys(returned, true, source, line_index) {
                    for (key, span) in keys {
                        if !consumed.contains(&key) {
                            records.push(PageDataKeyRecord {
                                function: "getInitialProps".to_string(),
                                key,
                                span,
                            });
                        }
                    }
                }
            }
        }
    }
    records
}

/// Names exported from the module, including `export { name }` specifiers and
/// declarations. A data function must be exported for Next.js to call it.
fn exported_names(program: &Program<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for statement in &program.body {
        match statement {
            Statement::ExportDefaultDeclaration(_) => {
                names.insert("default".to_string());
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    match declaration {
                        Declaration::FunctionDeclaration(function) => {
                            if let Some(id) = &function.id {
                                names.insert(id.name.to_string());
                            }
                        }
                        Declaration::VariableDeclaration(variable) => {
                            for declarator in &variable.declarations {
                                if let BindingPatternKind::BindingIdentifier(id) =
                                    &declarator.id.kind
                                {
                                    names.insert(id.name.to_string());
                                }
                            }
                        }
                        Declaration::ClassDeclaration(class) => {
                            if let Some(id) = &class.id {
                                names.insert(id.name.to_string());
                            }
                        }
                        _ => {}
                    }
                }
                for specifier in &export.specifiers {
                    names.insert(export_name(&specifier.exported));
                }
            }
            _ => {}
        }
    }
    names
}

enum FunctionValue<'a> {
    Function(&'a Function<'a>),
    Arrow(&'a ArrowFunctionExpression<'a>),
}

impl<'a> FunctionValue<'a> {
    fn body(&self) -> Option<&'a FunctionBody<'a>> {
        match self {
            Self::Function(function) => function.body.as_deref(),
            Self::Arrow(arrow) => Some(&arrow.body),
        }
    }

    fn expression_body(&self) -> Option<&'a Expression<'a>> {
        match self {
            Self::Function(_) => None,
            Self::Arrow(arrow) if arrow.expression => arrow.get_expression(),
            Self::Arrow(_) => None,
        }
    }
}

fn top_level_function<'a>(program: &'a Program<'a>, name: &str) -> Option<FunctionValue<'a>> {
    for statement in &program.body {
        match statement {
            Statement::FunctionDeclaration(function) => {
                if function.id.as_ref().is_some_and(|id| id.name == name) {
                    return Some(FunctionValue::Function(function));
                }
            }
            Statement::VariableDeclaration(variable) => {
                for declarator in &variable.declarations {
                    if let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind
                        && id.name == name
                    {
                        return declarator
                            .init
                            .as_ref()
                            .and_then(function_value_from_expression);
                    }
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                let Some(declaration) = &export.declaration else {
                    continue;
                };
                match declaration {
                    Declaration::FunctionDeclaration(function) => {
                        if function.id.as_ref().is_some_and(|id| id.name == name) {
                            return Some(FunctionValue::Function(function));
                        }
                    }
                    Declaration::VariableDeclaration(variable) => {
                        for declarator in &variable.declarations {
                            if let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind
                                && id.name == name
                            {
                                return declarator
                                    .init
                                    .as_ref()
                                    .and_then(function_value_from_expression);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    None
}

fn function_value_from_expression<'a>(expression: &'a Expression<'a>) -> Option<FunctionValue<'a>> {
    match expression {
        Expression::ArrowFunctionExpression(arrow) => Some(FunctionValue::Arrow(arrow)),
        Expression::FunctionExpression(function) => Some(FunctionValue::Function(function)),
        Expression::ParenthesizedExpression(inner) => {
            function_value_from_expression(&inner.expression)
        }
        Expression::TSAsExpression(inner) => function_value_from_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => {
            function_value_from_expression(&inner.expression)
        }
        Expression::TSNonNullExpression(inner) => function_value_from_expression(&inner.expression),
        _ => None,
    }
}

/// `Page.getInitialProps = ...` on the default-exported component. Next.js
/// reads the property from the loaded component; a named `getInitialProps`
/// export is not consulted.
fn get_initial_props_value<'a>(
    program: &'a Program<'a>,
    default_local: &str,
) -> Option<FunctionValue<'a>> {
    for statement in &program.body {
        let Statement::ExpressionStatement(expression) = statement else {
            continue;
        };
        let Expression::AssignmentExpression(assignment) = &expression.expression else {
            continue;
        };
        let AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
            continue;
        };
        if member.property.name != "getInitialProps" {
            continue;
        }
        if !matches!(&member.object, Expression::Identifier(object) if object.name == default_local)
        {
            continue;
        }
        return function_value_from_expression(&assignment.right);
    }
    None
}

/// The single statically returned expression of a function. A block body may
/// contain control flow, but every return outside a nested function must be
/// the same single return; multiple or bare returns abstain.
fn returned_expression<'a>(value: &FunctionValue<'a>) -> Option<&'a Expression<'a>> {
    if let Some(expression) = value.expression_body() {
        return Some(expression);
    }
    let mut returns = Vec::new();
    let mut opaque = false;
    collect_returns(&value.body()?.statements, &mut returns, &mut opaque);
    if opaque || returns.len() != 1 {
        return None;
    }
    Some(returns[0])
}

/// Collect `return` arguments outside nested functions and classes. The caller
/// requires exactly one, so a conditional return path abstains.
fn collect_returns<'a>(
    statements: &'a [Statement<'a>],
    returns: &mut Vec<&'a Expression<'a>>,
    opaque: &mut bool,
) {
    for statement in statements {
        match statement {
            Statement::ReturnStatement(statement) => match &statement.argument {
                Some(argument) => returns.push(argument),
                None => *opaque = true,
            },
            Statement::BlockStatement(block) => {
                collect_returns(&block.body, returns, opaque);
            }
            Statement::IfStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.consequent), returns, opaque);
                if let Some(alternate) = &statement.alternate {
                    collect_returns(std::slice::from_ref(alternate), returns, opaque);
                }
            }
            Statement::SwitchStatement(statement) => {
                for case in &statement.cases {
                    collect_returns(&case.consequent, returns, opaque);
                }
            }
            Statement::TryStatement(statement) => {
                collect_returns(&statement.block.body, returns, opaque);
                if let Some(handler) = &statement.handler {
                    collect_returns(&handler.body.body, returns, opaque);
                }
                if let Some(finalizer) = &statement.finalizer {
                    collect_returns(&finalizer.body, returns, opaque);
                }
            }
            Statement::ForStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            Statement::ForInStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            Statement::ForOfStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            Statement::WhileStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            Statement::DoWhileStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            Statement::LabeledStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            Statement::WithStatement(statement) => {
                collect_returns(std::slice::from_ref(&statement.body), returns, opaque);
            }
            _ => {}
        }
    }
}

/// The statically known prop keys of a data-function return value.
///
/// `getStaticProps` and `getServerSideProps` return `{ props: { ... } }`;
/// `getInitialProps` returns the props object directly. A spread, computed key,
/// duplicate key, accessor, or non-literal `props` value makes the shape
/// opaque. A missing `props` key returns an empty list: the function
/// deliberately returns no page props (for example `notFound`).
fn static_props_keys(
    expression: &Expression<'_>,
    initial: bool,
    source: &str,
    line_index: &LineIndex,
) -> Option<Vec<(String, shrike_core::Span)>> {
    let object = object_literal(expression)?;
    if initial {
        return object_keys(object, source, line_index);
    }
    let mut props_value = None;
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.computed || property.kind != PropertyKind::Init {
            return None;
        }
        let name = property.key.static_name()?;
        if name != "props" {
            continue;
        }
        if props_value.is_some() {
            return None;
        }
        props_value = Some(&property.value);
    }
    let Some(props_value) = props_value else {
        return Some(Vec::new());
    };
    object_keys(object_literal(props_value)?, source, line_index)
}

fn object_keys(
    object: &ObjectExpression<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Option<Vec<(String, shrike_core::Span)>> {
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.computed || property.kind != PropertyKind::Init {
            return None;
        }
        let name = property.key.static_name()?.into_owned();
        if !seen.insert(name.clone()) {
            return None;
        }
        keys.push((name, span_of(line_index, source, property.span)));
    }
    Some(keys)
}

fn object_literal<'a>(expression: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
    match expression {
        Expression::ObjectExpression(object) => Some(object),
        Expression::ParenthesizedExpression(inner) => object_literal(&inner.expression),
        Expression::TSAsExpression(inner) => object_literal(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => object_literal(&inner.expression),
        Expression::TSTypeAssertion(inner) => object_literal(&inner.expression),
        _ => None,
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
