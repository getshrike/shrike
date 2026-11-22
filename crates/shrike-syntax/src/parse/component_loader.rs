//! Narrow component-loader imports to the target's `default` export.
//!
//! `React.lazy(() => import("./Widget"))` and
//! `dynamic(() => import("./Widget"))` consume only the default export of the
//! loaded module. The collector records the loader's `import()` as a namespace
//! use, which would keep every named export of the target alive. This pass
//! proves that the callee is an unshadowed React/Next.js loader and credits
//! `default` alone, matching fallow 3.45.0's
//! `record_arrow_wrapped_dynamic_import`.
//!
//! A top-level local loader function passed by identifier
//! (`const load = () => import("./Widget"); lazy(load)`) gets the same credit,
//! but only when every reference to the loader is one of those proven loader
//! arguments. A loader that is exported, reassignable, used by another call,
//! or otherwise escapes keeps the whole namespace live.
//!
//! The pass abstains when the loader binding is shadowed, when the import
//! comes from another module or is type-only, and inside TypeScript
//! namespaces. A loader callback that hands the namespace to another call,
//! such as `import("./x").then((m) => m.Widget)`, keeps the narrower credit
//! that the `.then` collector already recorded.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, BindingPatternKind, CallExpression,
    CatchClause, ExportDefaultDeclarationKind, Expression, ForInStatement, ForOfStatement,
    ForStatement, ForStatementInit, ForStatementLeft, Function, IdentifierReference,
    ImportExpression, Program, Statement, StaticBlock, TSModuleDeclaration, VariableDeclaration,
    VariableDeclarationKind,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_arrow_function_expression, walk_block_statement, walk_call_expression, walk_catch_clause,
    walk_for_in_statement, walk_for_of_statement, walk_for_statement, walk_function,
    walk_identifier_reference, walk_static_block, walk_ts_module_declaration,
};

use super::refs::export_name;
use crate::summary::{BindingKind, FileSummary, ImportKind, ImportRecord};

/// What a component-loader import proves about the callback it receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoaderContract {
    /// `import { lazy } from "react"`: a direct callable.
    Function,
    /// `import React from "react"` or `import * as React from "react"`:
    /// the callable is `React.lazy`.
    Namespace,
    /// `import dynamic from "next/dynamic"`: a direct callable.
    NextDynamic,
}

/// Credit `default` for each `import()` directly returned by an inline loader
/// or a local loader function passed to `React.lazy` or `next/dynamic`.
pub(super) fn narrow_component_loader_imports(program: &Program<'_>, summary: &mut FileSummary) {
    let contracts = loader_contracts(&summary.imports);
    if contracts.is_empty() {
        return;
    }

    let local_loaders = local_loader_contracts(program);
    let mut visitor = ComponentLoaderVisitor {
        contracts: &contracts,
        local_loaders: &local_loaders,
        scopes: Vec::new(),
        namespace_depth: 0,
        spans: Vec::new(),
        local_loader_references: BTreeMap::new(),
        local_loader_credited: BTreeMap::new(),
    };
    visitor.visit_program(program);
    visitor.finish_local_loaders();
    if visitor.spans.is_empty() {
        return;
    }

    for record in &mut summary.dynamic {
        if record.imported.is_none()
            && visitor
                .spans
                .iter()
                .any(|(start, end)| *start == record.span.start && *end == record.span.end)
        {
            record.imported = Some(vec!["default".to_string()]);
        }
    }
}

fn loader_contracts(imports: &[ImportRecord]) -> BTreeMap<String, LoaderContract> {
    let mut contracts = BTreeMap::new();
    for import in imports {
        if import.type_only || import.kind != ImportKind::Static {
            continue;
        }
        for binding in &import.bindings {
            if binding.type_only {
                continue;
            }
            let contract = match (
                import.source.as_str(),
                binding.kind,
                binding.imported.as_str(),
            ) {
                ("react", BindingKind::Named, "lazy") => Some(LoaderContract::Function),
                ("react", BindingKind::Default, _)
                | ("react", BindingKind::Namespace, _)
                | ("react", BindingKind::Named, "default") => Some(LoaderContract::Namespace),
                ("next/dynamic", BindingKind::Default, _)
                | ("next/dynamic", BindingKind::Named, "default") => {
                    Some(LoaderContract::NextDynamic)
                }
                _ => None,
            };
            if let Some(contract) = contract {
                contracts.insert(binding.local.clone(), contract);
            }
        }
    }
    contracts
}

/// Top-level local loader functions and the `import()` spans they return.
///
/// Only a `const` arrow/function expression or a function declaration with at
/// least one directly returned static `import()` qualifies. Exported,
/// reassignable, and non-static loaders are deliberately absent: their callers
/// can consume named exports or hand the namespace on.
fn local_loader_contracts(program: &Program<'_>) -> BTreeMap<String, Vec<(u32, u32)>> {
    let exported = exported_local_names(program);
    let mut loaders = BTreeMap::new();
    for statement in &program.body {
        match statement {
            Statement::VariableDeclaration(declaration)
                if declaration.kind == VariableDeclarationKind::Const =>
            {
                for declarator in &declaration.declarations {
                    let BindingPatternKind::BindingIdentifier(identifier) = &declarator.id.kind
                    else {
                        continue;
                    };
                    if exported.contains(identifier.name.as_str()) {
                        continue;
                    }
                    let Some(init) = &declarator.init else {
                        continue;
                    };
                    let mut imports = Vec::new();
                    collect_loader_imports_from_expression(init, &mut imports);
                    register_local_loader(&mut loaders, identifier.name.as_str(), &imports);
                }
            }
            Statement::FunctionDeclaration(function) => {
                let Some(identifier) = &function.id else {
                    continue;
                };
                if exported.contains(identifier.name.as_str()) {
                    continue;
                }
                let Some(body) = &function.body else {
                    continue;
                };
                let mut imports = Vec::new();
                collect_returned_statement_imports(&body.statements, &mut imports);
                register_local_loader(&mut loaders, identifier.name.as_str(), &imports);
            }
            _ => {}
        }
    }
    loaders
}

/// Top-level names exposed through `export { name }` or `export default name`.
/// `export const` and `export function` declarations are skipped by the
/// declaration scan itself.
fn exported_local_names(program: &Program<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for statement in &program.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if export.declaration.is_none() && export.source.is_none() {
                    for specifier in &export.specifiers {
                        names.insert(export_name(&specifier.local));
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::Identifier(identifier) = &export.declaration {
                    names.insert(identifier.name.to_string());
                }
            }
            _ => {}
        }
    }
    names
}

fn register_local_loader(
    loaders: &mut BTreeMap<String, Vec<(u32, u32)>>,
    name: &str,
    imports: &[&ImportExpression<'_>],
) {
    if imports.is_empty() || loaders.contains_key(name) {
        return;
    }
    let mut spans = Vec::with_capacity(imports.len());
    for import in imports {
        if !is_static_import_source(&import.source) {
            return;
        }
        spans.push((import.span.start, import.span.end));
    }
    loaders.insert(name.to_string(), spans);
}

fn is_static_import_source(source: &Expression<'_>) -> bool {
    match source {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(template) => template.expressions.is_empty(),
        _ => false,
    }
}

struct ComponentLoaderVisitor<'c> {
    contracts: &'c BTreeMap<String, LoaderContract>,
    local_loaders: &'c BTreeMap<String, Vec<(u32, u32)>>,
    /// Names declared by each enclosing lexical scope. The module scope is
    /// implicit: an imported loader name is the provenance unless a nested
    /// scope declares the same name.
    scopes: Vec<BTreeSet<String>>,
    namespace_depth: usize,
    /// Byte ranges of `import()` expressions directly returned by a proven
    /// loader callback.
    spans: Vec<(u32, u32)>,
    /// Every unshadowed reference to a local loader binding, including the
    /// references credited below.
    local_loader_references: BTreeMap<String, usize>,
    /// References to a local loader in the first argument of a proven loader
    /// call.
    local_loader_credited: BTreeMap<String, usize>,
}

impl<'a> Visit<'a> for ComponentLoaderVisitor<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.is_loader_call(it) {
            if let Some(argument) = it.arguments.first() {
                let mut imports = Vec::new();
                collect_loader_imports(argument, &mut imports);
                for import in imports {
                    self.spans.push((import.span.start, import.span.end));
                }
                if let Some(expression) = argument.as_expression() {
                    if let Expression::Identifier(identifier) = expression.get_inner_expression() {
                        let name = identifier.name.as_str();
                        if self.local_loaders.contains_key(name) && !self.is_shadowed(name) {
                            *self
                                .local_loader_credited
                                .entry(name.to_string())
                                .or_default() += 1;
                        }
                    }
                }
            }
        }
        walk_call_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let name = it.name.as_str();
        if self.local_loaders.contains_key(name) && !self.is_shadowed(name) {
            *self
                .local_loader_references
                .entry(name.to_string())
                .or_default() += 1;
        }
        walk_identifier_reference(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        let mut names = BTreeSet::new();
        if let Some(id) = &it.id {
            names.insert(id.name.to_string());
        }
        add_parameter_names(&it.params, &mut names);
        if let Some(body) = &it.body {
            collect_hoisted_names(&body.statements, &mut names);
        }
        self.scopes.push(names);
        walk_function(self, it, flags);
        self.scopes.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let mut names = BTreeSet::new();
        add_parameter_names(&it.params, &mut names);
        collect_hoisted_names(&it.body.statements, &mut names);
        self.scopes.push(names);
        walk_arrow_function_expression(self, it);
        self.scopes.pop();
    }

    fn visit_block_statement(&mut self, it: &oxc_ast::ast::BlockStatement<'a>) {
        let mut names = BTreeSet::new();
        add_block_names(&it.body, &mut names);
        self.scopes.push(names);
        walk_block_statement(self, it);
        self.scopes.pop();
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        let mut names = BTreeSet::new();
        if let Some(param) = &it.param {
            add_binding_names(&param.pattern, &mut names);
        }
        self.scopes.push(names);
        walk_catch_clause(self, it);
        self.scopes.pop();
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        let mut names = BTreeSet::new();
        if let Some(ForStatementInit::VariableDeclaration(declaration)) = &it.init {
            add_variable_names(declaration, &mut names);
        }
        self.scopes.push(names);
        walk_for_statement(self, it);
        self.scopes.pop();
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        let mut names = BTreeSet::new();
        if let ForStatementLeft::VariableDeclaration(declaration) = &it.left {
            add_variable_names(declaration, &mut names);
        }
        self.scopes.push(names);
        walk_for_in_statement(self, it);
        self.scopes.pop();
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        let mut names = BTreeSet::new();
        if let ForStatementLeft::VariableDeclaration(declaration) = &it.left {
            add_variable_names(declaration, &mut names);
        }
        self.scopes.push(names);
        walk_for_of_statement(self, it);
        self.scopes.pop();
    }

    fn visit_switch_statement(&mut self, it: &oxc_ast::ast::SwitchStatement<'a>) {
        let mut names = BTreeSet::new();
        for case in &it.cases {
            add_block_names(&case.consequent, &mut names);
        }
        self.scopes.push(names);
        oxc_ast_visit::walk::walk_switch_statement(self, it);
        self.scopes.pop();
    }

    fn visit_static_block(&mut self, it: &StaticBlock<'a>) {
        let mut names = BTreeSet::new();
        add_block_names(&it.body, &mut names);
        self.scopes.push(names);
        walk_static_block(self, it);
        self.scopes.pop();
    }

    fn visit_ts_module_declaration(&mut self, it: &TSModuleDeclaration<'a>) {
        self.namespace_depth += 1;
        walk_ts_module_declaration(self, it);
        self.namespace_depth -= 1;
    }
}

impl ComponentLoaderVisitor<'_> {
    /// Credit the imports of each local loader whose every reference is a
    /// proven loader argument. A loader with an unknown or escaping reference
    /// keeps the namespace-wide record that the collector already emitted.
    fn finish_local_loaders(&mut self) {
        for (name, spans) in self.local_loaders {
            let credited = self.local_loader_credited.get(name).copied().unwrap_or(0);
            if credited == 0 {
                continue;
            }
            let references = self.local_loader_references.get(name).copied().unwrap_or(0);
            if references.saturating_sub(credited) == 0 {
                self.spans.extend(spans.iter().copied());
            }
        }
    }

    fn is_loader_call(&self, call: &CallExpression<'_>) -> bool {
        if self.namespace_depth > 0 {
            return false;
        }
        match call.callee.without_parentheses() {
            Expression::Identifier(callee) => {
                matches!(
                    self.contracts.get(callee.name.as_str()),
                    Some(LoaderContract::Function | LoaderContract::NextDynamic)
                ) && !self.is_shadowed(callee.name.as_str())
            }
            Expression::StaticMemberExpression(member) => {
                if member.property.name != "lazy" {
                    return false;
                }
                let Expression::Identifier(root) = member.object.without_parentheses() else {
                    return false;
                };
                self.contracts.get(root.name.as_str()) == Some(&LoaderContract::Namespace)
                    && !self.is_shadowed(root.name.as_str())
            }
            _ => false,
        }
    }

    fn is_shadowed(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }
}

fn collect_loader_imports<'a>(
    argument: &'a Argument<'a>,
    imports: &mut Vec<&'a ImportExpression<'a>>,
) {
    let Some(expression) = argument.as_expression() else {
        return;
    };
    collect_loader_imports_from_expression(expression, imports);
}

fn collect_loader_imports_from_expression<'a>(
    expression: &'a Expression<'a>,
    imports: &mut Vec<&'a ImportExpression<'a>>,
) {
    match expression.get_inner_expression() {
        Expression::ArrowFunctionExpression(arrow) => {
            if let Some(body) = arrow.get_expression() {
                collect_returned_expression_imports(body, imports);
            } else {
                collect_returned_statement_imports(&arrow.body.statements, imports);
            }
        }
        Expression::FunctionExpression(function) => {
            if let Some(body) = &function.body {
                collect_returned_statement_imports(&body.statements, imports);
            }
        }
        _ => {}
    }
}

/// Collect the `import()` returned by an expression-bodied callback. A
/// conditional or logical return can choose between loaders, so every branch
/// that directly yields an import is credited. A `.then(...)` call is not a
/// direct import: its callback decides which export the loader consumes.
fn collect_returned_expression_imports<'a>(
    expression: &'a Expression<'a>,
    imports: &mut Vec<&'a ImportExpression<'a>>,
) {
    match expression.get_inner_expression() {
        Expression::ImportExpression(import) => imports.push(import),
        Expression::AwaitExpression(awaited) => {
            collect_returned_expression_imports(&awaited.argument, imports);
        }
        Expression::ConditionalExpression(conditional) => {
            collect_returned_expression_imports(&conditional.consequent, imports);
            collect_returned_expression_imports(&conditional.alternate, imports);
        }
        Expression::LogicalExpression(logical) => {
            collect_returned_expression_imports(&logical.left, imports);
            collect_returned_expression_imports(&logical.right, imports);
        }
        Expression::SequenceExpression(sequence) => {
            if let Some(last) = sequence.expressions.last() {
                collect_returned_expression_imports(last, imports);
            }
        }
        _ => {}
    }
}

/// Collect imports returned from a callback body. The search follows nested
/// blocks (`if`, `switch`, `try`, loops, labels) but not nested functions:
/// their `return` statements belong to them.
fn collect_returned_statement_imports<'a>(
    statements: &'a [Statement<'a>],
    imports: &mut Vec<&'a ImportExpression<'a>>,
) {
    for statement in statements {
        collect_returned_statement_import(statement, imports);
    }
}

fn collect_returned_statement_import<'a>(
    statement: &'a Statement<'a>,
    imports: &mut Vec<&'a ImportExpression<'a>>,
) {
    match statement {
        Statement::ReturnStatement(statement) => {
            if let Some(argument) = &statement.argument {
                collect_returned_expression_imports(argument, imports);
            }
        }
        Statement::BlockStatement(block) => {
            collect_returned_statement_imports(&block.body, imports);
        }
        Statement::IfStatement(statement) => {
            collect_returned_statement_import(&statement.consequent, imports);
            if let Some(alternate) = &statement.alternate {
                collect_returned_statement_import(alternate, imports);
            }
        }
        Statement::SwitchStatement(statement) => {
            for case in &statement.cases {
                collect_returned_statement_imports(&case.consequent, imports);
            }
        }
        Statement::TryStatement(statement) => {
            collect_returned_statement_imports(&statement.block.body, imports);
            if let Some(handler) = &statement.handler {
                collect_returned_statement_imports(&handler.body.body, imports);
            }
            if let Some(finalizer) = &statement.finalizer {
                collect_returned_statement_imports(&finalizer.body, imports);
            }
        }
        Statement::LabeledStatement(statement) => {
            collect_returned_statement_import(&statement.body, imports);
        }
        Statement::ForStatement(statement) => {
            collect_returned_statement_import(&statement.body, imports);
        }
        Statement::ForInStatement(statement) => {
            collect_returned_statement_import(&statement.body, imports);
        }
        Statement::ForOfStatement(statement) => {
            collect_returned_statement_import(&statement.body, imports);
        }
        Statement::WhileStatement(statement) => {
            collect_returned_statement_import(&statement.body, imports);
        }
        Statement::DoWhileStatement(statement) => {
            collect_returned_statement_import(&statement.body, imports);
        }
        _ => {}
    }
}

fn add_parameter_names(params: &oxc_ast::ast::FormalParameters<'_>, names: &mut BTreeSet<String>) {
    for parameter in &params.items {
        add_binding_names(&parameter.pattern, names);
    }
    if let Some(rest) = &params.rest {
        add_binding_names(&rest.argument, names);
    }
}

fn add_binding_names(pattern: &BindingPattern<'_>, names: &mut BTreeSet<String>) {
    match &pattern.kind {
        BindingPatternKind::BindingIdentifier(identifier) => {
            names.insert(identifier.name.to_string());
        }
        BindingPatternKind::ObjectPattern(object) => {
            for property in &object.properties {
                add_binding_names(&property.value, names);
            }
            if let Some(rest) = &object.rest {
                add_binding_names(&rest.argument, names);
            }
        }
        BindingPatternKind::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                add_binding_names(element, names);
            }
            if let Some(rest) = &array.rest {
                add_binding_names(&rest.argument, names);
            }
        }
        BindingPatternKind::AssignmentPattern(assignment) => {
            add_binding_names(&assignment.left, names);
        }
    }
}

fn add_variable_names(declaration: &VariableDeclaration<'_>, names: &mut BTreeSet<String>) {
    for declarator in &declaration.declarations {
        add_binding_names(&declarator.id, names);
    }
}

/// Lexical declarations of one block. `var` and function declarations are
/// also hoisted to the enclosing function, but recording them here is a safe
/// over-approximation: the extra scope only makes the pass abstain more.
fn add_block_names(statements: &[Statement<'_>], names: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            Statement::VariableDeclaration(declaration) => {
                add_variable_names(declaration, names);
            }
            Statement::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    names.insert(id.name.to_string());
                }
            }
            Statement::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    names.insert(id.name.to_string());
                }
            }
            _ => {}
        }
    }
}

/// `var` and function declarations hoisted to a function scope. Nested
/// functions and classes are separate scopes, so their declarations do not
/// shadow this function's calls.
fn collect_hoisted_names(statements: &[Statement<'_>], names: &mut BTreeSet<String>) {
    let mut collector = HoistedNameCollector { names };
    for statement in statements {
        collector.visit_statement(statement);
    }
}

struct HoistedNameCollector<'n> {
    names: &'n mut BTreeSet<String>,
}

impl<'a> Visit<'a> for HoistedNameCollector<'_> {
    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if it.kind == VariableDeclarationKind::Var {
            add_variable_names(it, self.names);
        }
        // Initializers are expressions, not declarations of this function
        // scope. Do not walk them.
    }

    fn visit_function(&mut self, it: &Function<'a>, _flags: oxc_syntax::scope::ScopeFlags) {
        if let Some(id) = &it.id {
            self.names.insert(id.name.to_string());
        }
        // A nested function is its own scope.
    }

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, _it: &oxc_ast::ast::Class<'a>) {}

    fn visit_ts_module_declaration(&mut self, _it: &TSModuleDeclaration<'a>) {}
}
