use oxc_ast::ast::{
    Declaration, ExportDefaultDeclarationKind, ImportDeclarationSpecifier, Statement,
    TSModuleDeclarationKind, TSModuleDeclarationName, TSModuleReference,
};
use oxc_ast_visit::Visit;
use shrike_core::LineIndex;

use crate::summary::{
    BindingKind, DeclarationKind, DeclarationRecord, ExportAllRecord, FileSummary, ImportBinding,
    ImportKind, ImportRecord, LocalExportRecord, ReExportRecord,
};

use super::collector::RefCollector;
use super::declarations::{
    class_record, collect_expression_or_declaration_refs, declaration_record, function_record,
    refs_for_enum, refs_for_interface, refs_for_module, refs_for_type_alias, refs_for_variable,
};
use super::refs::{binding_names, export_name, merge_refs, span_of};

pub(super) fn extract_statement(
    statement: &Statement<'_>,
    source: &str,
    line_index: &LineIndex,
    summary: &mut FileSummary,
) {
    match statement {
        Statement::ImportDeclaration(import) => {
            let source_text = import.source.value.to_string();
            let type_only = import.import_kind.is_type();
            let mut bindings = Vec::new();
            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                            bindings.push(ImportBinding {
                                imported: specifier.imported.name().to_string(),
                                local: specifier.local.name.to_string(),
                                kind: BindingKind::Named,
                                type_only: type_only || specifier.import_kind.is_type(),
                                span: span_of(line_index, source, specifier.span),
                            });
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            bindings.push(ImportBinding {
                                imported: "default".to_string(),
                                local: specifier.local.name.to_string(),
                                kind: BindingKind::Default,
                                type_only,
                                span: span_of(line_index, source, specifier.span),
                            });
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            bindings.push(ImportBinding {
                                imported: "*".to_string(),
                                local: specifier.local.name.to_string(),
                                kind: BindingKind::Namespace,
                                type_only,
                                span: span_of(line_index, source, specifier.span),
                            });
                        }
                    }
                }
            }
            let side_effect_only = bindings.is_empty();
            summary.imports.push(ImportRecord {
                source: source_text,
                bindings,
                type_only,
                span: span_of(line_index, source, import.span),
                kind: ImportKind::Static,
                side_effect_only,
            });
        }
        Statement::ExportNamedDeclaration(export) => {
            let statement_span = span_of(line_index, source, export.span);
            if let Some(declaration) = &export.declaration {
                let (mut record, dynamic, unknown) = declaration_record(
                    declaration,
                    statement_span,
                    true,
                    false,
                    line_index,
                    source,
                );
                record.type_only = record.type_only || export.export_kind.is_type();
                if let Declaration::TSModuleDeclaration(module) = declaration {
                    if module.declare || module.kind == TSModuleDeclarationKind::Global {
                        let name = match &module.id {
                            TSModuleDeclarationName::Identifier(id) => id.name.to_string(),
                            TSModuleDeclarationName::StringLiteral(literal) => {
                                literal.value.to_string()
                            }
                        };
                        summary.ambient_modules.push(name);
                    }
                }
                summary.dynamic.extend(dynamic);
                summary.unknown.extend(unknown);
                summary.declarations.push(record);
            } else if let Some(source_literal) = &export.source {
                let source_text = source_literal.value.to_string();
                let type_only = export.export_kind.is_type();
                for specifier in &export.specifiers {
                    summary.re_exports.push(ReExportRecord {
                        imported: export_name(&specifier.local),
                        exported: export_name(&specifier.exported),
                        source: source_text.clone(),
                        span: span_of(line_index, source, specifier.span),
                        statement_span,
                        type_only: type_only || specifier.export_kind.is_type(),
                    });
                }
            } else {
                let type_only = export.export_kind.is_type();
                for specifier in &export.specifiers {
                    summary.local_exports.push(LocalExportRecord {
                        local: export_name(&specifier.local),
                        exported: export_name(&specifier.exported),
                        span: span_of(line_index, source, specifier.span),
                        statement_span,
                        type_only: type_only || specifier.export_kind.is_type(),
                    });
                }
            }
        }
        Statement::ExportDefaultDeclaration(export) => {
            let statement_span = span_of(line_index, source, export.span);
            match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                    let name = function
                        .id
                        .as_ref()
                        .map_or_else(|| "default".to_string(), |id| id.name.to_string());
                    let (record, dynamic, unknown) = function_record(
                        function,
                        name,
                        statement_span,
                        true,
                        true,
                        source,
                        line_index,
                    );
                    summary.dynamic.extend(dynamic);
                    summary.unknown.extend(unknown);
                    summary.declarations.push(record);
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    let name = class
                        .id
                        .as_ref()
                        .map_or_else(|| "default".to_string(), |id| id.name.to_string());
                    let (record, dynamic, unknown) =
                        class_record(class, name, statement_span, true, true, source, line_index);
                    summary.dynamic.extend(dynamic);
                    summary.unknown.extend(unknown);
                    summary.declarations.push(record);
                }
                ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                    let (references, dynamic, unknown) = collect_expression_or_declaration_refs(
                        None,
                        Some(&export.declaration),
                        source,
                    );
                    summary.dynamic.extend(dynamic);
                    summary.unknown.extend(unknown);
                    summary.declarations.push(DeclarationRecord {
                        name: interface.id.name.to_string(),
                        names: vec![interface.id.name.to_string()],
                        kind: DeclarationKind::Interface,
                        span: statement_span,
                        exported: true,
                        directly_exported: true,
                        default: true,
                        type_only: true,
                        r#async: false,
                        directives: Vec::new(),
                        references,
                    });
                }
                other => {
                    let mut refs = RefCollector::default();
                    if let Some(expression) = other.as_expression() {
                        refs.visit_expression(expression);
                    }
                    let (module_refs, dynamic, unknown) = refs.into_parts();
                    summary.declarations.push(DeclarationRecord {
                        name: "default".to_string(),
                        names: vec!["default".to_string()],
                        kind: DeclarationKind::Default,
                        span: statement_span,
                        exported: true,
                        directly_exported: true,
                        default: true,
                        type_only: false,
                        r#async: false,
                        directives: Vec::new(),
                        references: module_refs,
                    });
                    summary.dynamic.extend(dynamic);
                    summary.unknown.extend(unknown);
                }
            }
        }
        Statement::ExportAllDeclaration(export) => {
            summary.export_alls.push(ExportAllRecord {
                source: export.source.value.to_string(),
                exported: export.exported.as_ref().map(export_name),
                span: span_of(line_index, source, export.span),
                type_only: export.export_kind.is_type(),
            });
        }
        Statement::TSImportEqualsDeclaration(import) => {
            if let TSModuleReference::ExternalModuleReference(reference) = &import.module_reference
            {
                summary.imports.push(ImportRecord {
                    source: reference.expression.value.to_string(),
                    bindings: vec![ImportBinding {
                        imported: "*".to_string(),
                        local: import.id.name.to_string(),
                        kind: BindingKind::Namespace,
                        type_only: import.import_kind.is_type(),
                        span: span_of(line_index, source, import.span),
                    }],
                    type_only: import.import_kind.is_type(),
                    span: span_of(line_index, source, import.span),
                    kind: ImportKind::ImportEquals,
                    side_effect_only: false,
                });
            }
        }
        Statement::VariableDeclaration(variable) => {
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
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary
                .module_refs
                .identifiers
                .extend(refs.identifiers.iter().cloned());
            summary.module_refs.free.extend(refs.free.iter().cloned());
            summary
                .module_refs
                .members
                .extend(refs.members.iter().cloned());
            summary.module_refs.jsx.extend(refs.jsx.iter().cloned());
            summary.declarations.push(DeclarationRecord {
                name,
                names,
                kind: DeclarationKind::Variable,
                span: span_of(line_index, source, variable.span),
                exported: false,
                directly_exported: false,
                default: false,
                type_only: false,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        Statement::FunctionDeclaration(function) => {
            let name = function
                .id
                .as_ref()
                .map_or_else(|| "__function".to_string(), |id| id.name.to_string());
            let (record, dynamic, unknown) = function_record(
                function,
                name,
                span_of(line_index, source, function.span),
                false,
                false,
                source,
                line_index,
            );
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary.declarations.push(record);
        }
        Statement::ClassDeclaration(class) => {
            let name = class
                .id
                .as_ref()
                .map_or_else(|| "__class".to_string(), |id| id.name.to_string());
            let (record, dynamic, unknown) = class_record(
                class,
                name,
                span_of(line_index, source, class.span),
                false,
                false,
                source,
                line_index,
            );
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary.declarations.push(record);
        }
        Statement::TSTypeAliasDeclaration(alias) => {
            let (refs, dynamic, unknown) = refs_for_type_alias(alias);
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary.declarations.push(DeclarationRecord {
                name: alias.id.name.to_string(),
                names: vec![alias.id.name.to_string()],
                kind: DeclarationKind::Type,
                span: span_of(line_index, source, alias.span),
                exported: false,
                directly_exported: false,
                default: false,
                type_only: true,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        Statement::TSInterfaceDeclaration(interface) => {
            let (refs, dynamic, unknown) = refs_for_interface(interface);
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary.declarations.push(DeclarationRecord {
                name: interface.id.name.to_string(),
                names: vec![interface.id.name.to_string()],
                kind: DeclarationKind::Interface,
                span: span_of(line_index, source, interface.span),
                exported: false,
                directly_exported: false,
                default: false,
                type_only: true,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        Statement::TSEnumDeclaration(enumeration) => {
            let (refs, dynamic, unknown) = refs_for_enum(enumeration);
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary.declarations.push(DeclarationRecord {
                name: enumeration.id.name.to_string(),
                names: vec![enumeration.id.name.to_string()],
                kind: DeclarationKind::Enum,
                span: span_of(line_index, source, enumeration.span),
                exported: false,
                directly_exported: false,
                default: false,
                type_only: false,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        Statement::TSModuleDeclaration(module) => {
            let name = match &module.id {
                TSModuleDeclarationName::Identifier(id) => id.name.to_string(),
                TSModuleDeclarationName::StringLiteral(literal) => literal.value.to_string(),
            };
            let (refs, dynamic, unknown) = refs_for_module(module);
            if module.declare || module.kind == TSModuleDeclarationKind::Global {
                summary.ambient_modules.push(name.clone());
            }
            summary.dynamic.extend(dynamic);
            summary.unknown.extend(unknown);
            summary.declarations.push(DeclarationRecord {
                name: name.clone(),
                names: vec![name],
                kind: DeclarationKind::Namespace,
                span: span_of(line_index, source, module.span),
                exported: false,
                directly_exported: false,
                default: false,
                type_only: false,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        other => {
            let mut refs = RefCollector::default();
            refs.visit_statement(other);
            merge_refs(&mut summary.module_refs, &refs);
            summary.dynamic.extend(refs.dynamic);
            summary.unknown.extend(refs.unknown);
        }
    }
}
