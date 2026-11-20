use oxc_ast::ast::{
    Class, Declaration, ExportDefaultDeclarationKind, Function, Statement, TSModuleDeclarationName,
};
use oxc_ast_visit::Visit;
use shrike_core::{LineIndex, Span};

use crate::summary::{
    DeclarationKind, DeclarationRecord, DynamicRecord, ModuleRefs, UnknownRecord,
};

use super::collector::RefCollector;
use super::refs::binding_names;

type CollectedParts = (ModuleRefs, Vec<DynamicRecord>, Vec<UnknownRecord>);

pub(super) fn declaration_record(
    declaration: &Declaration<'_>,
    statement_span: Span,
    exported: bool,
    default: bool,
    line_index: &LineIndex,
    source: &str,
) -> (DeclarationRecord, Vec<DynamicRecord>, Vec<UnknownRecord>) {
    match declaration {
        Declaration::FunctionDeclaration(function) => {
            let name = function
                .id
                .as_ref()
                .map_or_else(|| "__function".to_string(), |id| id.name.to_string());
            function_record(
                function,
                name,
                statement_span,
                exported,
                default,
                source,
                line_index,
            )
        }
        Declaration::ClassDeclaration(class) => {
            let name = class
                .id
                .as_ref()
                .map_or_else(|| "__class".to_string(), |id| id.name.to_string());
            class_record(
                class,
                name,
                statement_span,
                exported,
                default,
                source,
                line_index,
            )
        }
        Declaration::VariableDeclaration(variable) => {
            let names = variable
                .declarations
                .iter()
                .flat_map(|declarator| binding_names(&declarator.id))
                .collect::<Vec<_>>();
            let name = names
                .first()
                .cloned()
                .unwrap_or_else(|| "__variable".to_string());
            let (refs, dynamic, unknown) = refs_for_variable(variable);
            (
                DeclarationRecord {
                    name,
                    names,
                    kind: DeclarationKind::Variable,
                    span: statement_span,
                    exported,
                    default,
                    type_only: false,
                    r#async: false,
                    directives: Vec::new(),
                    references: refs,
                },
                dynamic,
                unknown,
            )
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            let (refs, dynamic, unknown) = refs_for_type_alias(alias);
            (
                DeclarationRecord {
                    name: alias.id.name.to_string(),
                    names: vec![alias.id.name.to_string()],
                    kind: DeclarationKind::Type,
                    span: statement_span,
                    exported,
                    default,
                    type_only: true,
                    r#async: false,
                    directives: Vec::new(),
                    references: refs,
                },
                dynamic,
                unknown,
            )
        }
        Declaration::TSInterfaceDeclaration(interface) => {
            let (refs, dynamic, unknown) = refs_for_interface(interface);
            (
                DeclarationRecord {
                    name: interface.id.name.to_string(),
                    names: vec![interface.id.name.to_string()],
                    kind: DeclarationKind::Interface,
                    span: statement_span,
                    exported,
                    default,
                    type_only: true,
                    r#async: false,
                    directives: Vec::new(),
                    references: refs,
                },
                dynamic,
                unknown,
            )
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            let (refs, dynamic, unknown) = refs_for_enum(enumeration);
            (
                DeclarationRecord {
                    name: enumeration.id.name.to_string(),
                    names: vec![enumeration.id.name.to_string()],
                    kind: DeclarationKind::Enum,
                    span: statement_span,
                    exported,
                    default,
                    type_only: false,
                    r#async: false,
                    directives: Vec::new(),
                    references: refs,
                },
                dynamic,
                unknown,
            )
        }
        Declaration::TSModuleDeclaration(module) => {
            let name = match &module.id {
                TSModuleDeclarationName::Identifier(id) => id.name.to_string(),
                TSModuleDeclarationName::StringLiteral(literal) => literal.value.to_string(),
            };
            let (refs, dynamic, unknown) = refs_for_module(module);
            (
                DeclarationRecord {
                    name: name.clone(),
                    names: vec![name],
                    kind: DeclarationKind::Namespace,
                    span: statement_span,
                    exported,
                    default,
                    type_only: false,
                    r#async: false,
                    directives: Vec::new(),
                    references: refs,
                },
                dynamic,
                unknown,
            )
        }
        Declaration::TSImportEqualsDeclaration(import) => {
            let name = import.id.name.to_string();
            (
                DeclarationRecord {
                    name: name.clone(),
                    names: vec![name],
                    kind: DeclarationKind::Variable,
                    span: statement_span,
                    exported,
                    default,
                    type_only: import.import_kind.is_type(),
                    r#async: false,
                    directives: Vec::new(),
                    references: ModuleRefs::default(),
                },
                Vec::new(),
                Vec::new(),
            )
        }
    }
}

pub(super) fn function_record(
    function: &Function<'_>,
    name: String,
    span: Span,
    exported: bool,
    default: bool,
    _source: &str,
    _line_index: &LineIndex,
) -> (DeclarationRecord, Vec<DynamicRecord>, Vec<UnknownRecord>) {
    let mut refs = RefCollector::default();
    refs.visit_function(function, oxc_syntax::scope::ScopeFlags::Function);
    let directives = function
        .body
        .as_ref()
        .map(|body| {
            body.directives
                .iter()
                .map(|directive| directive.directive.to_string())
                .collect()
        })
        .unwrap_or_default();
    let (references, dynamic, unknown) = refs.into_parts();
    let mut record = DeclarationRecord {
        name: name.clone(),
        names: vec![name],
        kind: DeclarationKind::Function,
        span,
        exported,
        default,
        type_only: false,
        r#async: function.r#async,
        directives,
        references,
    };
    // The function's own name is not a reference.
    record
        .references
        .identifiers
        .retain(|id| id != &record.name);
    record.references.free.retain(|id| id != &record.name);
    (record, dynamic, unknown)
}

pub(super) fn class_record(
    class: &Class<'_>,
    name: String,
    span: Span,
    exported: bool,
    default: bool,
    _source: &str,
    _line_index: &LineIndex,
) -> (DeclarationRecord, Vec<DynamicRecord>, Vec<UnknownRecord>) {
    let mut refs = RefCollector::default();
    refs.visit_class(class);
    let (references, dynamic, unknown) = refs.into_parts();
    let mut record = DeclarationRecord {
        name: name.clone(),
        names: vec![name],
        kind: DeclarationKind::Class,
        span,
        exported,
        default,
        type_only: false,
        r#async: false,
        directives: Vec::new(),
        references,
    };
    record
        .references
        .identifiers
        .retain(|id| id != &record.name);
    record.references.free.retain(|id| id != &record.name);
    (record, dynamic, unknown)
}

pub(super) fn refs_for_variable(
    variable: &oxc_ast::ast::VariableDeclaration<'_>,
) -> CollectedParts {
    let mut refs = RefCollector::default();
    refs.visit_variable_declaration(variable);
    refs.into_parts()
}

pub(super) fn refs_for_type_alias(
    alias: &oxc_ast::ast::TSTypeAliasDeclaration<'_>,
) -> CollectedParts {
    let mut refs = RefCollector::default();
    refs.visit_ts_type_alias_declaration(alias);
    refs.into_parts()
}

pub(super) fn refs_for_interface(
    interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
) -> CollectedParts {
    let mut refs = RefCollector::default();
    refs.visit_ts_interface_declaration(interface);
    refs.into_parts()
}

pub(super) fn refs_for_enum(enumeration: &oxc_ast::ast::TSEnumDeclaration<'_>) -> CollectedParts {
    let mut refs = RefCollector::default();
    refs.visit_ts_enum_declaration(enumeration);
    refs.into_parts()
}

pub(super) fn refs_for_module(module: &oxc_ast::ast::TSModuleDeclaration<'_>) -> CollectedParts {
    let mut refs = RefCollector::default();
    refs.visit_ts_module_declaration(module);
    refs.into_parts()
}

pub(super) fn collect_expression_or_declaration_refs(
    statement: Option<&Statement<'_>>,
    default_kind: Option<&ExportDefaultDeclarationKind<'_>>,
    _source: &str,
) -> CollectedParts {
    let mut refs = RefCollector::default();
    if let Some(statement) = statement {
        refs.visit_statement(statement);
    }
    if let Some(kind) = default_kind {
        match kind {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                refs.visit_function(function, oxc_syntax::scope::ScopeFlags::Function);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => refs.visit_class(class),
            other => {
                if let Some(expression) = other.as_expression() {
                    refs.visit_expression(expression);
                }
            }
        }
    }
    refs.into_parts()
}
