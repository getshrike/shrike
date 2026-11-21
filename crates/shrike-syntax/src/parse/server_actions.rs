//! Server Action shape detection.
//!
//! A `"use server"` file may only export async functions. The bundler replaces
//! each such export with an action reference, so the module and its imports
//! never enter the client bundle. Shrike uses this predicate to stop the static
//! client/server leak cone at an all-action module, matching fallow 3.45.0's
//! `is_server_action_module`.
//!
//! Inline Server Actions (`export async function f() { "use server"; ... }` or
//! `export const f = async () => { "use server"; ... }`) are actions even when
//! the file has no leading `"use server"` directive. Their export names are
//! recorded so `serverActionPolicy: "protect"` can treat them like whole-file
//! actions.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    BindingPatternKind, Declaration, ExportDefaultDeclarationKind, Expression, FunctionBody,
    ImportDeclarationSpecifier, ModuleExportName, Program, Statement, VariableDeclarationKind,
};
use shrike_core::{LineIndex, Span};

use super::refs::{export_name, span_of};
use crate::summary::InvalidServerExportRecord;

const USE_SERVER: &str = "use server";

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalKind {
    AsyncFunction,
    TypeOnly,
    Other,
}

/// Whether a leading `"use server"` module exports only async functions.
///
/// Type exports do not count, and an overload signature or an ambient
/// declaration is action-shaped because it has no runtime value. A value
/// re-export or any other value export keeps the module out of the action
/// shape: the conservative answer is what keeps the leak cone walking.
pub(super) fn is_server_action_module(program: &Program<'_>) -> bool {
    if !program
        .directives
        .iter()
        .any(|directive| directive.directive.as_str() == USE_SERVER)
    {
        return false;
    }
    let locals = collect_top_level_locals(program);
    program
        .body
        .iter()
        .all(|statement| statement_exports_only_actions(statement, &locals))
}

/// Names of exports whose function body carries an inline `"use server"`
/// directive. `"default"` is recorded for a default-exported function or
/// arrow. A local `export { action }` is covered when the declaration itself
/// carries the directive; an alias of an inline action declared elsewhere is
/// not inferred, matching fallow's exported-only capture.
pub(super) fn inline_server_action_exports(program: &Program<'_>) -> Vec<String> {
    let mut names = BTreeSet::new();
    for statement in &program.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::FunctionDeclaration(function)) => {
                    if let Some(id) = &function.id {
                        if function_body_has_use_server(function.body.as_deref()) {
                            names.insert(id.name.to_string());
                        }
                    }
                }
                Some(Declaration::VariableDeclaration(variable)) => {
                    for declarator in &variable.declarations {
                        if let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind {
                            if declarator
                                .init
                                .as_ref()
                                .is_some_and(expression_body_has_use_server)
                            {
                                names.insert(id.name.to_string());
                            }
                        }
                    }
                }
                _ => {}
            },
            Statement::ExportDefaultDeclaration(export) => {
                let inline = match &export.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        function_body_has_use_server(function.body.as_deref())
                    }
                    other => other
                        .as_expression()
                        .is_some_and(expression_body_has_use_server),
                };
                if inline {
                    names.insert("default".to_string());
                }
            }
            _ => {}
        }
    }
    names.into_iter().collect()
}

/// Value exports of a leading `"use server"` module that Next.js rejects as
/// actions. The runtime validator throws when `typeof action !== "function"`,
/// and the TypeScript plugin rejects any variable initializer that is not an
/// arrow, function expression, call, or identifier. Together those make this a
/// build-failure diagnostic rather than a style rule.
///
/// The check is deliberately conservative: an arrow, function expression, call,
/// or identifier could be a Promise-returning function, so those are skipped.
/// Only the definite union is reported: literals, objects, arrays, class
/// expressions, `new`, tagged templates, `await`, and missing initializers.
/// Type-only exports and value re-exports are skipped because they have no
/// local initializer to classify, and ambient declarations are erased.
pub(super) fn invalid_server_exports(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Vec<InvalidServerExportRecord> {
    if !program
        .directives
        .iter()
        .any(|directive| directive.directive.as_str() == USE_SERVER)
    {
        return Vec::new();
    }
    let locals = collect_top_level_value_kinds(program);
    let mut invalid: BTreeMap<String, Span> = BTreeMap::new();
    for statement in &program.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    record_invalid_declaration(declaration, source, line_index, &mut invalid);
                } else if export.source.is_none() {
                    for specifier in &export.specifiers {
                        if specifier.export_kind.is_type() {
                            continue;
                        }
                        let ModuleExportName::IdentifierReference(local) = &specifier.local else {
                            continue;
                        };
                        if locals.get(local.name.as_str())
                            != Some(&ValueKind::DefinitelyNotFunction)
                        {
                            continue;
                        }
                        invalid
                            .entry(export_name(&specifier.exported))
                            .or_insert_with(|| span_of(line_index, source, specifier.span));
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                let is_invalid = match &export.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(_)
                    | ExportDefaultDeclarationKind::ClassDeclaration(_)
                    | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => false,
                    other => other.as_expression().is_some_and(definitely_not_function),
                };
                if is_invalid {
                    invalid
                        .entry("default".to_string())
                        .or_insert_with(|| span_of(line_index, source, export.span));
                }
            }
            _ => {}
        }
    }
    invalid
        .into_iter()
        .map(|(name, span)| InvalidServerExportRecord { name, span })
        .collect()
}

fn record_invalid_declaration(
    declaration: &Declaration<'_>,
    source: &str,
    line_index: &LineIndex,
    invalid: &mut BTreeMap<String, Span>,
) {
    match declaration {
        Declaration::VariableDeclaration(variable) => {
            // An ambient declaration is erased and has no runtime value; the
            // all-action predicate treats it as action-shaped, so it is not an
            // invalid export.
            if variable.declare {
                return;
            }
            for declarator in &variable.declarations {
                let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
                    continue;
                };
                let invalid_init = match &declarator.init {
                    None => true,
                    Some(init) => definitely_not_function(init),
                };
                if invalid_init {
                    invalid
                        .entry(id.name.to_string())
                        .or_insert_with(|| span_of(line_index, source, declarator.span));
                }
            }
        }
        Declaration::TSEnumDeclaration(enumeration) if !enumeration.declare => {
            invalid
                .entry(enumeration.id.name.to_string())
                .or_insert_with(|| span_of(line_index, source, enumeration.span));
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    TypeOnly,
    DefinitelyNotFunction,
    Unknown,
}

fn collect_top_level_value_kinds(program: &Program<'_>) -> BTreeMap<String, ValueKind> {
    let mut locals = BTreeMap::new();
    let mut record = |name: &str, kind: ValueKind| {
        locals
            .entry(name.to_string())
            .and_modify(|existing| {
                if *existing != kind {
                    *existing = ValueKind::Unknown;
                }
            })
            .or_insert(kind);
    };
    for statement in &program.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    record_value_declaration(declaration, &mut record);
                }
            }
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                    if let Some(id) = &function.id {
                        record(id.name.as_str(), ValueKind::Unknown);
                    }
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    if let Some(id) = &class.id {
                        record(id.name.as_str(), ValueKind::Unknown);
                    }
                }
                _ => {}
            },
            Statement::ImportDeclaration(import) => {
                if let Some(specifiers) = &import.specifiers {
                    for specifier in specifiers {
                        let local = match specifier {
                            ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                                &specifier.local
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                                &specifier.local
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                                &specifier.local
                            }
                        };
                        record(local.name.as_str(), ValueKind::Unknown);
                    }
                }
            }
            other => {
                if let Some(declaration) = other.as_declaration() {
                    record_value_declaration(declaration, &mut record);
                }
            }
        }
    }
    locals
}

fn record_value_declaration(
    declaration: &Declaration<'_>,
    record: &mut impl FnMut(&str, ValueKind),
) {
    match declaration {
        Declaration::FunctionDeclaration(function) => {
            if function.body.is_none() {
                return;
            }
            if let Some(id) = &function.id {
                record(id.name.as_str(), ValueKind::Unknown);
            }
        }
        Declaration::VariableDeclaration(variable) => {
            // Ambient declarations are erased and have no runtime value.
            if variable.declare {
                return;
            }
            for declarator in &variable.declarations {
                let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
                    continue;
                };
                let kind = match &declarator.init {
                    None => ValueKind::DefinitelyNotFunction,
                    Some(init) if definitely_not_function(init) => ValueKind::DefinitelyNotFunction,
                    Some(_) => ValueKind::Unknown,
                };
                record(id.name.as_str(), kind);
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            record(alias.id.name.as_str(), ValueKind::TypeOnly);
        }
        Declaration::TSInterfaceDeclaration(interface) => {
            record(interface.id.name.as_str(), ValueKind::TypeOnly);
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                record(id.name.as_str(), ValueKind::Unknown);
            }
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            if enumeration.declare {
                return;
            }
            record(
                enumeration.id.name.as_str(),
                ValueKind::DefinitelyNotFunction,
            );
        }
        Declaration::TSModuleDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => {}
    }
}

fn definitely_not_function(expression: &Expression<'_>) -> bool {
    !matches!(
        unwrap_expression(expression),
        Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::CallExpression(_)
            | Expression::Identifier(_)
    )
}

fn statement_exports_only_actions(
    statement: &Statement<'_>,
    locals: &BTreeMap<String, LocalKind>,
) -> bool {
    match statement {
        Statement::ExportNamedDeclaration(export) => {
            if let Some(declaration) = &export.declaration {
                return declaration_is_action_only(declaration);
            }
            if export.export_kind.is_type()
                || export
                    .specifiers
                    .iter()
                    .all(|specifier| specifier.export_kind.is_type())
            {
                return true;
            }
            if export.source.is_some() {
                // A value re-export cannot be classified from this module. A
                // conservative `false` keeps the module in the leak cone.
                return false;
            }
            export.specifiers.iter().all(|specifier| {
                if specifier.export_kind.is_type() {
                    return true;
                }
                let ModuleExportName::IdentifierReference(local) = &specifier.local else {
                    return false;
                };
                matches!(
                    locals.get(local.name.as_str()),
                    Some(LocalKind::AsyncFunction | LocalKind::TypeOnly)
                )
            })
        }
        Statement::ExportDefaultDeclaration(export) => {
            default_export_is_action(&export.declaration, locals)
        }
        Statement::ExportAllDeclaration(export) => export.export_kind.is_type(),
        Statement::TSExportAssignment(_) => false,
        _ => true,
    }
}

fn declaration_is_action_only(declaration: &Declaration<'_>) -> bool {
    match declaration {
        Declaration::FunctionDeclaration(function) => {
            function.r#async || function.declare || function.body.is_none()
        }
        Declaration::VariableDeclaration(variable) => {
            variable.declare
                || variable.declarations.iter().all(|declarator| {
                    declarator
                        .init
                        .as_ref()
                        .is_some_and(is_async_function_expression)
                })
        }
        Declaration::TSTypeAliasDeclaration(_) | Declaration::TSInterfaceDeclaration(_) => true,
        Declaration::TSEnumDeclaration(enumeration) => enumeration.declare,
        Declaration::TSModuleDeclaration(module) => module.declare,
        Declaration::ClassDeclaration(class) => class.declare,
        Declaration::TSImportEqualsDeclaration(_) => false,
    }
}

fn default_export_is_action(
    declaration: &ExportDefaultDeclarationKind<'_>,
    locals: &BTreeMap<String, LocalKind>,
) -> bool {
    match declaration {
        ExportDefaultDeclarationKind::FunctionDeclaration(function) => function.r#async,
        ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => true,
        ExportDefaultDeclarationKind::ClassDeclaration(_) => false,
        other => other.as_expression().is_some_and(|expression| {
            if is_async_function_expression(expression) {
                return true;
            }
            let Expression::Identifier(identifier) = unwrap_expression(expression) else {
                return false;
            };
            locals.get(identifier.name.as_str()) == Some(&LocalKind::AsyncFunction)
        }),
    }
}

fn is_async_function_expression(expression: &Expression<'_>) -> bool {
    match unwrap_expression(expression) {
        Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
        Expression::FunctionExpression(function) => function.r#async,
        _ => false,
    }
}

fn expression_body_has_use_server(expression: &Expression<'_>) -> bool {
    match unwrap_expression(expression) {
        Expression::ArrowFunctionExpression(arrow) => {
            function_body_has_use_server(Some(arrow.body.as_ref()))
        }
        Expression::FunctionExpression(function) => {
            function_body_has_use_server(function.body.as_deref())
        }
        _ => false,
    }
}

fn function_body_has_use_server(body: Option<&FunctionBody<'_>>) -> bool {
    body.is_some_and(|body| {
        body.directives
            .iter()
            .any(|directive| directive.directive.as_str() == USE_SERVER)
    })
}

fn unwrap_expression<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    loop {
        current = match current {
            Expression::ParenthesizedExpression(paren) => &paren.expression,
            Expression::TSAsExpression(inner) => &inner.expression,
            Expression::TSSatisfiesExpression(inner) => &inner.expression,
            Expression::TSTypeAssertion(inner) => &inner.expression,
            Expression::TSNonNullExpression(inner) => &inner.expression,
            Expression::TSInstantiationExpression(inner) => &inner.expression,
            _ => return current,
        };
    }
}

/// Classify top-level locals so `export { x }` and `export default x` can
/// resolve a local name. A name declared more than once, or imported, resolves
/// to `Other`. Only `const` async function expressions are actions; `let` and
/// `var` may be reassigned, so they stay conservative.
fn collect_top_level_locals(program: &Program<'_>) -> BTreeMap<String, LocalKind> {
    let mut locals = BTreeMap::new();
    let mut record = |name: &str, kind: LocalKind| {
        locals
            .entry(name.to_string())
            .and_modify(|existing| {
                if *existing != kind {
                    *existing = LocalKind::Other;
                }
            })
            .or_insert(kind);
    };
    for statement in &program.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    record_declaration(declaration, &mut record);
                }
            }
            Statement::ImportDeclaration(import) => {
                if let Some(specifiers) = &import.specifiers {
                    for specifier in specifiers {
                        let local = match specifier {
                            ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                                &specifier.local
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                                &specifier.local
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                                &specifier.local
                            }
                        };
                        record(local.name.as_str(), LocalKind::Other);
                    }
                }
            }
            other => {
                if let Some(declaration) = other.as_declaration() {
                    record_declaration(declaration, &mut record);
                }
            }
        }
    }
    locals
}

fn record_declaration(declaration: &Declaration<'_>, record: &mut impl FnMut(&str, LocalKind)) {
    match declaration {
        Declaration::FunctionDeclaration(function) => {
            // Overload signatures have no body and do not change the kind.
            if function.body.is_none() {
                return;
            }
            if let Some(id) = &function.id {
                let kind = if function.r#async {
                    LocalKind::AsyncFunction
                } else {
                    LocalKind::Other
                };
                record(id.name.as_str(), kind);
            }
        }
        Declaration::VariableDeclaration(variable) => {
            for declarator in &variable.declarations {
                let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
                    continue;
                };
                let is_async_const = variable.kind == VariableDeclarationKind::Const
                    && declarator
                        .init
                        .as_ref()
                        .is_some_and(is_async_function_expression);
                let kind = if is_async_const {
                    LocalKind::AsyncFunction
                } else {
                    LocalKind::Other
                };
                record(id.name.as_str(), kind);
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            record(alias.id.name.as_str(), LocalKind::TypeOnly);
        }
        Declaration::TSInterfaceDeclaration(interface) => {
            record(interface.id.name.as_str(), LocalKind::TypeOnly);
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                record(id.name.as_str(), LocalKind::Other);
            }
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            record(enumeration.id.name.as_str(), LocalKind::Other);
        }
        Declaration::TSModuleDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => {}
    }
}
