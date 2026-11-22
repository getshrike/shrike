//! Narrow dynamic imports whose module namespace is returned from a local
//! function.
//!
//! `async function load() { return import("./mod"); }` hands the namespace to
//! its callers. When every call site in the same module destructures the
//! result or reads a static member, the import consumes exactly those exports.
//! An opaque call site, a value escape, or a directly exported loader keeps the
//! widening record, so the rule only removes liveness it can prove.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, BindingPatternKind, CallExpression, Declaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression,
    Function, ImportExpression, ObjectPattern, Program, PropertyKey, ReturnStatement,
    StaticMemberExpression, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_arrow_function_expression, walk_call_expression, walk_computed_member_expression,
    walk_declaration, walk_export_default_declaration, walk_export_named_declaration,
    walk_expression_statement, walk_static_member_expression, walk_variable_declarator,
};
use oxc_syntax::scope::ScopeFlags;
use shrike_core::LineIndex;

use crate::summary::{DynamicSpecifier, FileSummary};

use super::refs::span_of;

struct Candidate {
    specifier: String,
    span: oxc_span::Span,
    id_span: oxc_span::Span,
}

/// Find local function declarations whose every return is a literal dynamic
/// import of the same specifier. Mixed returns and bare `return;` statements
/// abstain: the caller could receive a non-namespace value.
///
/// A name bound more than once is ambiguous and abstains. The visitor also
/// rejects a candidate when the name is shadowed by a parameter, local, import,
/// or nested declaration, because calls in that scope do not reach the loader.
#[derive(Default)]
struct ReturnFinder {
    candidates: BTreeMap<String, Candidate>,
    declarations: BTreeSet<String>,
}

impl<'a> Visit<'a> for ReturnFinder {
    fn visit_declaration(&mut self, declaration: &Declaration<'a>) {
        if let Declaration::FunctionDeclaration(function) = declaration {
            self.consider(function);
        }
        walk_declaration(self, declaration);
    }
}

impl ReturnFinder {
    fn consider(&mut self, function: &Function<'_>) {
        let (Some(id), Some(body)) = (&function.id, &function.body) else {
            return;
        };
        let name = id.name.to_string();
        let duplicate = !self.declarations.insert(name.clone());
        let mut returns = ReturnCollector {
            returns: Vec::new(),
            bare: false,
        };
        returns.visit_function_body(body);
        let import = returns.single_import();
        if duplicate {
            self.candidates.remove(&name);
            return;
        }
        match import {
            Some((specifier, span)) => {
                self.candidates.insert(
                    name,
                    Candidate {
                        specifier,
                        span,
                        id_span: id.span,
                    },
                );
            }
            None => {
                // A same-named declaration that does not return an import
                // makes the name ambiguous even when it appears later.
                self.candidates.remove(&name);
            }
        }
    }
}

struct ReturnCollector {
    returns: Vec<Option<(String, oxc_span::Span)>>,
    bare: bool,
}

impl ReturnCollector {
    fn single_import(&self) -> Option<(String, oxc_span::Span)> {
        if self.bare || self.returns.is_empty() {
            return None;
        }
        let mut found: Option<(String, oxc_span::Span)> = None;
        for value in &self.returns {
            let (specifier, span) = value.as_ref()?;
            match &found {
                Some((existing, _)) if existing != specifier => return None,
                Some(_) => {}
                None => found = Some((specifier.clone(), *span)),
            }
        }
        found
    }
}

impl<'a> Visit<'a> for ReturnCollector {
    fn visit_return_statement(&mut self, statement: &ReturnStatement<'a>) {
        match &statement.argument {
            Some(argument) => self.returns.push(returned_import(argument)),
            None => self.bare = true,
        }
    }

    // A nested function has its own return sites and is not part of this
    // function's returned value.
    fn visit_function(&mut self, _function: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_arrow_function_expression(
        &mut self,
        _arrow: &oxc_ast::ast::ArrowFunctionExpression<'a>,
    ) {
    }
}

fn returned_import<'a>(expression: &'a Expression<'a>) -> Option<(String, oxc_span::Span)> {
    let expression = unwrap_await(expression);
    let Expression::ImportExpression(import) = expression else {
        return None;
    };
    let Expression::StringLiteral(literal) = &import.source else {
        return None;
    };
    Some((literal.value.to_string(), import.span))
}

fn unwrap_await<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    match expression {
        Expression::AwaitExpression(awaited) => unwrap_await(&awaited.argument),
        Expression::ParenthesizedExpression(inner) => unwrap_await(&inner.expression),
        Expression::TSAsExpression(inner) => unwrap_await(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap_await(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap_await(&inner.expression),
        Expression::TSInstantiationExpression(inner) => unwrap_await(&inner.expression),
        _ => expression,
    }
}

enum Use {
    Names(BTreeSet<String>),
    Unknown,
}

struct UsesVisitor<'a> {
    candidates: &'a BTreeMap<String, Candidate>,
    uses: BTreeMap<String, Use>,
    bindings: BTreeMap<String, BTreeSet<String>>,
    handled: Vec<oxc_span::Span>,
}

impl<'a> UsesVisitor<'a> {
    fn new(candidates: &'a BTreeMap<String, Candidate>) -> Self {
        Self {
            candidates,
            uses: BTreeMap::new(),
            bindings: BTreeMap::new(),
            handled: Vec::new(),
        }
    }

    fn mark_unknown(&mut self, name: &str) {
        self.uses.insert(name.to_string(), Use::Unknown);
    }

    fn add_names(&mut self, name: &str, names: impl IntoIterator<Item = String>) {
        match self
            .uses
            .entry(name.to_string())
            .or_insert_with(|| Use::Names(BTreeSet::new()))
        {
            Use::Names(set) => set.extend(names),
            Use::Unknown => {}
        }
    }

    fn handle_call_arguments(&mut self, call: &CallExpression<'a>) {
        self.handled.push(call.span);
        for argument in &call.arguments {
            self.visit_argument(argument);
        }
    }

    /// Resolve identifier bindings after the AST walk. The summary's reference
    /// sets already distinguish a static member access (precise) from a bare
    /// use (opaque), and they include uses in nested scopes.
    fn finish(&mut self, summary: &FileSummary) {
        let bindings = std::mem::take(&mut self.bindings);
        for (binding, functions) in &bindings {
            let escaped = summary.module_refs.free.iter().any(|name| name == binding)
                || summary.declarations.iter().any(|declaration| {
                    declaration
                        .references
                        .free
                        .iter()
                        .any(|name| name == binding)
                })
                || summary
                    .local_exports
                    .iter()
                    .any(|export| &export.local == binding);
            let mut names = BTreeSet::new();
            let mut widened = false;
            let members = summary.module_refs.members.iter().chain(
                summary
                    .declarations
                    .iter()
                    .flat_map(|declaration| declaration.references.members.iter()),
            );
            for member in members {
                if &member.object != binding {
                    continue;
                }
                match &member.property {
                    Some(property) => {
                        names.insert(property.clone());
                    }
                    None => {
                        widened = true;
                        break;
                    }
                }
            }
            for function in functions {
                if escaped || widened {
                    self.mark_unknown(function);
                } else {
                    self.add_names(function, names.iter().cloned());
                }
            }
        }
    }
}

impl<'a> Visit<'a> for UsesVisitor<'a> {
    /// An expression-bodied arrow's body is a `FunctionBody` holding one
    /// expression statement, but the value is returned, not discarded. Visit
    /// the expression as a value use instead of classifying it as a statement.
    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        if arrow.expression {
            self.visit_formal_parameters(&arrow.params);
            if let Some(oxc_ast::ast::Statement::ExpressionStatement(statement)) =
                arrow.body.statements.first()
            {
                self.visit_expression(&statement.expression);
                return;
            }
        }
        walk_arrow_function_expression(self, arrow);
    }

    fn visit_binding_identifier(&mut self, identifier: &BindingIdentifier<'a>) {
        if let Some(candidate) = self.candidates.get(identifier.name.as_str())
            && identifier.span != candidate.id_span
        {
            // A parameter, local, import, catch binding, or nested declaration
            // shadows the loader in some scope. Calls there may not reach it.
            self.mark_unknown(identifier.name.as_str());
        }
    }

    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if declarator.kind == VariableDeclarationKind::Const
            && let Some(init) = &declarator.init
            && let Some((name, call, awaited)) = candidate_call(init, self.candidates)
        {
            if !awaited {
                self.mark_unknown(name);
            } else {
                match &declarator.id.kind {
                    BindingPatternKind::ObjectPattern(pattern) => {
                        if let Some(names) = object_pattern_names(pattern) {
                            self.add_names(name, names);
                        } else {
                            self.mark_unknown(name);
                        }
                    }
                    BindingPatternKind::BindingIdentifier(identifier) => {
                        self.bindings
                            .entry(identifier.name.to_string())
                            .or_default()
                            .insert(name.to_string());
                    }
                    _ => self.mark_unknown(name),
                }
            }
            self.handle_call_arguments(call);
            self.visit_binding_pattern(&declarator.id);
            return;
        }
        walk_variable_declarator(self, declarator);
    }

    fn visit_expression_statement(&mut self, statement: &oxc_ast::ast::ExpressionStatement<'a>) {
        if let Some((_name, call, _awaited)) =
            candidate_call(&statement.expression, self.candidates)
        {
            // The namespace is discarded: the import runs for its side effects
            // only and consumes no exports.
            self.handle_call_arguments(call);
            return;
        }
        walk_expression_statement(self, statement);
    }

    fn visit_static_member_expression(&mut self, member: &StaticMemberExpression<'a>) {
        if let Some((name, call, awaited)) = candidate_call(&member.object, self.candidates) {
            if !awaited || matches!(member.property.name.as_str(), "then" | "catch" | "finally") {
                self.mark_unknown(name);
            } else {
                self.add_names(name, [member.property.name.to_string()]);
            }
            self.handle_call_arguments(call);
            return;
        }
        walk_static_member_expression(self, member);
    }

    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        if let Some((name, call, _awaited)) = candidate_call(&member.object, self.candidates) {
            // `(await load())[key]` can read any export.
            self.mark_unknown(name);
            self.handle_call_arguments(call);
            self.visit_expression(&member.expression);
            return;
        }
        walk_computed_member_expression(self, member);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = &call.callee
            && self.candidates.contains_key(callee.name.as_str())
        {
            if !self.handled.contains(&call.span) {
                self.mark_unknown(callee.name.as_str());
            }
            for argument in &call.arguments {
                self.visit_argument(argument);
            }
            return;
        }
        walk_call_expression(self, call);
    }

    fn visit_identifier_reference(&mut self, reference: &oxc_ast::ast::IdentifierReference<'a>) {
        if self.candidates.contains_key(reference.name.as_str()) {
            self.mark_unknown(reference.name.as_str());
        }
    }

    fn visit_export_named_declaration(&mut self, export: &ExportNamedDeclaration<'a>) {
        if let Some(Declaration::FunctionDeclaration(function)) = &export.declaration
            && let Some(id) = &function.id
            && self.candidates.contains_key(id.name.as_str())
        {
            self.mark_unknown(id.name.as_str());
        }
        walk_export_named_declaration(self, export);
    }

    fn visit_export_default_declaration(&mut self, export: &ExportDefaultDeclaration<'a>) {
        if let ExportDefaultDeclarationKind::FunctionDeclaration(function) = &export.declaration
            && let Some(id) = &function.id
            && self.candidates.contains_key(id.name.as_str())
        {
            self.mark_unknown(id.name.as_str());
        }
        walk_export_default_declaration(self, export);
    }
}

fn candidate_call<'a, 'b>(
    expression: &'b Expression<'a>,
    candidates: &BTreeMap<String, Candidate>,
) -> Option<(&'b str, &'b CallExpression<'a>, bool)> {
    let (expression, awaited) = unwrap_await_with_flag(expression);
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    candidates.contains_key(callee.name.as_str()).then_some((
        callee.name.as_str(),
        call.as_ref(),
        awaited,
    ))
}

fn unwrap_await_with_flag<'a, 'b>(expression: &'b Expression<'a>) -> (&'b Expression<'a>, bool) {
    match expression {
        Expression::AwaitExpression(awaited) => (unwrap_await(&awaited.argument), true),
        Expression::ParenthesizedExpression(inner) => unwrap_await_with_flag(&inner.expression),
        Expression::TSAsExpression(inner) => unwrap_await_with_flag(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap_await_with_flag(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap_await_with_flag(&inner.expression),
        Expression::TSInstantiationExpression(inner) => unwrap_await_with_flag(&inner.expression),
        _ => (expression, false),
    }
}

fn object_pattern_names(pattern: &ObjectPattern<'_>) -> Option<Vec<String>> {
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
            PropertyKey::NumericLiteral(literal) => literal
                .raw
                .as_ref()
                .map_or_else(|| literal.value.to_string(), ToString::to_string),
            _ => return None,
        };
        names.push(name);
    }
    Some(names)
}

/// A bare `await import("./m");` statement runs the module for its side
/// effects and discards the namespace, so it consumes no exports.
struct SideEffectFinder {
    spans: Vec<oxc_span::Span>,
    expression_body_depth: usize,
}

impl<'a> Visit<'a> for SideEffectFinder {
    fn visit_expression_statement(&mut self, statement: &oxc_ast::ast::ExpressionStatement<'a>) {
        if self.expression_body_depth == 0
            && let Some(import) = side_effect_import(&statement.expression)
        {
            self.spans.push(import.span);
        }
        walk_expression_statement(self, statement);
    }

    /// An expression-bodied arrow's body statement is its returned value, not
    /// a discarded statement. Do not classify a bare `import()` there as a
    /// side-effect-only import.
    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        if arrow.expression {
            self.expression_body_depth += 1;
        }
        walk_arrow_function_expression(self, arrow);
        if arrow.expression {
            self.expression_body_depth -= 1;
        }
    }
}

fn side_effect_import<'a>(expression: &'a Expression<'a>) -> Option<&'a ImportExpression<'a>> {
    match expression {
        Expression::AwaitExpression(awaited) => side_effect_import(&awaited.argument),
        Expression::ParenthesizedExpression(inner) => side_effect_import(&inner.expression),
        Expression::TSAsExpression(inner) => side_effect_import(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => side_effect_import(&inner.expression),
        Expression::TSNonNullExpression(inner) => side_effect_import(&inner.expression),
        Expression::TSInstantiationExpression(inner) => side_effect_import(&inner.expression),
        Expression::ImportExpression(import) => Some(import),
        _ => None,
    }
}

pub(super) fn narrow_returned_dynamic_imports(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
    summary: &mut FileSummary,
) {
    let mut finder = ReturnFinder::default();
    finder.visit_program(program);

    let mut side_effects = SideEffectFinder {
        spans: Vec::new(),
        expression_body_depth: 0,
    };
    side_effects.visit_program(program);
    for ast_span in side_effects.spans {
        let span = span_of(line_index, source, ast_span);
        if let Some(record) = summary
            .dynamic
            .iter_mut()
            .find(|record| record.span == span && record.imported.is_none())
        {
            record.imported = Some(Vec::new());
        }
    }

    if finder.candidates.is_empty() {
        return;
    }

    let mut uses = UsesVisitor::new(&finder.candidates);
    uses.visit_program(program);
    uses.finish(summary);

    for (name, candidate) in &finder.candidates {
        let imported = match uses.uses.get(name) {
            Some(Use::Names(names)) => Some(names.iter().cloned().collect::<Vec<_>>()),
            Some(Use::Unknown) => None,
            None => Some(Vec::new()),
        };
        let Some(imported) = imported else {
            continue;
        };
        let span = span_of(line_index, source, candidate.span);
        if let Some(record) = summary.dynamic.iter_mut().find(|record| {
            record.span == span
                && record.imported.is_none()
                && matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == &candidate.specifier)
        }) {
            record.imported = Some(imported);
        }
    }
}
