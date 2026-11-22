//! Harvest top-level classes for member-level dead-code checks.
//!
//! The parser records only what it can read without guessing: each method and
//! property's name, kind, and span, the `this`/`super`/`ClassName` member reads
//! inside the class body, and local bindings that hold an instance. Anything
//! that could make a member reachable through an untracked path (computed
//! names, decorators, an unknown base class, an index signature, or a class or
//! instance value that escapes) marks the class opaque. The graph decides
//! liveness and cross-file accesses.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    AccessorProperty, BindingPattern, BindingPatternKind, Class, ClassElement, Declaration,
    ExportDefaultDeclarationKind, Expression, MethodDefinition, MethodDefinitionKind,
    MethodDefinitionType, PropertyDefinition, PropertyDefinitionType, PropertyKey, Statement,
    TSAccessibility, TSType, TSTypeName, VariableDeclaration,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_computed_member_expression, walk_static_member_expression, walk_variable_declarator,
};
use oxc_span::GetSpan;
use shrike_core::{LineIndex, Span};

use crate::summary::{
    ClassBindingRecord, ClassMemberKind, ClassMemberRecord, ClassRecord, FileSummary, MemberAccess,
};

use super::refs::span_of;

pub(super) struct ClassCollection {
    pub(super) classes: Vec<ClassRecord>,
    pub(super) bindings: Vec<ClassBindingRecord>,
    pub(super) member_accesses: Vec<MemberAccess>,
    /// Names used as whole runtime values. Cross-file resolution turns these
    /// into class-level abstentions.
    pub(super) value_escapes: Vec<String>,
}

pub(super) fn collect(
    program: &oxc_ast::ast::Program<'_>,
    source: &str,
    line_index: &LineIndex,
    summary: &FileSummary,
) -> ClassCollection {
    let classes = harvest_classes(program, source, line_index);
    let mut bindings = Vec::new();
    let mut member_accesses = Vec::new();
    let mut opaque_classes = BTreeSet::new();
    {
        let mut harvest = BindingHarvest {
            bindings: &mut bindings,
            member_accesses: &mut member_accesses,
            opaque_classes: &mut opaque_classes,
            source,
            line_index,
        };
        harvest.visit_program(program);
    }

    let class_names: BTreeSet<String> = classes.iter().map(|class| class.name.clone()).collect();
    let binding_names: BTreeSet<String> = bindings
        .iter()
        .map(|binding| binding.binding.clone())
        .collect();
    let mut usage = Usage {
        class_names: &class_names,
        binding_names: &binding_names,
        uses: BTreeMap::new(),
        safe: BTreeMap::new(),
        opaque_classes,
        escaped_bindings: BTreeSet::new(),
        new_safe: false,
    };
    usage.visit_program(program);

    let exported_bindings = exported_binding_names(summary);
    let mut classes = classes;
    for class in &mut classes {
        if usage.opaque_classes.contains(&class.name) {
            class.opaque = true;
        }
    }
    for binding in &mut bindings {
        if exported_bindings.contains(&binding.binding)
            || usage.escaped_bindings.contains(&binding.binding)
            || usage.uses.get(&binding.binding).copied().unwrap_or(0)
                > usage.safe.get(&binding.binding).copied().unwrap_or(0)
        {
            binding.escaped = true;
        }
    }
    // An escaped instance can read any member of its class.
    let escaped_classes: BTreeSet<&str> = bindings
        .iter()
        .filter(|binding| binding.escaped)
        .map(|binding| binding.class.as_str())
        .collect();
    for class in &mut classes {
        if escaped_classes.contains(class.name.as_str()) {
            class.opaque = true;
        }
    }
    // A class value used as a whole value (not `new`, `extends`, a static
    // access, or a type) can expose any member to the receiver.
    for class in &mut classes {
        if usage.uses.get(&class.name).copied().unwrap_or(0)
            > usage.safe.get(&class.name).copied().unwrap_or(0)
        {
            class.opaque = true;
        }
    }

    let value_escapes: Vec<String> = usage
        .uses
        .iter()
        .filter(|(name, uses)| **uses > usage.safe.get(*name).copied().unwrap_or(0))
        .map(|(name, _)| name.clone())
        .collect();

    ClassCollection {
        classes,
        bindings,
        member_accesses,
        value_escapes,
    }
}

fn exported_binding_names(summary: &FileSummary) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for declaration in &summary.declarations {
        if declaration.exported {
            names.extend(declaration.names.iter().cloned());
        }
    }
    for local_export in &summary.local_exports {
        names.insert(local_export.local.clone());
    }
    names
}

fn harvest_classes(
    program: &oxc_ast::ast::Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> Vec<ClassRecord> {
    let mut classes = Vec::new();
    for statement in &program.body {
        match statement {
            Statement::ClassDeclaration(class) => {
                if let Some(record) = record_class(class, None, false, source, line_index) {
                    classes.push(record);
                }
            }
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::ClassDeclaration(class)) => {
                    if let Some(record) = record_class(class, None, false, source, line_index) {
                        classes.push(record);
                    }
                }
                Some(Declaration::VariableDeclaration(variable)) => {
                    harvest_variable_classes(variable, &mut classes, source, line_index);
                }
                _ => {}
            },
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    if let Some(record) = record_class(class, None, true, source, line_index) {
                        classes.push(record);
                    }
                }
            }
            Statement::VariableDeclaration(variable) => {
                harvest_variable_classes(variable, &mut classes, source, line_index);
            }
            _ => {}
        }
    }
    classes
}

fn harvest_variable_classes(
    variable: &VariableDeclaration<'_>,
    classes: &mut Vec<ClassRecord>,
    source: &str,
    line_index: &LineIndex,
) {
    for declarator in &variable.declarations {
        let Some(init) = &declarator.init else {
            continue;
        };
        let Expression::ClassExpression(class) = unwrap_expression(init) else {
            continue;
        };
        let binding = match &declarator.id.kind {
            BindingPatternKind::BindingIdentifier(identifier) => identifier.name.to_string(),
            _ => continue,
        };
        if let Some(record) = record_class(class, Some(binding), false, source, line_index) {
            classes.push(record);
        }
    }
}

fn record_class(
    class: &Class<'_>,
    binding_name: Option<String>,
    default: bool,
    source: &str,
    line_index: &LineIndex,
) -> Option<ClassRecord> {
    let internal_name = class.id.as_ref().map(|id| id.name.to_string());
    let name = binding_name.clone().or(internal_name.clone())?;
    let mut class_names = vec![name.clone()];
    if let Some(internal) = internal_name {
        if internal != name {
            class_names.push(internal);
        }
    }

    let (extends, extends_member, extends_opaque) = super_class(class.super_class.as_ref());
    let mut opaque = extends_opaque || !class.decorators.is_empty() || !class.implements.is_empty();
    let mut members = Vec::new();
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) => {
                if method.kind == MethodDefinitionKind::Constructor {
                    harvest_parameter_properties(method, &mut members, source, line_index);
                    continue;
                }
                if method.r#type == MethodDefinitionType::TSAbstractMethodDefinition
                    || method.value.body.is_none()
                {
                    continue;
                }
                let Some(member_name) = member_name(&method.key, method.computed) else {
                    opaque = true;
                    continue;
                };
                members.push(member(
                    member_name,
                    span_of(line_index, source, method.key.span()),
                    match method.kind {
                        MethodDefinitionKind::Get => ClassMemberKind::Getter,
                        MethodDefinitionKind::Set => ClassMemberKind::Setter,
                        _ => ClassMemberKind::Method,
                    },
                    MemberFlags {
                        is_static: method.r#static,
                        accessibility: method.accessibility,
                        is_abstract: false,
                        is_declare: false,
                        is_override: method.r#override,
                        decorated: !method.decorators.is_empty(),
                    },
                ));
            }
            ClassElement::PropertyDefinition(property) => {
                let Some(member_name) = member_name(&property.key, property.computed) else {
                    opaque = true;
                    continue;
                };
                members.push(member(
                    member_name,
                    span_of(line_index, source, property.key.span()),
                    ClassMemberKind::Property,
                    MemberFlags {
                        is_static: property.r#static,
                        accessibility: property.accessibility,
                        is_abstract: property.r#type
                            == PropertyDefinitionType::TSAbstractPropertyDefinition,
                        is_declare: property.declare,
                        is_override: property.r#override,
                        decorated: !property.decorators.is_empty(),
                    },
                ));
            }
            ClassElement::AccessorProperty(property) => {
                let Some(member_name) = member_name(&property.key, property.computed) else {
                    opaque = true;
                    continue;
                };
                members.push(member(
                    member_name,
                    span_of(line_index, source, property.key.span()),
                    ClassMemberKind::Property,
                    MemberFlags {
                        is_static: property.r#static,
                        accessibility: property.accessibility,
                        is_abstract: property.r#type
                            == oxc_ast::ast::AccessorPropertyType::TSAbstractAccessorProperty,
                        is_declare: false,
                        is_override: property.r#override,
                        decorated: !property.decorators.is_empty(),
                    },
                ));
            }
            ClassElement::TSIndexSignature(_) => opaque = true,
            ClassElement::StaticBlock(_) => {}
        }
    }

    let mut reads = SelfReads {
        instance: BTreeSet::new(),
        static_: BTreeSet::new(),
        super_: BTreeSet::new(),
        class_static: BTreeSet::new(),
        opaque: false,
        class_names,
        static_context: false,
    };
    reads.visit_class(class);

    Some(ClassRecord {
        name,
        span: span_of(line_index, source, class.span),
        default,
        ambient: class.declare,
        abstract_: class.r#abstract,
        decorated: !class.decorators.is_empty(),
        opaque: opaque || reads.opaque,
        extends,
        extends_member,
        implements: Vec::new(),
        members,
        self_instance_reads: reads.instance.into_iter().collect(),
        self_static_reads: reads.static_.into_iter().collect(),
        super_reads: reads.super_.into_iter().collect(),
        class_static_reads: reads.class_static.into_iter().collect(),
    })
}

fn harvest_parameter_properties(
    method: &MethodDefinition<'_>,
    members: &mut Vec<ClassMemberRecord>,
    source: &str,
    line_index: &LineIndex,
) {
    for parameter in &method.value.params.items {
        let Some(accessibility) = parameter.accessibility else {
            continue;
        };
        let BindingPatternKind::BindingIdentifier(identifier) = &parameter.pattern.kind else {
            continue;
        };
        members.push(member(
            identifier.name.to_string(),
            span_of(line_index, source, parameter.span),
            ClassMemberKind::Property,
            MemberFlags {
                is_static: false,
                accessibility: Some(accessibility),
                is_abstract: false,
                is_declare: false,
                is_override: parameter.r#override,
                decorated: !parameter.decorators.is_empty(),
            },
        ));
    }
}

fn member(
    name: String,
    span: Span,
    kind: ClassMemberKind,
    flags: MemberFlags,
) -> ClassMemberRecord {
    let is_private =
        matches!(flags.accessibility, Some(TSAccessibility::Private)) || name.starts_with('#');
    ClassMemberRecord {
        name,
        span,
        kind,
        is_static: flags.is_static,
        is_private,
        is_protected: matches!(flags.accessibility, Some(TSAccessibility::Protected)),
        is_abstract: flags.is_abstract,
        is_declare: flags.is_declare,
        is_override: flags.is_override,
        decorated: flags.decorated,
    }
}

struct MemberFlags {
    is_static: bool,
    accessibility: Option<TSAccessibility>,
    is_abstract: bool,
    is_declare: bool,
    is_override: bool,
    decorated: bool,
}

fn member_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::PrivateIdentifier(identifier) => Some(format!("#{}", identifier.name)),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::NumericLiteral(literal) => Some(
            literal
                .raw
                .as_ref()
                .map_or_else(|| literal.value.to_string(), ToString::to_string),
        ),
        _ if computed => None,
        _ => None,
    }
}

fn super_class(expression: Option<&Expression<'_>>) -> (Option<String>, bool, bool) {
    let Some(expression) = expression else {
        return (None, false, false);
    };
    let expression = unwrap_expression(expression);
    match expression {
        Expression::Identifier(identifier) => (Some(identifier.name.to_string()), false, false),
        Expression::StaticMemberExpression(member) => {
            let Some(root) = dotted_name(&member.object) else {
                return (None, true, true);
            };
            (
                Some(format!("{root}.{}", member.property.name)),
                true,
                false,
            )
        }
        _ => (None, false, true),
    }
}

fn dotted_name(expression: &Expression<'_>) -> Option<String> {
    match unwrap_expression(expression) {
        Expression::Identifier(identifier) => Some(identifier.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            let root = dotted_name(&member.object)?;
            Some(format!("{root}.{}", member.property.name))
        }
        _ => None,
    }
}

fn unwrap_expression<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::ParenthesizedExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSAsExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSTypeAssertion(inner) => unwrap_expression(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSInstantiationExpression(inner) => unwrap_expression(&inner.expression),
        _ => expression,
    }
}

/// `this.x`, `super.x`, and `ClassName.x` reads inside one class body.
struct SelfReads {
    instance: BTreeSet<String>,
    static_: BTreeSet<String>,
    super_: BTreeSet<String>,
    class_static: BTreeSet<String>,
    opaque: bool,
    class_names: Vec<String>,
    static_context: bool,
}

impl<'a> Visit<'a> for SelfReads {
    fn visit_method_definition(&mut self, method: &MethodDefinition<'a>) {
        let previous = self.static_context;
        self.static_context = method.r#static;
        for decorator in &method.decorators {
            self.visit_decorator(decorator);
        }
        self.visit_function(&method.value, oxc_syntax::scope::ScopeFlags::Function);
        self.static_context = previous;
    }

    fn visit_property_definition(&mut self, property: &PropertyDefinition<'a>) {
        let previous = self.static_context;
        self.static_context = property.r#static;
        for decorator in &property.decorators {
            self.visit_decorator(decorator);
        }
        if let Some(value) = &property.value {
            self.visit_expression(value);
        }
        self.static_context = previous;
    }

    fn visit_accessor_property(&mut self, property: &AccessorProperty<'a>) {
        let previous = self.static_context;
        self.static_context = property.r#static;
        for decorator in &property.decorators {
            self.visit_decorator(decorator);
        }
        if let Some(value) = &property.value {
            self.visit_expression(value);
        }
        self.static_context = previous;
    }

    fn visit_static_block(&mut self, block: &oxc_ast::ast::StaticBlock<'a>) {
        let previous = self.static_context;
        self.static_context = true;
        for statement in &block.body {
            self.visit_statement(statement);
        }
        self.static_context = previous;
    }

    fn visit_static_member_expression(
        &mut self,
        member: &oxc_ast::ast::StaticMemberExpression<'a>,
    ) {
        match &member.object {
            Expression::ThisExpression(_) => {
                self.record(member.property.name.as_str());
                return;
            }
            Expression::Super(_) => {
                self.super_.insert(member.property.name.to_string());
                return;
            }
            Expression::Identifier(identifier)
                if self
                    .class_names
                    .iter()
                    .any(|name| name == identifier.name.as_str()) =>
            {
                self.class_static.insert(member.property.name.to_string());
                return;
            }
            _ => {}
        }
        walk_static_member_expression(self, member);
    }

    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        let target = match &member.object {
            Expression::ThisExpression(_) => Some(false),
            Expression::Super(_) => Some(true),
            Expression::Identifier(identifier)
                if self
                    .class_names
                    .iter()
                    .any(|name| name == identifier.name.as_str()) =>
            {
                Some(false)
            }
            _ => None,
        };
        if let Some(super_read) = target {
            if let Some(name) = literal_member_name(&member.expression) {
                if super_read {
                    self.super_.insert(name);
                } else {
                    self.record(&name);
                }
            } else {
                self.opaque = true;
            }
            self.visit_expression(&member.expression);
            return;
        }
        walk_computed_member_expression(self, member);
    }
}

impl SelfReads {
    fn record(&mut self, name: &str) {
        if self.static_context {
            self.static_.insert(name.to_string());
        } else {
            self.instance.insert(name.to_string());
        }
    }
}

fn literal_member_name(expression: &Expression<'_>) -> Option<String> {
    match unwrap_expression(expression) {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::NumericLiteral(literal) => Some(
            literal
                .raw
                .as_ref()
                .map_or_else(|| literal.value.to_string(), ToString::to_string),
        ),
        _ => None,
    }
}

/// Harvest local instance bindings and destructured member reads.
struct BindingHarvest<'a> {
    bindings: &'a mut Vec<ClassBindingRecord>,
    member_accesses: &'a mut Vec<MemberAccess>,
    opaque_classes: &'a mut BTreeSet<String>,
    source: &'a str,
    line_index: &'a LineIndex,
}

impl<'a> Visit<'a> for BindingHarvest<'a> {
    fn visit_variable_declarator(&mut self, declarator: &oxc_ast::ast::VariableDeclarator<'a>) {
        self.harvest_pattern(&declarator.id, declarator.init.as_ref());
        walk_variable_declarator(self, declarator);
    }

    fn visit_formal_parameter(&mut self, parameter: &oxc_ast::ast::FormalParameter<'a>) {
        self.harvest_pattern(&parameter.pattern, None);
        // The parameter's default value may bind another instance; the generic
        // visitor covers it.
        if let BindingPatternKind::AssignmentPattern(assignment) = &parameter.pattern.kind {
            self.harvest_pattern(&assignment.left, Some(&assignment.right));
        }
        oxc_ast_visit::walk::walk_formal_parameter(self, parameter);
    }
}

impl BindingHarvest<'_> {
    fn harvest_pattern(&mut self, pattern: &BindingPattern<'_>, init: Option<&Expression<'_>>) {
        let mut pattern = pattern;
        let mut init = init;
        if let BindingPatternKind::AssignmentPattern(assignment) = &pattern.kind {
            pattern = &assignment.left;
            if init.is_none() {
                init = Some(&assignment.right);
            }
        }
        let type_class = type_name(pattern.type_annotation.as_deref());
        match &pattern.kind {
            BindingPatternKind::BindingIdentifier(identifier) => {
                let new_class = init.and_then(new_class_name);
                let via_new = new_class.is_some();
                let class = type_class.or(new_class);
                if let Some(class) = class {
                    self.bindings.push(ClassBindingRecord {
                        binding: identifier.name.to_string(),
                        class,
                        span: span_of(self.line_index, self.source, identifier.span),
                        escaped: false,
                        via_new,
                    });
                }
            }
            BindingPatternKind::ObjectPattern(object) => {
                let object_class = type_class
                    .clone()
                    .or_else(|| init.and_then(new_class_name))
                    .or_else(|| init.and_then(identifier_name));
                if let Some(class) = object_class {
                    for property in &object.properties {
                        if let Some(name) = member_name(&property.key, false) {
                            self.member_accesses.push(MemberAccess {
                                object: class.clone(),
                                property: Some(name),
                                computed: false,
                                instance: type_class.is_some()
                                    || init.and_then(new_class_name).is_some(),
                            });
                        } else {
                            self.opaque_classes.insert(class.clone());
                        }
                        self.harvest_pattern(&property.value, None);
                    }
                    if let Some(rest) = &object.rest {
                        self.opaque_classes.insert(class.clone());
                        self.harvest_pattern(&rest.argument, None);
                    }
                } else {
                    for property in &object.properties {
                        self.harvest_pattern(&property.value, None);
                    }
                    if let Some(rest) = &object.rest {
                        self.harvest_pattern(&rest.argument, None);
                    }
                }
            }
            BindingPatternKind::ArrayPattern(array) => {
                if let Some(class) = type_class.clone().or_else(|| init.and_then(new_class_name)) {
                    self.opaque_classes.insert(class);
                }
                for element in array.elements.iter().flatten() {
                    self.harvest_pattern(element, None);
                }
                if let Some(rest) = &array.rest {
                    self.harvest_pattern(&rest.argument, None);
                }
            }
            BindingPatternKind::AssignmentPattern(_) => {}
        }
    }
}

fn type_name(annotation: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>) -> Option<String> {
    let annotation = annotation?;
    match &annotation.type_annotation {
        TSType::TSTypeReference(reference) => match &reference.type_name {
            TSTypeName::IdentifierReference(identifier) => Some(identifier.name.to_string()),
            TSTypeName::QualifiedName(name) => {
                let root = match &name.left {
                    TSTypeName::IdentifierReference(identifier) => identifier.name.to_string(),
                    TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => return None,
                };
                Some(format!("{root}.{}", name.right.name))
            }
            _ => None,
        },
        _ => None,
    }
}

fn new_class_name(expression: &Expression<'_>) -> Option<String> {
    let Expression::NewExpression(new) = unwrap_expression(expression) else {
        return None;
    };
    identifier_name(&new.callee)
}

fn identifier_name(expression: &Expression<'_>) -> Option<String> {
    match unwrap_expression(expression) {
        Expression::Identifier(identifier) => Some(identifier.name.to_string()),
        _ => None,
    }
}

/// Conservative whole-value use detection.
///
/// A class or instance value is safe only in the contexts that cannot hand it
/// to code outside the analysis: a static member access, `new`, `extends`, a
/// local variable initializer, a destructured member read, or an expression
/// statement. Every other runtime use marks the class opaque or the binding
/// escaped.
struct Usage<'a> {
    class_names: &'a BTreeSet<String>,
    binding_names: &'a BTreeSet<String>,
    uses: BTreeMap<String, usize>,
    safe: BTreeMap<String, usize>,
    opaque_classes: BTreeSet<String>,
    escaped_bindings: BTreeSet<String>,
    new_safe: bool,
}

impl Usage<'_> {
    fn is_tracked(&self, name: &str) -> bool {
        self.class_names.contains(name) || self.binding_names.contains(name)
    }

    fn mark_safe(&mut self, name: &str) {
        *self.safe.entry(name.to_string()).or_default() += 1;
    }
}

impl<'a> Visit<'a> for Usage<'_> {
    fn visit_identifier_reference(&mut self, identifier: &oxc_ast::ast::IdentifierReference<'a>) {
        *self.uses.entry(identifier.name.to_string()).or_default() += 1;
    }

    fn visit_ts_type(&mut self, _ty: &TSType<'a>) {
        // Type positions never expose a runtime member and never escape.
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        if let Some(super_class) = &class.super_class {
            if let Expression::Identifier(identifier) = unwrap_expression(super_class) {
                // Extending a class never exposes its instance members.
                self.mark_safe(identifier.name.as_str());
            }
            self.visit_expression(super_class);
        }
        for decorator in &class.decorators {
            self.visit_decorator(decorator);
        }
        for element in &class.body.body {
            self.visit_class_element(element);
        }
    }

    fn visit_variable_declarator(&mut self, declarator: &oxc_ast::ast::VariableDeclarator<'a>) {
        // `const { x } = instance` reads one member of the instance.
        if let (BindingPatternKind::ObjectPattern(_), Some(Expression::Identifier(identifier))) =
            (&declarator.id.kind, &declarator.init)
        {
            if self.binding_names.contains(identifier.name.as_str()) {
                self.mark_safe(identifier.name.as_str());
            }
        }
        let previous = self.new_safe;
        self.new_safe = true;
        if let Some(init) = &declarator.init {
            self.visit_expression(init);
        }
        self.new_safe = previous;
        self.visit_binding_pattern(&declarator.id);
    }

    fn visit_expression_statement(&mut self, statement: &oxc_ast::ast::ExpressionStatement<'a>) {
        let previous = self.new_safe;
        self.new_safe = true;
        self.visit_expression(&statement.expression);
        self.new_safe = previous;
    }

    fn visit_static_member_expression(
        &mut self,
        member: &oxc_ast::ast::StaticMemberExpression<'a>,
    ) {
        if let Expression::Identifier(identifier) = &member.object {
            // A member access never passes the object itself to the receiver.
            self.mark_safe(identifier.name.as_str());
        }
        let previous = self.new_safe;
        self.new_safe = true;
        self.visit_expression(&member.object);
        self.new_safe = previous;
    }

    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        if let Expression::Identifier(identifier) = &member.object {
            if self.is_tracked(identifier.name.as_str()) {
                if literal_member_name(&member.expression).is_some() {
                    self.mark_safe(identifier.name.as_str());
                } else if self.class_names.contains(identifier.name.as_str()) {
                    self.opaque_classes.insert(identifier.name.to_string());
                } else {
                    self.escaped_bindings.insert(identifier.name.to_string());
                }
            }
        }
        let previous = self.new_safe;
        self.new_safe = true;
        self.visit_expression(&member.object);
        self.new_safe = previous;
        self.visit_expression(&member.expression);
    }

    fn visit_new_expression(&mut self, new: &oxc_ast::ast::NewExpression<'a>) {
        if let Expression::Identifier(identifier) = &new.callee {
            if self.new_safe {
                // A constructed value kept in a safe position cannot expose
                // members; only the class value is read.
                self.mark_safe(identifier.name.as_str());
            } else if self.class_names.contains(identifier.name.as_str()) {
                // A returned, passed, or exported instance can be inspected by
                // the receiver.
                self.opaque_classes.insert(identifier.name.to_string());
            }
        }
        self.visit_expression(&new.callee);
        let previous = self.new_safe;
        self.new_safe = false;
        for argument in &new.arguments {
            self.visit_argument(argument);
        }
        self.new_safe = previous;
    }

    fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        let previous = self.new_safe;
        self.new_safe = true;
        self.visit_expression(&call.callee);
        self.new_safe = false;
        for argument in &call.arguments {
            self.visit_argument(argument);
        }
        self.new_safe = previous;
    }

    fn visit_return_statement(&mut self, statement: &oxc_ast::ast::ReturnStatement<'a>) {
        let previous = self.new_safe;
        self.new_safe = false;
        if let Some(argument) = &statement.argument {
            self.visit_expression(argument);
        }
        self.new_safe = previous;
    }

    fn visit_assignment_expression(&mut self, assignment: &oxc_ast::ast::AssignmentExpression<'a>) {
        let previous = self.new_safe;
        self.new_safe = false;
        self.visit_expression(&assignment.right);
        self.new_safe = previous;
        self.visit_assignment_target(&assignment.left);
    }

    fn visit_conditional_expression(
        &mut self,
        expression: &oxc_ast::ast::ConditionalExpression<'a>,
    ) {
        let previous = self.new_safe;
        self.new_safe = false;
        self.visit_expression(&expression.test);
        self.visit_expression(&expression.consequent);
        self.visit_expression(&expression.alternate);
        self.new_safe = previous;
    }

    fn visit_logical_expression(&mut self, expression: &oxc_ast::ast::LogicalExpression<'a>) {
        let previous = self.new_safe;
        self.new_safe = false;
        self.visit_expression(&expression.left);
        self.visit_expression(&expression.right);
        self.new_safe = previous;
    }

    fn visit_binary_expression(&mut self, expression: &oxc_ast::ast::BinaryExpression<'a>) {
        let previous = self.new_safe;
        self.new_safe = false;
        self.visit_expression(&expression.left);
        self.visit_expression(&expression.right);
        self.new_safe = previous;
    }

    fn visit_parenthesized_expression(
        &mut self,
        expression: &oxc_ast::ast::ParenthesizedExpression<'a>,
    ) {
        self.visit_expression(&expression.expression);
    }

    fn visit_ts_as_expression(&mut self, expression: &oxc_ast::ast::TSAsExpression<'a>) {
        self.visit_expression(&expression.expression);
    }

    fn visit_ts_satisfies_expression(
        &mut self,
        expression: &oxc_ast::ast::TSSatisfiesExpression<'a>,
    ) {
        self.visit_expression(&expression.expression);
    }

    fn visit_ts_non_null_expression(&mut self, expression: &oxc_ast::ast::TSNonNullExpression<'a>) {
        self.visit_expression(&expression.expression);
    }

    fn visit_ts_instantiation_expression(
        &mut self,
        expression: &oxc_ast::ast::TSInstantiationExpression<'a>,
    ) {
        self.visit_expression(&expression.expression);
    }
}
