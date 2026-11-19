//! `oxc_parser` integration and summary extraction.
//!
//! Rule B2: semantics come from the AST, never from a text scan. The AST is
//! dropped at the end of this function; only the summary survives.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, BindingPattern, BindingPatternKind, CallExpression, Class, ComputedMemberExpression,
    Declaration, ExportDefaultDeclarationKind, Expression, Function, ImportDeclarationSpecifier,
    ImportExpression, JSXIdentifier, MemberExpression, ModuleExportName, Statement,
    StaticMemberExpression, TSModuleDeclarationName, TSModuleReference,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_call_expression, walk_computed_member_expression, walk_import_expression,
    walk_static_member_expression,
};
use oxc_parser::Parser;
use oxc_span::SourceType;
use shrike_core::{Diagnostic, LineIndex, Span, content_hash};

use crate::summary::{
    BindingKind, DeclarationKind, DeclarationRecord, DynamicKind, DynamicRecord, DynamicSpecifier,
    ExportAllRecord, FileSummary, ImportBinding, ImportKind, ImportRecord, LocalExportRecord,
    MemberAccess, ModuleRefs, ParseStatus, ReExportRecord, UnknownRecord, UnknownScope,
};

/// Parse a file from disk. I/O errors are reported as a skipped summary.
#[must_use]
pub fn parse_file(path: &str, source: &str) -> FileSummary {
    parse_source(path, source)
}

/// Parse source text and build a deterministic summary.
#[must_use]
pub fn parse_source(path: &str, source: &str) -> FileSummary {
    let hash = content_hash(source);
    let module_kind = module_kind(path).to_string();
    let line_index = LineIndex::new(source);
    let line_count = line_index.line_count();
    let generated = looks_generated(source);

    let source_type = match SourceType::from_path(path) {
        Ok(source_type) => source_type,
        Err(_) => {
            return FileSummary {
                path: path.to_string(),
                content_hash: hash,
                module_kind,
                parse_status: ParseStatus::Skipped,
                parse_message: Some("unsupported source extension".to_string()),
                line_count,
                directives: Vec::new(),
                imports: Vec::new(),
                declarations: Vec::new(),
                local_exports: Vec::new(),
                re_exports: Vec::new(),
                export_alls: Vec::new(),
                module_refs: ModuleRefs::default(),
                dynamic: Vec::new(),
                unknown: Vec::new(),
                generated,
                diagnostics: vec![
                    Diagnostic::warning(
                        "unsupported_extension",
                        "file is not a supported JavaScript or TypeScript source",
                    )
                    .in_file(path),
                ],
            };
        }
    };

    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        let message = parsed
            .errors
            .first()
            .map_or_else(|| "parse error".to_string(), ToString::to_string);
        let message = message.lines().next().unwrap_or("parse error").to_string();
        return FileSummary {
            path: path.to_string(),
            content_hash: hash,
            module_kind,
            parse_status: ParseStatus::Error,
            parse_message: Some(message.clone()),
            line_count,
            directives: Vec::new(),
            imports: Vec::new(),
            declarations: Vec::new(),
            local_exports: Vec::new(),
            re_exports: Vec::new(),
            export_alls: Vec::new(),
            module_refs: ModuleRefs::default(),
            dynamic: Vec::new(),
            unknown: Vec::new(),
            generated,
            diagnostics: vec![Diagnostic::error("parse_error", message).in_file(path)],
        };
    }

    let program = parsed.program;
    let directives: Vec<String> = program
        .directives
        .iter()
        .map(|directive| directive.directive.to_string())
        .collect();

    let mut summary = FileSummary {
        path: path.to_string(),
        content_hash: hash,
        module_kind,
        parse_status: ParseStatus::Ok,
        parse_message: None,
        line_count,
        directives,
        imports: Vec::new(),
        declarations: Vec::new(),
        local_exports: Vec::new(),
        re_exports: Vec::new(),
        export_alls: Vec::new(),
        module_refs: ModuleRefs::default(),
        dynamic: Vec::new(),
        unknown: Vec::new(),
        generated,
        diagnostics: Vec::new(),
    };

    for statement in &program.body {
        extract_statement(statement, source, &line_index, &mut summary);
    }

    mark_local_exports(&mut summary);
    finalize_refs(&mut summary);
    summary
}

fn module_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".d.ts") {
        "dts"
    } else if lower.ends_with(".tsx") {
        "tsx"
    } else if lower.ends_with(".ts") {
        "ts"
    } else if lower.ends_with(".jsx") {
        "jsx"
    } else if lower.ends_with(".mjs") {
        "mjs"
    } else if lower.ends_with(".cjs") {
        "cjs"
    } else {
        "js"
    }
}

fn looks_generated(source: &str) -> bool {
    let head: String = source.chars().take(4096).collect();
    let lower = head.to_ascii_lowercase();
    lower.contains("@generated")
        || lower.contains("do not edit")
        || lower.contains("auto-generated")
        || lower.contains("autogenerated")
        || lower.contains("this file is generated")
}

fn extract_statement(
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
                let mut record = declaration_record(
                    declaration,
                    statement_span,
                    true,
                    false,
                    line_index,
                    source,
                );
                record.type_only = record.type_only || export.export_kind.is_type();
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
                    summary.declarations.push(function_record(
                        function,
                        name,
                        statement_span,
                        true,
                        true,
                        source,
                        line_index,
                    ));
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    let name = class
                        .id
                        .as_ref()
                        .map_or_else(|| "default".to_string(), |id| id.name.to_string());
                    summary.declarations.push(class_record(
                        class,
                        name,
                        statement_span,
                        true,
                        true,
                        source,
                        line_index,
                    ));
                }
                ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                    summary.declarations.push(DeclarationRecord {
                        name: interface.id.name.to_string(),
                        names: vec![interface.id.name.to_string()],
                        kind: DeclarationKind::Interface,
                        span: statement_span,
                        exported: true,
                        default: true,
                        type_only: true,
                        r#async: false,
                        directives: Vec::new(),
                        references: collect_expression_or_declaration_refs(
                            None,
                            Some(&export.declaration),
                            source,
                        ),
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
            summary.declarations.push(function_record(
                function,
                name,
                span_of(line_index, source, function.span),
                false,
                false,
                source,
                line_index,
            ));
        }
        Statement::ClassDeclaration(class) => {
            let name = class
                .id
                .as_ref()
                .map_or_else(|| "__class".to_string(), |id| id.name.to_string());
            summary.declarations.push(class_record(
                class,
                name,
                span_of(line_index, source, class.span),
                false,
                false,
                source,
                line_index,
            ));
        }
        Statement::TSTypeAliasDeclaration(alias) => {
            let refs = refs_for_type_alias(alias);
            summary.declarations.push(DeclarationRecord {
                name: alias.id.name.to_string(),
                names: vec![alias.id.name.to_string()],
                kind: DeclarationKind::Type,
                span: span_of(line_index, source, alias.span),
                exported: false,
                default: false,
                type_only: true,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        Statement::TSInterfaceDeclaration(interface) => {
            let refs = refs_for_interface(interface);
            summary.declarations.push(DeclarationRecord {
                name: interface.id.name.to_string(),
                names: vec![interface.id.name.to_string()],
                kind: DeclarationKind::Interface,
                span: span_of(line_index, source, interface.span),
                exported: false,
                default: false,
                type_only: true,
                r#async: false,
                directives: Vec::new(),
                references: refs,
            });
        }
        Statement::TSEnumDeclaration(enumeration) => {
            let refs = refs_for_enum(enumeration);
            summary.declarations.push(DeclarationRecord {
                name: enumeration.id.name.to_string(),
                names: vec![enumeration.id.name.to_string()],
                kind: DeclarationKind::Enum,
                span: span_of(line_index, source, enumeration.span),
                exported: false,
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
            let refs = refs_for_module(module);
            summary.declarations.push(DeclarationRecord {
                name: name.clone(),
                names: vec![name],
                kind: DeclarationKind::Namespace,
                span: span_of(line_index, source, module.span),
                exported: false,
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

fn declaration_record(
    declaration: &Declaration<'_>,
    statement_span: Span,
    exported: bool,
    default: bool,
    line_index: &LineIndex,
    source: &str,
) -> DeclarationRecord {
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
            let (refs, _, _) = refs_for_variable(variable);
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
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            let refs = refs_for_type_alias(alias);
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
            }
        }
        Declaration::TSInterfaceDeclaration(interface) => {
            let refs = refs_for_interface(interface);
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
            }
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            let refs = refs_for_enum(enumeration);
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
            }
        }
        Declaration::TSModuleDeclaration(module) => {
            let name = match &module.id {
                TSModuleDeclarationName::Identifier(id) => id.name.to_string(),
                TSModuleDeclarationName::StringLiteral(literal) => literal.value.to_string(),
            };
            let refs = refs_for_module(module);
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
            }
        }
        Declaration::TSImportEqualsDeclaration(import) => {
            let name = import.id.name.to_string();
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
            }
        }
    }
}

fn function_record(
    function: &Function<'_>,
    name: String,
    span: Span,
    exported: bool,
    default: bool,
    _source: &str,
    _line_index: &LineIndex,
) -> DeclarationRecord {
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
        references: refs.finish(),
    };
    // The function's own name is not a reference.
    record
        .references
        .identifiers
        .retain(|id| id != &record.name);
    record
}

fn class_record(
    class: &Class<'_>,
    name: String,
    span: Span,
    exported: bool,
    default: bool,
    _source: &str,
    _line_index: &LineIndex,
) -> DeclarationRecord {
    let mut refs = RefCollector::default();
    refs.visit_class(class);
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
        references: refs.finish(),
    };
    record
        .references
        .identifiers
        .retain(|id| id != &record.name);
    record
}

fn refs_for_variable(
    variable: &oxc_ast::ast::VariableDeclaration<'_>,
) -> (ModuleRefs, Vec<DynamicRecord>, Vec<UnknownRecord>) {
    let mut refs = RefCollector::default();
    refs.visit_variable_declaration(variable);
    refs.into_parts()
}

fn refs_for_type_alias(alias: &oxc_ast::ast::TSTypeAliasDeclaration<'_>) -> ModuleRefs {
    let mut refs = RefCollector::default();
    refs.visit_ts_type_alias_declaration(alias);
    refs.finish()
}

fn refs_for_interface(interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>) -> ModuleRefs {
    let mut refs = RefCollector::default();
    refs.visit_ts_interface_declaration(interface);
    refs.finish()
}

fn refs_for_enum(enumeration: &oxc_ast::ast::TSEnumDeclaration<'_>) -> ModuleRefs {
    let mut refs = RefCollector::default();
    refs.visit_ts_enum_declaration(enumeration);
    refs.finish()
}

fn refs_for_module(module: &oxc_ast::ast::TSModuleDeclaration<'_>) -> ModuleRefs {
    let mut refs = RefCollector::default();
    refs.visit_ts_module_declaration(module);
    refs.finish()
}

fn collect_expression_or_declaration_refs(
    statement: Option<&Statement<'_>>,
    default_kind: Option<&ExportDefaultDeclarationKind<'_>>,
    _source: &str,
) -> ModuleRefs {
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
    refs.finish()
}

fn merge_refs(target: &mut ModuleRefs, source: &RefCollector) {
    target
        .identifiers
        .extend(source.identifiers.iter().cloned());
    target.members.extend(source.members.iter().cloned());
    target.jsx.extend(source.jsx.iter().cloned());
    target.identifiers.sort();
    target.identifiers.dedup();
    target.jsx.sort();
    target.jsx.dedup();
    target.members.sort_by(|a, b| {
        a.object
            .cmp(&b.object)
            .then_with(|| a.property.cmp(&b.property))
            .then_with(|| a.computed.cmp(&b.computed))
    });
    target.members.dedup();
}

fn finalize_refs(summary: &mut FileSummary) {
    for declaration in &mut summary.declarations {
        declaration.references.identifiers.sort();
        declaration.references.identifiers.dedup();
        declaration.references.jsx.sort();
        declaration.references.jsx.dedup();
        declaration.references.members.sort_by(|a, b| {
            a.object
                .cmp(&b.object)
                .then_with(|| a.property.cmp(&b.property))
                .then_with(|| a.computed.cmp(&b.computed))
        });
        declaration.references.members.dedup();
    }
    summary.module_refs.identifiers.sort();
    summary.module_refs.identifiers.dedup();
    summary.module_refs.jsx.sort();
    summary.module_refs.jsx.dedup();
    summary.module_refs.members.sort_by(|a, b| {
        a.object
            .cmp(&b.object)
            .then_with(|| a.property.cmp(&b.property))
            .then_with(|| a.computed.cmp(&b.computed))
    });
    summary.module_refs.members.dedup();
}

fn mark_local_exports(summary: &mut FileSummary) {
    let aliases: Vec<(String, String)> = summary
        .local_exports
        .iter()
        .map(|export| (export.local.clone(), export.exported.clone()))
        .collect();
    for declaration in &mut summary.declarations {
        for (local, _exported) in &aliases {
            if declaration.names.iter().any(|name| name == local) {
                declaration.exported = true;
            }
        }
    }
}

fn binding_names(pattern: &BindingPattern<'_>) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_names(pattern, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_binding_names(pattern: &BindingPattern<'_>, names: &mut Vec<String>) {
    match &pattern.kind {
        BindingPatternKind::BindingIdentifier(identifier) => {
            names.push(identifier.name.to_string());
        }
        BindingPatternKind::ObjectPattern(object) => {
            for property in &object.properties {
                collect_binding_names(&property.value, names);
            }
            if let Some(rest) = &object.rest {
                collect_binding_names(&rest.argument, names);
            }
        }
        BindingPatternKind::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                collect_binding_names(element, names);
            }
            if let Some(rest) = &array.rest {
                collect_binding_names(&rest.argument, names);
            }
        }
        BindingPatternKind::AssignmentPattern(assignment) => {
            collect_binding_names(&assignment.left, names);
        }
    }
}

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.to_string(),
        ModuleExportName::IdentifierReference(identifier) => identifier.name.to_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.to_string(),
    }
}

fn span_of(line_index: &LineIndex, source: &str, span: oxc_span::Span) -> Span {
    line_index.span(source, span.start, span.end)
}

#[derive(Default)]
struct RefCollector {
    identifiers: Vec<String>,
    members: Vec<MemberAccess>,
    jsx: Vec<String>,
    dynamic: Vec<DynamicRecord>,
    unknown: Vec<UnknownRecord>,
}

impl RefCollector {
    fn into_parts(self) -> (ModuleRefs, Vec<DynamicRecord>, Vec<UnknownRecord>) {
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

    fn finish(self) -> ModuleRefs {
        ModuleRefs {
            identifiers: self.identifiers,
            members: self.members,
            jsx: self.jsx,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(source: &str, path: &str) -> FileSummary {
        parse_source(path, source)
    }

    #[test]
    fn extracts_imports_and_exports() {
        let source = r#"
import React, { useState as state } from "react";
import type { Foo } from "./foo";
import * as ns from "./ns";
export function formatInvoice(value: string) { return value; }
export default function Page() { return <div />; }
export const a = 1, b = 2;
export { state as localState };
export { thing } from "./barrel";
export * from "./all";
"#;
        let parsed = summary(source, "src/app/page.tsx");
        assert_eq!(parsed.parse_status, ParseStatus::Ok);
        assert_eq!(parsed.imports.len(), 3);
        assert_eq!(parsed.imports[0].bindings.len(), 2);
        assert!(parsed.imports[1].type_only);
        assert!(parsed.imports[2].bindings[0].kind == BindingKind::Namespace);
        assert!(
            parsed
                .declarations
                .iter()
                .any(|d| d.name == "formatInvoice" && d.exported)
        );
        assert!(
            parsed
                .declarations
                .iter()
                .any(|d| d.default && d.name == "Page")
        );
        assert_eq!(parsed.re_exports.len(), 1);
        assert_eq!(parsed.export_alls.len(), 1);
        assert_eq!(parsed.local_exports.len(), 1);
        assert!(parsed.exported_names().contains(&"a".to_string()));
        assert!(parsed.exported_names().contains(&"localState".to_string()));
    }

    #[test]
    fn captures_directives_and_server_actions() {
        let source = r#"
"use client";
export function Component() { return null; }
"#;
        let parsed = summary(source, "src/components/x.tsx");
        assert!(parsed.has_directive("use client"));
    }

    #[test]
    fn captures_dynamic_forms() {
        let source = r#"
const a = import("./literal");
const b = import(`./locales/${lang}.json`);
const c = import(someVar);
const d = require("./required");
const e = require(name);
const f = ns[key];
const g = globalThis[name];
"#;
        let parsed = summary(source, "src/dynamic.ts");
        let kinds: Vec<&DynamicSpecifier> = parsed
            .dynamic
            .iter()
            .map(|record| &record.specifier)
            .collect();
        assert!(
            kinds.iter().any(
                |kind| matches!(kind, DynamicSpecifier::Literal(value) if value == "./literal")
            )
        );
        assert!(kinds.iter().any(
            |kind| matches!(kind, DynamicSpecifier::TemplatePrefix(value) if value == "./locales/")
        ));
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, DynamicSpecifier::Unknown))
        );
        assert!(
            parsed
                .unknown
                .iter()
                .any(|record| matches!(record.scope, UnknownScope::Repo))
        );
        assert!(
            parsed
                .module_refs
                .members
                .iter()
                .any(|member| member.object == "ns" && member.property.is_none())
        );
    }

    #[test]
    fn parse_errors_block_summary() {
        let parsed = summary("export function broken( {", "src/broken.ts");
        assert_eq!(parsed.parse_status, ParseStatus::Error);
        assert!(parsed.declarations.is_empty());
        assert!(parsed.parse_message.is_some());
    }

    #[test]
    fn unsupported_extension_is_skipped() {
        let parsed = summary("hello", "README.md");
        assert_eq!(parsed.parse_status, ParseStatus::Skipped);
    }

    #[test]
    fn generated_markers_are_detected() {
        let parsed = summary("// @generated\nconst x = 1;\n", "src/gen.ts");
        assert!(parsed.generated);
    }
}
