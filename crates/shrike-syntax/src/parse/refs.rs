use oxc_ast::ast::{BindingPattern, BindingPatternKind, ModuleExportName};
use shrike_core::{LineIndex, Span};

use crate::summary::{FileSummary, ModuleRefs};

use super::collector::RefCollector;

pub(super) fn merge_refs(target: &mut ModuleRefs, source: &RefCollector) {
    target
        .identifiers
        .extend(source.identifiers.iter().cloned());
    target.free.extend(source.free.iter().cloned());
    target.members.extend(source.members.iter().cloned());
    target.jsx.extend(source.jsx.iter().cloned());
    target.identifiers.sort();
    target.identifiers.dedup();
    target.free.sort();
    target.free.dedup();
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

pub(super) fn finalize_refs(summary: &mut FileSummary) {
    for declaration in &mut summary.declarations {
        declaration.references.identifiers.sort();
        declaration.references.identifiers.dedup();
        declaration.references.free.sort();
        declaration.references.free.dedup();
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
    summary.module_refs.free.sort();
    summary.module_refs.free.dedup();
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

pub(super) fn mark_local_exports(summary: &mut FileSummary) {
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

pub(super) fn binding_names(pattern: &BindingPattern<'_>) -> Vec<String> {
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

pub(super) fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.to_string(),
        ModuleExportName::IdentifierReference(identifier) => identifier.name.to_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.to_string(),
    }
}

pub(super) fn span_of(line_index: &LineIndex, source: &str, span: oxc_span::Span) -> Span {
    line_index.span(source, span.start, span.end)
}
