//! Harvest top-level TypeScript enums for member-level dead-code checks.
//!
//! The parser records only what it can read without guessing: each member's
//! name and span, plus the sibling members its initializer reads. A computed
//! template member name, a duplicate member name, or a `declare enum` makes the
//! whole enum abstain. The graph decides liveness and cross-file accesses.

use std::collections::BTreeSet;

use oxc_ast::ast::{
    Declaration, IdentifierReference, Program, Statement, TSEnumDeclaration, TSEnumMemberName,
};
use oxc_ast_visit::Visit;
use shrike_core::LineIndex;

use crate::summary::{EnumMemberRecord, EnumRecord};

use super::refs::span_of;

pub(super) fn collect(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Vec<EnumRecord> {
    let mut records = Vec::new();
    for statement in &program.body {
        match statement {
            Statement::TSEnumDeclaration(enumeration) => {
                records.push(record(enumeration, source, line_index));
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::TSEnumDeclaration(enumeration)) = &export.declaration {
                    records.push(record(enumeration, source, line_index));
                }
            }
            _ => {}
        }
    }
    records
}

fn record(enumeration: &TSEnumDeclaration<'_>, source: &str, line_index: &LineIndex) -> EnumRecord {
    let mut names = Vec::new();
    let mut opaque = false;
    for member in &enumeration.body.members {
        let Some(name) = member_name(&member.id) else {
            opaque = true;
            continue;
        };
        if names.iter().any(|existing| existing == &name) {
            opaque = true;
        }
        names.push(name);
    }

    let member_names: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let mut members = Vec::with_capacity(enumeration.body.members.len());
    for member in &enumeration.body.members {
        let Some(name) = member_name(&member.id) else {
            continue;
        };
        let mut initializer_members = BTreeSet::new();
        if let Some(initializer) = &member.initializer {
            let mut refs = InitializerRefs::default();
            refs.visit_expression(initializer);
            initializer_members.extend(
                refs.names
                    .into_iter()
                    .filter(|name| member_names.contains(name.as_str())),
            );
        }
        members.push(EnumMemberRecord {
            name,
            span: span_of(line_index, source, member.span),
            initializer_members: initializer_members.into_iter().collect(),
        });
    }

    EnumRecord {
        name: enumeration.id.name.to_string(),
        span: span_of(line_index, source, enumeration.span),
        ambient: enumeration.declare,
        opaque,
        members,
    }
}

fn member_name(name: &TSEnumMemberName<'_>) -> Option<String> {
    match name {
        TSEnumMemberName::Identifier(identifier) => Some(identifier.name.to_string()),
        TSEnumMemberName::String(literal) | TSEnumMemberName::ComputedString(literal) => {
            Some(literal.value.to_string())
        }
        // A template literal can embed an expression. Only a fully static name
        // is safe to match against a member access; anything else abstains.
        TSEnumMemberName::ComputedTemplateString(_) => None,
    }
}

/// Identifier references inside one member initializer. Intersecting these
/// with the enum's member names credits `A` in `B = A` without treating an
/// unrelated local with the same name as a member.
#[derive(Default)]
struct InitializerRefs {
    names: BTreeSet<String>,
}

impl<'a> Visit<'a> for InitializerRefs {
    fn visit_identifier_reference(&mut self, reference: &IdentifierReference<'a>) {
        self.names.insert(reference.name.to_string());
    }
}
