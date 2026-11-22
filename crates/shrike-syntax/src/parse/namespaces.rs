//! Harvest top-level TypeScript namespaces for member-level dead-code checks.
//!
//! The parser records exported members of top-level namespaces, including
//! nested exported namespaces with dotted paths (`Inner.deep`). It also records
//! full dotted member reads because the ordinary `MemberAccess` record keeps
//! only the first property of `Outer.Inner.deep`. The graph decides liveness
//! and cross-file accesses.
//!
//! A namespace declared without `export`, a declaration file, `declare global`,
//! and a `declare module "..."` augmentation abstain: their members are either
//! not part of the module surface or are type-level side effects.

use oxc_ast::ast::{
    BindingPattern, BindingPatternKind, CallExpression, ComputedMemberExpression, Declaration,
    ExportDefaultDeclarationKind, Expression, IdentifierReference, JSXMemberExpression,
    JSXMemberExpressionObject, NewExpression, Program, PropertyKey, Statement,
    StaticMemberExpression, TSImportEqualsDeclaration, TSImportType, TSImportTypeQualifier,
    TSModuleDeclaration, TSModuleDeclarationBody, TSModuleDeclarationKind, TSModuleDeclarationName,
    TSModuleReference, TSQualifiedName, TSType, TSTypeName, TSTypeQuery, TSTypeQueryExprName,
    VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk;
use shrike_core::LineIndex;

use crate::summary::{NamespaceMemberRecord, NamespaceMemberUseRecord, NamespaceRecord};

use super::refs::span_of;

pub(super) struct NamespaceCollection {
    pub(super) records: Vec<NamespaceRecord>,
    pub(super) member_uses: Vec<NamespaceMemberUseRecord>,
}

pub(super) fn collect(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> NamespaceCollection {
    let mut records = Vec::new();
    let mut body_uses = Vec::new();
    for statement in &program.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::TSModuleDeclaration(module)) = &export.declaration {
                    if let Some(record) = root_record(module, source, line_index) {
                        // Unqualified references inside the namespace body can
                        // read any lexically visible exported member. Record
                        // them against the root so `Body.helper` credits the
                        // member read by `Body.entry()`.
                        let mut body = BodyUseCollector {
                            object: record.name.clone(),
                            prefix: Vec::new(),
                            uses: &mut body_uses,
                        };
                        body.visit_root(module);
                        records.push(record);
                    }
                }
            }
            // A local namespace is not part of the module surface. Its members
            // are not reported even when a later `export { Ns }` exposes the
            // whole object: every member is then public and nothing is dead.
            Statement::TSModuleDeclaration(_) => {}
            _ => {}
        }
    }

    let mut uses = UseCollector::default();
    uses.visit_program(program);
    uses.member_uses.extend(body_uses);
    uses.member_uses
        .sort_by(|a, b| a.object.cmp(&b.object).then_with(|| a.path.cmp(&b.path)));
    uses.member_uses.dedup();

    NamespaceCollection {
        records,
        member_uses: uses.member_uses,
    }
}

fn root_record(
    module: &TSModuleDeclaration<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Option<NamespaceRecord> {
    if module.kind == TSModuleDeclarationKind::Global {
        return None;
    }
    let TSModuleDeclarationName::Identifier(id) = &module.id else {
        // `declare module "pkg"` augments another module; its exports are not
        // this file's namespace surface.
        return None;
    };
    let mut members = Vec::new();
    record_members(module, "", source, line_index, &mut members);
    Some(NamespaceRecord {
        name: id.name.to_string(),
        span: span_of(line_index, source, module.span),
        ambient: module.declare,
        members,
    })
}

/// Collect exported declarations in one namespace body. `prefix` is the dotted
/// path from the exported root (`Inner` for members of `Outer.Inner`).
fn record_members(
    module: &TSModuleDeclaration<'_>,
    prefix: &str,
    source: &str,
    line_index: &LineIndex,
    members: &mut Vec<NamespaceMemberRecord>,
) {
    match &module.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            for statement in &block.body {
                let Statement::ExportNamedDeclaration(export) = statement else {
                    continue;
                };
                let Some(declaration) = &export.declaration else {
                    continue;
                };
                record_member(declaration, prefix, source, line_index, members);
            }
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(child)) => {
            // `namespace A.B {}`: B is implicitly a member of A.
            let TSModuleDeclarationName::Identifier(id) = &child.id else {
                return;
            };
            let name = join(prefix, id.name.as_str());
            members.push(NamespaceMemberRecord {
                name: name.clone(),
                span: span_of(line_index, source, id.span),
            });
            record_members(child, &name, source, line_index, members);
        }
        None => {}
    }
}

fn record_member(
    declaration: &Declaration<'_>,
    prefix: &str,
    source: &str,
    line_index: &LineIndex,
    members: &mut Vec<NamespaceMemberRecord>,
) {
    match declaration {
        Declaration::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                push_member(
                    members,
                    prefix,
                    id.name.as_str(),
                    id.span,
                    source,
                    line_index,
                );
            }
        }
        Declaration::VariableDeclaration(variable) => {
            for declarator in &variable.declarations {
                collect_binding_members(&declarator.id, prefix, source, line_index, members);
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                push_member(
                    members,
                    prefix,
                    id.name.as_str(),
                    id.span,
                    source,
                    line_index,
                );
            }
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            push_member(
                members,
                prefix,
                enumeration.id.name.as_str(),
                enumeration.id.span,
                source,
                line_index,
            );
        }
        Declaration::TSInterfaceDeclaration(interface) => {
            push_member(
                members,
                prefix,
                interface.id.name.as_str(),
                interface.id.span,
                source,
                line_index,
            );
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            push_member(
                members,
                prefix,
                alias.id.name.as_str(),
                alias.id.span,
                source,
                line_index,
            );
        }
        Declaration::TSModuleDeclaration(namespace) => {
            let TSModuleDeclarationName::Identifier(id) = &namespace.id else {
                return;
            };
            let name = join(prefix, id.name.as_str());
            members.push(NamespaceMemberRecord {
                name: name.clone(),
                span: span_of(line_index, source, id.span),
            });
            record_members(namespace, &name, source, line_index, members);
        }
        Declaration::TSImportEqualsDeclaration(import) => {
            push_member(
                members,
                prefix,
                import.id.name.as_str(),
                import.id.span,
                source,
                line_index,
            );
        }
    }
}

fn push_member(
    members: &mut Vec<NamespaceMemberRecord>,
    prefix: &str,
    name: &str,
    span: oxc_span::Span,
    source: &str,
    line_index: &LineIndex,
) {
    members.push(NamespaceMemberRecord {
        name: join(prefix, name),
        span: span_of(line_index, source, span),
    });
}

fn collect_binding_members(
    pattern: &BindingPattern<'_>,
    prefix: &str,
    source: &str,
    line_index: &LineIndex,
    members: &mut Vec<NamespaceMemberRecord>,
) {
    match &pattern.kind {
        BindingPatternKind::BindingIdentifier(id) => {
            push_member(
                members,
                prefix,
                id.name.as_str(),
                id.span,
                source,
                line_index,
            );
        }
        BindingPatternKind::ObjectPattern(object) => {
            for property in &object.properties {
                collect_binding_members(&property.value, prefix, source, line_index, members);
            }
            if let Some(rest) = &object.rest {
                collect_binding_members(&rest.argument, prefix, source, line_index, members);
            }
        }
        BindingPatternKind::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                collect_binding_members(element, prefix, source, line_index, members);
            }
            if let Some(rest) = &array.rest {
                collect_binding_members(&rest.argument, prefix, source, line_index, members);
            }
        }
        BindingPatternKind::AssignmentPattern(assignment) => {
            collect_binding_members(&assignment.left, prefix, source, line_index, members);
        }
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

#[derive(Default)]
struct UseCollector {
    member_uses: Vec<NamespaceMemberUseRecord>,
    /// The visitor is walking the object of a member expression. Only the
    /// outermost chain is recorded, so `Outer.Inner.deep` produces one use
    /// instead of `Inner` plus `Inner.deep`.
    member_object_depth: usize,
    /// The visitor is walking a call or `new` callee. A function merged with a
    /// namespace can be called without reading any namespace member, so its
    /// bare identifier is not a whole-namespace use.
    suppress_whole: usize,
}

impl UseCollector {
    fn record_chain(&mut self, chain: Option<(String, Vec<String>)>) {
        let Some((object, path)) = chain else {
            return;
        };
        if path.is_empty() {
            return;
        }
        self.member_uses.push(NamespaceMemberUseRecord {
            object,
            path: path.join("."),
        });
    }

    fn record_whole(&mut self, object: String) {
        self.member_uses.push(NamespaceMemberUseRecord {
            object,
            path: "*".to_string(),
        });
    }
}

impl<'a> Visit<'a> for UseCollector {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        // A bare value read can hand the whole namespace object to a receiver
        // (`Object.keys(Whole)`, `{ ...Spread }`, `const copy = Legacy`).
        // Calls and `new` suppress their callee because a function merged with
        // a namespace is callable without exposing namespace members.
        if self.suppress_whole == 0 && self.member_object_depth == 0 {
            self.record_whole(it.name.to_string());
        }
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.suppress_whole += 1;
        self.visit_expression(&it.callee);
        self.suppress_whole -= 1;
        if let Some(type_arguments) = &it.type_arguments {
            self.visit_ts_type_parameter_instantiation(type_arguments);
        }
        for argument in &it.arguments {
            self.visit_argument(argument);
        }
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.suppress_whole += 1;
        self.visit_expression(&it.callee);
        self.suppress_whole -= 1;
        if let Some(type_arguments) = &it.type_arguments {
            self.visit_ts_type_parameter_instantiation(type_arguments);
        }
        for argument in &it.arguments {
            self.visit_argument(argument);
        }
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if self.member_object_depth == 0 {
            let mut chain = expression_chain(&it.object);
            if let Some((_, path)) = &mut chain {
                path.push(it.property.name.to_string());
            }
            self.record_chain(chain);
        }
        self.member_object_depth += 1;
        self.visit_expression(&it.object);
        self.member_object_depth -= 1;
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if self.member_object_depth == 0 {
            if let Some((object, mut path)) = expression_chain(&it.object) {
                if let Some(property) = literal_property(&it.expression) {
                    path.push(property);
                    self.record_chain(Some((object, path)));
                } else {
                    // A computed key can select any member. Credit all rather
                    // than invent a finding.
                    self.record_whole(object);
                }
            }
        }
        self.member_object_depth += 1;
        self.visit_expression(&it.object);
        self.member_object_depth -= 1;
        self.visit_expression(&it.expression);
    }

    fn visit_ts_type(&mut self, it: &TSType<'a>) {
        // A bare type reference (`: Color`) names the namespace's type side,
        // not a runtime value that can expose every member. Qualified type
        // names still record their member path, and `typeof Ns` records a
        // whole-value use explicitly.
        self.suppress_whole += 1;
        walk::walk_ts_type(self, it);
        self.suppress_whole -= 1;
    }

    fn visit_ts_qualified_name(&mut self, it: &TSQualifiedName<'a>) {
        if self.member_object_depth == 0 {
            self.record_chain(qualified_chain(it));
        }
        self.member_object_depth += 1;
        walk::walk_ts_qualified_name(self, it);
        self.member_object_depth -= 1;
    }

    fn visit_ts_type_query(&mut self, it: &TSTypeQuery<'a>) {
        if let TSTypeQueryExprName::IdentifierReference(id) = &it.expr_name {
            // `typeof Legacy` reads the namespace object as a whole.
            self.record_whole(id.name.to_string());
        }
        walk::walk_ts_type_query(self, it);
    }

    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        if let Some((source, path)) = import_type_chain(it) {
            self.member_uses.push(NamespaceMemberUseRecord {
                object: format!("import(\"{source}\")"),
                path,
            });
        }
        walk::walk_ts_import_type(self, it);
    }

    fn visit_jsx_member_expression(&mut self, it: &JSXMemberExpression<'a>) {
        if self.member_object_depth == 0 {
            self.record_chain(jsx_chain(it));
        }
        self.member_object_depth += 1;
        if let JSXMemberExpressionObject::MemberExpression(inner) = &it.object {
            self.visit_jsx_member_expression(inner);
        }
        self.member_object_depth -= 1;
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        let mut precise = false;
        if let (BindingPatternKind::ObjectPattern(pattern), Some(Expression::Identifier(id))) =
            (&it.id.kind, &it.init)
        {
            precise = true;
            let mut whole = pattern.rest.is_some();
            for property in &pattern.properties {
                if property.computed {
                    whole = true;
                    continue;
                }
                let Some(name) = property_key_name(&property.key) else {
                    whole = true;
                    continue;
                };
                if matches!(
                    property.value.kind,
                    BindingPatternKind::ObjectPattern(_) | BindingPatternKind::ArrayPattern(_)
                ) {
                    whole = true;
                    continue;
                }
                self.member_uses.push(NamespaceMemberUseRecord {
                    object: id.name.to_string(),
                    path: name,
                });
            }
            if whole {
                self.record_whole(id.name.to_string());
            }
        }
        // The destructured object is a precise member read, not a whole-value
        // escape. Visiting its initializer would credit every member.
        self.visit_binding_pattern(&it.id);
        if !precise {
            if let Some(init) = &it.init {
                self.visit_expression(init);
            }
        }
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        match &it.module_reference {
            TSModuleReference::QualifiedName(name) => {
                self.record_chain(qualified_chain(name));
            }
            TSModuleReference::IdentifierReference(id) => {
                self.record_whole(id.name.to_string());
            }
            TSModuleReference::ExternalModuleReference(_)
            | TSModuleReference::ThisExpression(_) => {}
        }
        walk::walk_ts_import_equals_declaration(self, it);
    }

    fn visit_export_default_declaration(
        &mut self,
        it: &oxc_ast::ast::ExportDefaultDeclaration<'a>,
    ) {
        if let ExportDefaultDeclarationKind::Identifier(id) = &it.declaration {
            self.record_whole(id.name.to_string());
        }
        walk::walk_export_default_declaration(self, it);
    }
}

/// Collect unqualified references inside one exported namespace body.
///
/// `namespace Body { const local = 1; export const helper = local + 1;
/// export function entry() { return helper; } }` reads `helper` without a
/// dotted path. The graph cannot see that read through the ordinary member
/// records, so record every identifier against each lexically visible
/// namespace prefix. Over-crediting is safe; a missed read is not.
struct BodyUseCollector<'a> {
    object: String,
    /// Nested namespaces between the exported root and the current body.
    prefix: Vec<String>,
    uses: &'a mut Vec<NamespaceMemberUseRecord>,
}

impl BodyUseCollector<'_> {
    fn record_identifier(&mut self, name: &str) {
        for depth in (0..=self.prefix.len()).rev() {
            let mut path = self.prefix[..depth].join(".");
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(name);
            self.uses.push(NamespaceMemberUseRecord {
                object: self.object.clone(),
                path,
            });
        }
    }

    fn visit_root(&mut self, module: &TSModuleDeclaration<'_>) {
        match &module.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.visit_ts_module_block(block);
            }
            // `export namespace Dotted.Inner {}`: references inside `Inner`
            // are members of the dotted root.
            Some(TSModuleDeclarationBody::TSModuleDeclaration(child)) => {
                self.visit_nested(child);
            }
            None => {}
        }
    }

    fn visit_nested(&mut self, module: &TSModuleDeclaration<'_>) {
        let TSModuleDeclarationName::Identifier(id) = &module.id else {
            return;
        };
        self.prefix.push(id.name.to_string());
        match &module.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.visit_ts_module_block(block);
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(child)) => {
                self.visit_nested(child);
            }
            None => {}
        }
        self.prefix.pop();
    }
}

impl<'a> Visit<'a> for BodyUseCollector<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.record_identifier(it.name.as_str());
    }

    fn visit_ts_module_declaration(&mut self, it: &TSModuleDeclaration<'a>) {
        self.visit_nested(it);
    }
}

fn expression_chain(expression: &Expression<'_>) -> Option<(String, Vec<String>)> {
    match expression {
        Expression::Identifier(id) => Some((id.name.to_string(), Vec::new())),
        Expression::StaticMemberExpression(member) => {
            let (object, mut path) = expression_chain(&member.object)?;
            path.push(member.property.name.to_string());
            Some((object, path))
        }
        Expression::ComputedMemberExpression(member) => {
            let (object, mut path) = expression_chain(&member.object)?;
            path.push(literal_property(&member.expression)?);
            Some((object, path))
        }
        Expression::ParenthesizedExpression(paren) => expression_chain(&paren.expression),
        Expression::TSAsExpression(expression) => expression_chain(&expression.expression),
        Expression::TSSatisfiesExpression(expression) => expression_chain(&expression.expression),
        Expression::TSNonNullExpression(expression) => expression_chain(&expression.expression),
        Expression::ChainExpression(chain) => match &chain.expression {
            oxc_ast::ast::ChainElement::StaticMemberExpression(member) => {
                let (object, mut path) = expression_chain(&member.object)?;
                path.push(member.property.name.to_string());
                Some((object, path))
            }
            oxc_ast::ast::ChainElement::ComputedMemberExpression(member) => {
                let (object, mut path) = expression_chain(&member.object)?;
                path.push(literal_property(&member.expression)?);
                Some((object, path))
            }
            _ => None,
        },
        _ => None,
    }
}

fn qualified_chain(name: &TSQualifiedName<'_>) -> Option<(String, Vec<String>)> {
    match &name.left {
        TSTypeName::IdentifierReference(id) => {
            Some((id.name.to_string(), vec![name.right.name.to_string()]))
        }
        TSTypeName::QualifiedName(left) => {
            let (object, mut path) = qualified_chain(left)?;
            path.push(name.right.name.to_string());
            Some((object, path))
        }
        TSTypeName::ThisExpression(_) => None,
    }
}

fn jsx_chain(member: &JSXMemberExpression<'_>) -> Option<(String, Vec<String>)> {
    match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => {
            Some((id.name.to_string(), vec![member.property.name.to_string()]))
        }
        JSXMemberExpressionObject::MemberExpression(inner) => {
            let (object, mut path) = jsx_chain(inner)?;
            path.push(member.property.name.to_string());
            Some((object, path))
        }
        JSXMemberExpressionObject::ThisExpression(_) => None,
    }
}

fn import_type_chain(it: &TSImportType<'_>) -> Option<(String, String)> {
    let oxc_ast::ast::TSType::TSLiteralType(literal) = &it.argument else {
        return None;
    };
    let oxc_ast::ast::TSLiteral::StringLiteral(value) = &literal.literal else {
        return None;
    };
    let qualifier = it.qualifier.as_ref()?;
    let path = import_qualifier_path(qualifier);
    if path.is_empty() {
        return None;
    }
    Some((value.value.to_string(), path.join(".")))
}

fn import_qualifier_path(qualifier: &TSImportTypeQualifier<'_>) -> Vec<String> {
    match qualifier {
        TSImportTypeQualifier::Identifier(id) => vec![id.name.to_string()],
        TSImportTypeQualifier::QualifiedName(name) => {
            let mut path = import_qualifier_path(&name.left);
            path.push(name.right.name.to_string());
            path
        }
    }
}

fn literal_property(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::NumericLiteral(literal) => Some(
            literal
                .raw
                .as_ref()
                .map_or_else(|| literal.value.to_string(), ToString::to_string),
        ),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .map(|quasi| quasi.value.raw.to_string()),
        _ => None,
    }
}

fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::NumericLiteral(literal) => Some(
            literal
                .raw
                .as_ref()
                .map_or_else(|| literal.value.to_string(), ToString::to_string),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use shrike_core::LineIndex;

    use crate::parse::parse_source;

    fn summary(source: &str) -> crate::summary::FileSummary {
        parse_source("src/namespaces.ts", source)
    }

    #[test]
    fn records_exported_members_and_dotted_nested_namespaces() {
        let summary = summary(
            r#"
export namespace Outer {
  export const used = 1;
  export function unusedFn(): void {}
  export namespace Inner {
    export type Used = string;
    export type Unused = number;
  }
  namespace Hidden {
    export const hidden = 1;
  }
}
"#,
        );
        let outer = &summary.namespaces[0];
        assert_eq!(outer.name, "Outer");
        let names: Vec<&str> = outer.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            ["used", "unusedFn", "Inner", "Inner.Used", "Inner.Unused"]
        );
    }

    #[test]
    fn skips_local_namespaces_and_ambient_modules() {
        let summary = summary(
            r#"
namespace Local {
  export const member = 1;
}
declare module "pkg" {
  export namespace Augmented {
    export const member: number;
  }
}
declare global {
  namespace Global {
    export const member: number;
  }
}
"#,
        );
        assert!(summary.namespaces.is_empty(), "{:?}", summary.namespaces);
    }

    #[test]
    fn records_dotted_root_declarations() {
        let summary = summary(
            r#"
export namespace Dotted.Inner {
  export const used = 1;
  export const unused = 2;
}
"#,
        );
        let dotted = &summary.namespaces[0];
        assert_eq!(dotted.name, "Dotted");
        let names: Vec<&str> = dotted.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Inner", "Inner.used", "Inner.unused"]);
    }

    #[test]
    fn records_full_member_chains_and_destructuring() {
        let summary = summary(
            r#"
import { Outer, Legacy } from "./legacy";
export const deep = Outer.Inner.deep;
const { used } = Legacy;
export const value = used;
export type Shape = Outer.Inner.Shape;
"#,
        );
        let paths: Vec<(&str, &str)> = summary
            .namespace_member_uses
            .iter()
            .map(|use_| (use_.object.as_str(), use_.path.as_str()))
            .collect();
        assert!(paths.contains(&("Outer", "Inner.deep")), "{paths:?}");
        assert!(paths.contains(&("Legacy", "used")), "{paths:?}");
        assert!(paths.contains(&("Outer", "Inner.Shape")), "{paths:?}");
    }

    #[test]
    fn records_inline_import_type_chains() {
        let summary = summary(
            r#"
export type A = typeof import("./lib").Legacy.used;
export type B = import("./lib").Shapes.Point;
"#,
        );
        let paths: Vec<(&str, &str)> = summary
            .namespace_member_uses
            .iter()
            .map(|use_| (use_.object.as_str(), use_.path.as_str()))
            .collect();
        assert!(
            paths.contains(&("import(\"./lib\")", "Legacy.used")),
            "{paths:?}"
        );
        assert!(
            paths.contains(&("import(\"./lib\")", "Shapes.Point")),
            "{paths:?}"
        );
    }

    #[test]
    fn records_computed_access_as_whole() {
        let summary = summary(
            r#"
import { Legacy } from "./legacy";
const key = "used";
export const value = Legacy[key];
"#,
        );
        assert!(
            summary
                .namespace_member_uses
                .iter()
                .any(|use_| use_.object == "Legacy" && use_.path == "*"),
            "{:?}",
            summary.namespace_member_uses
        );
    }

    #[test]
    fn parses_the_line_index_but_does_not_depend_on_it() {
        let _ = LineIndex::new("");
        let summary = summary("export namespace N { export const a = 1; }");
        assert_eq!(summary.namespaces[0].members[0].name, "a");
    }
}
