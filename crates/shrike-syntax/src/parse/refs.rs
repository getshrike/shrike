use std::collections::{BTreeMap, BTreeSet};

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

/// Narrow `const ns = await import("./m")` namespace bindings after the whole
/// module has been visited. A binding used only through static member accesses
/// consumes exactly those exports; a bare use, computed access, or export of
/// the binding can reach any export and keeps the default widening record.
///
/// Name collisions with a shadowing local can only add names, never remove a
/// real use: the collector records every bare identifier reference and every
/// static member access in the module without scope filtering.
pub(super) fn narrow_dynamic_bindings(summary: &mut FileSummary) {
    let mut bindings: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, record) in summary.dynamic.iter().enumerate() {
        if let Some(binding) = &record.binding {
            bindings.entry(binding.clone()).or_default().push(index);
        }
    }
    if bindings.is_empty() {
        return;
    }

    let mut escapes: BTreeSet<String> = summary.module_refs.free.iter().cloned().collect();
    for declaration in &summary.declarations {
        escapes.extend(declaration.references.free.iter().cloned());
    }
    escapes.extend(
        summary
            .local_exports
            .iter()
            .map(|export| export.local.clone()),
    );

    let mut members: Vec<(String, Option<String>)> = summary
        .module_refs
        .members
        .iter()
        .map(|member| (member.object.clone(), member.property.clone()))
        .collect();
    for declaration in &summary.declarations {
        members.extend(
            declaration
                .references
                .members
                .iter()
                .map(|member| (member.object.clone(), member.property.clone())),
        );
    }

    for (binding, indices) in bindings {
        if escapes.contains(&binding) {
            continue;
        }
        let mut names = BTreeSet::new();
        let mut widened = false;
        for (object, property) in &members {
            if object != &binding {
                continue;
            }
            match property {
                Some(property) => {
                    names.insert(property.clone());
                }
                None => {
                    widened = true;
                    break;
                }
            }
        }
        if widened {
            continue;
        }
        // No escape and no static member access means the binding is never
        // read. An empty list is precise: the import runs for side effects and
        // consumes no exports.
        let names: Vec<String> = names.into_iter().collect();
        for index in indices {
            summary.dynamic[index].imported = Some(names.clone());
        }
    }
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
