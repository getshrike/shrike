//! Narrow namespace bindings that are only inspected by negative property
//! assertions.
//!
//! `expect(mod).not.toHaveProperty("removedAction")` checks that an export is
//! absent. It cannot make any export live, but a generic namespace use would
//! otherwise widen the dynamic import to the whole module surface. The rule is
//! deliberately narrow: the binding must be the direct first argument of
//! `expect(...)`, the matcher must be `toHaveProperty`, every asserted name
//! must be a string literal, and any other reference to the binding abstains.
//! Positive `expect(mod).toHaveProperty("name")` credits `name` like a static
//! member read.

use std::collections::BTreeSet;

use oxc_ast::ast::{Argument, CallExpression, Expression, Program, StaticMemberExpression};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_call_expression, walk_computed_member_expression, walk_static_member_expression,
};

use crate::summary::{DynamicSpecifier, FileSummary};

struct AssertionUses<'a> {
    binding: &'a str,
    positive: BTreeSet<String>,
    opaque: bool,
}

impl<'a> Visit<'a> for AssertionUses<'_> {
    fn visit_identifier_reference(&mut self, reference: &oxc_ast::ast::IdentifierReference<'a>) {
        if reference.name.as_str() == self.binding {
            self.opaque = true;
        }
    }

    fn visit_static_member_expression(&mut self, member: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &member.object
            && object.name.as_str() == self.binding
        {
            self.positive.insert(member.property.name.to_string());
            return;
        }
        walk_static_member_expression(self, member);
    }

    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        if let Expression::Identifier(object) = &member.object
            && object.name.as_str() == self.binding
        {
            self.opaque = true;
            return;
        }
        walk_computed_member_expression(self, member);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some((negated, names)) = property_assertion(call, self.binding) {
            if !negated {
                self.positive.extend(names);
            }
            // The namespace argument is part of the assertion chain and is
            // handled above. Other arguments are literals by construction.
            return;
        }
        walk_call_expression(self, call);
    }
}

/// Match `expect(binding).not.toHaveProperty("name", ...)` and
/// `expect(binding).toHaveProperty("name", ...)`. Returns whether the
/// assertion is negated and the asserted property names. A non-literal name
/// or a different helper abstains.
fn property_assertion(call: &CallExpression<'_>, binding: &str) -> Option<(bool, Vec<String>)> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name != "toHaveProperty" {
        return None;
    }
    let (negated, expect_call) = match &member.object {
        Expression::CallExpression(expect) => (false, expect.as_ref()),
        Expression::StaticMemberExpression(not) if not.property.name == "not" => {
            let Expression::CallExpression(expect) = &not.object else {
                return None;
            };
            (true, expect.as_ref())
        }
        _ => return None,
    };
    let Expression::Identifier(callee) = &expect_call.callee else {
        return None;
    };
    if callee.name != "expect" {
        return None;
    }
    let Some(Argument::Identifier(argument)) = expect_call.arguments.first() else {
        return None;
    };
    if argument.name.as_str() != binding {
        return None;
    }
    let mut names = Vec::new();
    for argument in &call.arguments {
        let Some(Expression::StringLiteral(literal)) = argument.as_expression() else {
            return None;
        };
        names.push(literal.value.to_string());
    }
    if names.is_empty() {
        return None;
    }
    Some((negated, names))
}

pub(super) fn narrow_assertion_bindings(program: &Program<'_>, summary: &mut FileSummary) {
    let bindings: Vec<(usize, String)> = summary
        .dynamic
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            if record.imported.is_some() {
                return None;
            }
            let Some(binding) = &record.binding else {
                return None;
            };
            if !matches!(&record.specifier, DynamicSpecifier::Literal(_)) {
                return None;
            }
            Some((index, binding.clone()))
        })
        .collect();
    if bindings.is_empty() {
        return;
    }

    for (index, binding) in bindings {
        let mut uses = AssertionUses {
            binding: &binding,
            positive: BTreeSet::new(),
            opaque: false,
        };
        uses.visit_program(program);
        if uses.opaque {
            continue;
        }
        summary.dynamic[index].imported = Some(uses.positive.into_iter().collect());
    }
}
