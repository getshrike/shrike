//! Statically harvest props of local React components.
//!
//! The rule is deliberately narrow. A component qualifies only when it is a
//! top-level function declaration or a `const` binding whose initializer is an
//! arrow or function expression (optionally wrapped in a local `memo` or
//! `forwardRef` call), its name starts uppercase, and its body returns JSX.
//!
//! Props are harvested from two shapes:
//!
//! - an inline object destructure with no rest element, no computed key, and no
//!   nested pattern; or
//! - a bare identifier parameter annotated with a bare, same-file `interface`
//!   or `type X = { ... }` object type, where each declared property is a prop
//!   and `props.x` / `const { x } = props` reads credit it. Imported, generic,
//!   inherited, union, intersection, and qualified types abstain, as does any
//!   whole-object use of the props binding (`{...props}`, `fn(props)`, ...).
//!
//! A component whose value escapes the module is skipped: a direct export,
//! `export default Card`, `memo(Card)`, a registry object, or a `displayName`
//! assignment. Rendering `<Card />` is not an escape: the props are consumed
//! at that site, and an unread prop is still a local defect. This is stricter
//! than fallow 3.45.0, which only checks whether the component binding itself
//! is exported and therefore reports props on a component wrapped by
//! `export default memo(Card)`.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPatternKind, CallExpression, Declaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression,
    FormalParameters, Function, FunctionBody, IdentifierReference, JSXAttributeItem,
    JSXAttributeName, JSXElementName, JSXMemberExpression, JSXMemberExpressionObject,
    JSXOpeningElement, ObjectPattern, Program, PropertyKey, Statement, StaticMemberExpression,
    TSInterfaceDeclaration, TSSignature, TSType, TSTypeAliasDeclaration, TSTypeAnnotation,
    TSTypeName, VariableDeclaration, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use shrike_core::{LineIndex, Span};

use crate::summary::{
    ComponentContractProp, ComponentContractRecord, ComponentPropRecord, FileSummary,
    JsxNamespaceUseRecord, JsxUseRecord,
};

pub(super) fn collect(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
    summary: &FileSummary,
) -> Vec<ComponentPropRecord> {
    let candidates = component_candidates(program);
    if candidates.is_empty() {
        // Most files have no local component at all. Skip the whole-program
        // value-reference walk on that fast path.
        return Vec::new();
    }
    let object_types = same_file_props_types(program, source, line_index);
    let escaped = escaped_components(program, summary);
    let mut records = Vec::new();
    for candidate in candidates {
        if escaped.contains(candidate.name) {
            continue;
        }
        let Some(props) = harvest_props(&candidate, &object_types, source, line_index) else {
            continue;
        };
        let used = used_locals(summary, candidate.name);
        push_records(candidate.name, props, &used, &mut records);
    }
    records
}

/// Harvest the optional props of every top-level React component, including
/// exported ones, for the absent-prop rule. This is separate from `collect`:
/// `unused_component_prop` only checks local components, while the absent-prop
/// rule follows exported components to their statically known render sites.
pub(super) fn collect_contracts(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
    summary: &FileSummary,
) -> Vec<ComponentContractRecord> {
    let candidates = component_candidates(program);
    if candidates.is_empty() {
        return Vec::new();
    }
    let object_types = same_file_props_types(program, source, line_index);
    let mut records = Vec::new();
    for candidate in candidates {
        let Some(props) = harvest_props(&candidate, &object_types, source, line_index) else {
            continue;
        };
        let used = used_locals(summary, candidate.name);
        let escaped = component_value_escapes(program, candidate.name);
        let props = props
            .into_iter()
            .map(|prop| ComponentContractProp {
                name: prop.prop,
                span: prop.span,
                optional: prop.optional,
                used: prop
                    .used
                    .unwrap_or_else(|| used.contains(prop.local.as_str())),
                has_default: prop.has_default,
            })
            .collect();
        records.push(ComponentContractRecord {
            component: candidate.name.to_string(),
            escaped,
            props,
        });
    }
    records
}

/// Collect JSX opening elements with simple identifier tags and JSX member
/// expression roots. The graph resolves the names against component contracts;
/// lowercase DOM tags are harmless because no contract matches them.
pub(super) fn collect_jsx_uses(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> (Vec<JsxUseRecord>, Vec<JsxNamespaceUseRecord>, Vec<String>) {
    let mut collector = JsxUseCollector {
        source,
        line_index,
        uses: Vec::new(),
        namespace_uses: Vec::new(),
        value_uses: BTreeSet::new(),
        scopes: vec![BTreeSet::new()],
    };
    collector.visit_program(program);
    (
        collector.uses,
        collector.namespace_uses,
        collector.value_uses.into_iter().collect(),
    )
}

struct JsxUseCollector<'a> {
    source: &'a str,
    line_index: &'a LineIndex,
    uses: Vec<JsxUseRecord>,
    namespace_uses: Vec<JsxNamespaceUseRecord>,
    value_uses: BTreeSet<String>,
    /// Lexical bindings, innermost last. Index 0 is module scope; a name bound
    /// in any deeper scope shadows a same-named import or top-level component.
    scopes: Vec<BTreeSet<String>>,
}

impl JsxUseCollector<'_> {
    fn is_shadowed(&self, name: &str) -> bool {
        self.scopes.iter().skip(1).any(|scope| scope.contains(name))
    }

    fn push_scope(&mut self) {
        self.scopes.push(BTreeSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind_pattern(&mut self, pattern: &oxc_ast::ast::BindingPattern<'_>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.extend(super::refs::binding_names(pattern));
        }
    }

    fn bind_parameters(&mut self, params: &FormalParameters<'_>) {
        for parameter in &params.items {
            self.bind_pattern(&parameter.pattern);
        }
        if let Some(rest) = &params.rest {
            self.bind_pattern(&rest.argument);
        }
    }
}

impl<'a> Visit<'a> for JsxUseCollector<'_> {
    fn visit_identifier_reference(&mut self, reference: &IdentifierReference<'a>) {
        if !self.is_shadowed(reference.name.as_str()) {
            self.value_uses.insert(reference.name.to_string());
        }
    }

    fn visit_jsx_element_name(&mut self, _name: &JSXElementName<'a>) {
        // The tag itself is a render site, not a value escape. Attributes and
        // children are still visited through the opening element walk.
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        if let Some(id) = &function.id {
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(id.name.to_string());
            }
        }
        self.push_scope();
        if let Some(id) = &function.id {
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(id.name.to_string());
            }
        }
        self.bind_parameters(&function.params);
        walk::walk_function(self, function, flags);
        self.pop_scope();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        self.push_scope();
        self.bind_parameters(&arrow.params);
        walk::walk_arrow_function_expression(self, arrow);
        self.pop_scope();
    }

    fn visit_block_statement(&mut self, block: &oxc_ast::ast::BlockStatement<'a>) {
        self.push_scope();
        walk::walk_block_statement(self, block);
        self.pop_scope();
    }

    fn visit_variable_declaration(&mut self, declaration: &VariableDeclaration<'a>) {
        for declarator in &declaration.declarations {
            self.bind_pattern(&declarator.id);
        }
        walk::walk_variable_declaration(self, declaration);
    }

    fn visit_class(&mut self, class: &oxc_ast::ast::Class<'a>) {
        if let Some(id) = &class.id {
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(id.name.to_string());
            }
        }
        walk::walk_class(self, class);
    }

    fn visit_catch_clause(&mut self, clause: &oxc_ast::ast::CatchClause<'a>) {
        if let Some(param) = &clause.param {
            self.bind_pattern(&param.pattern);
        }
        walk::walk_catch_clause(self, clause);
    }

    fn visit_export_named_declaration(&mut self, export: &ExportNamedDeclaration<'a>) {
        // `export { Card }` is a direct export, not a value use. The default
        // walk would visit the specifier's local name as a reference.
        if let Some(declaration) = &export.declaration {
            walk::walk_declaration(self, declaration);
        }
    }

    fn visit_export_default_declaration(&mut self, declaration: &ExportDefaultDeclaration<'a>) {
        // `export default Card` is a direct export. A wrapper such as
        // `export default memo(Card)` still exposes the component to an unknown
        // consumer, so its arguments are value uses.
        if declaration
            .declaration
            .as_expression()
            .is_some_and(is_identifier_expression)
        {
            return;
        }
        walk::walk_export_default_declaration(self, declaration);
    }

    fn visit_jsx_opening_element(&mut self, element: &JSXOpeningElement<'a>) {
        match &element.name {
            JSXElementName::Identifier(identifier) => {
                if !self.is_shadowed(identifier.name.as_str()) {
                    self.push_use(identifier.name.to_string(), element);
                }
            }
            JSXElementName::IdentifierReference(identifier) => {
                if !self.is_shadowed(identifier.name.as_str()) {
                    self.push_use(identifier.name.to_string(), element);
                }
            }
            JSXElementName::MemberExpression(member) => {
                if let Some(root) = jsx_member_root(member)
                    && !self.is_shadowed(root)
                {
                    self.namespace_uses.push(JsxNamespaceUseRecord {
                        root: root.to_string(),
                        span: self.line_index.span(
                            self.source,
                            element.span.start,
                            element.span.end,
                        ),
                    });
                }
            }
            JSXElementName::NamespacedName(_) | JSXElementName::ThisExpression(_) => {}
        }
        walk::walk_jsx_opening_element(self, element);
    }
}

impl JsxUseCollector<'_> {
    fn push_use(&mut self, component: String, element: &JSXOpeningElement<'_>) {
        let mut supplied = Vec::new();
        let mut opaque = false;
        for attribute in &element.attributes {
            match attribute {
                JSXAttributeItem::Attribute(attribute) => match &attribute.name {
                    JSXAttributeName::Identifier(identifier) => {
                        supplied.push(identifier.name.to_string());
                    }
                    JSXAttributeName::NamespacedName(_) => opaque = true,
                },
                JSXAttributeItem::SpreadAttribute(_) => opaque = true,
            }
        }
        self.uses.push(JsxUseRecord {
            component,
            span: self
                .line_index
                .span(self.source, element.span.start, element.span.end),
            supplied,
            opaque,
        });
    }
}

fn jsx_member_root<'a>(member: &'a JSXMemberExpression<'a>) -> Option<&'a str> {
    match &member.object {
        JSXMemberExpressionObject::IdentifierReference(identifier) => {
            Some(identifier.name.as_str())
        }
        JSXMemberExpressionObject::MemberExpression(inner) => jsx_member_root(inner),
        JSXMemberExpressionObject::ThisExpression(_) => None,
    }
}

/// Whether a value reference other than a direct export or a JSX tag reaches
/// the component. Such a reference can hand the component to an unknown
/// consumer, so the set of render sites is open and the absent-prop rule
/// abstains.
fn component_value_escapes(program: &Program<'_>, component: &str) -> bool {
    let mut collector = ValueEscapeCollector {
        component,
        escaped: false,
    };
    collector.visit_program(program);
    collector.escaped
}

struct ValueEscapeCollector<'a> {
    component: &'a str,
    escaped: bool,
}

impl<'a> Visit<'a> for ValueEscapeCollector<'_> {
    fn visit_identifier_reference(&mut self, reference: &IdentifierReference<'a>) {
        if reference.name.as_str() == self.component {
            self.escaped = true;
        }
    }

    fn visit_jsx_element_name(&mut self, name: &JSXElementName<'a>) {
        if let JSXElementName::MemberExpression(member) = name
            && jsx_member_root(member) == Some(self.component)
        {
            self.escaped = true;
        }
    }

    fn visit_export_default_declaration(&mut self, declaration: &ExportDefaultDeclaration<'a>) {
        if is_direct_component_export(&declaration.declaration, self.component) {
            return;
        }
        walk::walk_export_default_declaration(self, declaration);
    }

    fn visit_export_named_declaration(&mut self, export: &ExportNamedDeclaration<'a>) {
        // `export { Card }` is a direct export, not a value use. The default
        // walk visits the specifier's local name as an identifier reference,
        // which would falsely mark the component as escaped. An inline
        // declaration (`export const Alias = Card`) still needs its body
        // visited, so walk the declaration only.
        if let Some(declaration) = &export.declaration {
            walk::walk_declaration(self, declaration);
        }
    }
}

fn is_direct_component_export(
    declaration: &ExportDefaultDeclarationKind<'_>,
    component: &str,
) -> bool {
    let Some(expression) = declaration.as_expression() else {
        return false;
    };
    is_component_identifier(expression, component)
}

fn is_component_identifier(expression: &Expression<'_>, component: &str) -> bool {
    match expression {
        Expression::Identifier(identifier) => identifier.name.as_str() == component,
        Expression::ParenthesizedExpression(inner) => {
            is_component_identifier(&inner.expression, component)
        }
        Expression::TSAsExpression(inner) => is_component_identifier(&inner.expression, component),
        Expression::TSSatisfiesExpression(inner) => {
            is_component_identifier(&inner.expression, component)
        }
        Expression::TSNonNullExpression(inner) => {
            is_component_identifier(&inner.expression, component)
        }
        _ => false,
    }
}

fn is_identifier_expression(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Identifier(_) => true,
        Expression::ParenthesizedExpression(inner) => is_identifier_expression(&inner.expression),
        Expression::TSAsExpression(inner) => is_identifier_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => is_identifier_expression(&inner.expression),
        Expression::TSNonNullExpression(inner) => is_identifier_expression(&inner.expression),
        _ => false,
    }
}

struct ComponentCandidate<'a> {
    name: &'a str,
    params: &'a FormalParameters<'a>,
    body: Option<&'a FunctionBody<'a>>,
    /// The second generic argument of a `forwardRef<Ref, Props>` wrapper when
    /// the inner props parameter carries no annotation of its own.
    wrapper_props_type: Option<&'a str>,
    /// `forwardRef` passes a second `ref` parameter, so one extra parameter is
    /// expected for that wrapper only.
    forward_ref: bool,
}

/// Top-level declarations that look like React components: an uppercase
/// function or arrow/function-expression binding whose body returns JSX, plus
/// a binding initialized by a local `memo(...)` / `forwardRef(...)` wrapper.
fn component_candidates<'a>(program: &'a Program<'a>) -> Vec<ComponentCandidate<'a>> {
    let mut candidates = Vec::new();
    for statement in &program.body {
        match statement {
            Statement::FunctionDeclaration(function) => {
                push_function_candidate(function, &mut candidates);
            }
            Statement::VariableDeclaration(variable) => {
                push_variable_candidates(variable, &mut candidates);
            }
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::FunctionDeclaration(function)) => {
                    push_function_candidate(function, &mut candidates);
                }
                Some(Declaration::VariableDeclaration(variable)) => {
                    push_variable_candidates(variable, &mut candidates);
                }
                _ => {}
            },
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::FunctionDeclaration(function) =
                    &export.declaration
                {
                    push_function_candidate(function, &mut candidates);
                }
            }
            _ => {}
        }
    }
    candidates
}

fn push_function_candidate<'a>(
    function: &'a Function<'a>,
    candidates: &mut Vec<ComponentCandidate<'a>>,
) {
    let Some(id) = &function.id else {
        return;
    };
    let name = id.name.as_str();
    if is_component_name(name) && function_body_returns_jsx(function.body.as_deref()) {
        candidates.push(ComponentCandidate {
            name,
            params: function.params.as_ref(),
            body: function.body.as_deref(),
            wrapper_props_type: None,
            forward_ref: false,
        });
    }
}

fn push_variable_candidates<'a>(
    variable: &'a VariableDeclaration<'a>,
    candidates: &mut Vec<ComponentCandidate<'a>>,
) {
    for declarator in &variable.declarations {
        let BindingPatternKind::BindingIdentifier(id) = &declarator.id.kind else {
            continue;
        };
        let name = id.name.as_str();
        if !is_component_name(name) {
            continue;
        }
        let Some(init) = &declarator.init else {
            continue;
        };
        if let Some(candidate) = candidate_from_init(name, init) {
            candidates.push(candidate);
        }
    }
}

fn candidate_from_init<'a>(
    name: &'a str,
    init: &'a Expression<'a>,
) -> Option<ComponentCandidate<'a>> {
    match init {
        Expression::ArrowFunctionExpression(arrow) if arrow_returns_jsx(arrow) => {
            Some(ComponentCandidate {
                name,
                params: arrow.params.as_ref(),
                body: Some(&arrow.body),
                wrapper_props_type: None,
                forward_ref: false,
            })
        }
        Expression::FunctionExpression(function)
            if function_body_returns_jsx(function.body.as_deref()) =>
        {
            Some(ComponentCandidate {
                name,
                params: function.params.as_ref(),
                body: function.body.as_deref(),
                wrapper_props_type: None,
                forward_ref: false,
            })
        }
        Expression::CallExpression(call) => wrapper_candidate(name, call),
        Expression::ParenthesizedExpression(inner) => candidate_from_init(name, &inner.expression),
        Expression::TSAsExpression(inner) => candidate_from_init(name, &inner.expression),
        Expression::TSSatisfiesExpression(inner) => candidate_from_init(name, &inner.expression),
        _ => None,
    }
}

/// A local `memo(...)` / `forwardRef(...)` wrapper whose first argument is an
/// inline arrow or function component. A named component argument (`memo(Card)`)
/// is not a new component: `Card` is already a candidate, and the value escape
/// suppresses it. A `memo` comparator can read props the body does not, so that
/// shape abstains.
fn wrapper_candidate<'a>(
    name: &'a str,
    call: &'a CallExpression<'a>,
) -> Option<ComponentCandidate<'a>> {
    let wrapper = match &call.callee {
        Expression::Identifier(identifier) => identifier.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return None,
    };
    let forward_ref = match wrapper {
        "memo" => false,
        "forwardRef" => true,
        _ => return None,
    };
    if !forward_ref && call.arguments.len() > 1 {
        return None;
    }
    let first = unwrap_parentheses(call.arguments.first()?.as_expression()?);
    let wrapper_props_type = if forward_ref {
        forward_ref_props_type(call)
    } else {
        None
    };
    match first {
        Expression::ArrowFunctionExpression(arrow) if arrow_returns_jsx(arrow) => {
            Some(ComponentCandidate {
                name,
                params: arrow.params.as_ref(),
                body: Some(&arrow.body),
                wrapper_props_type,
                forward_ref,
            })
        }
        Expression::FunctionExpression(function)
            if function_body_returns_jsx(function.body.as_deref()) =>
        {
            Some(ComponentCandidate {
                name,
                params: function.params.as_ref(),
                body: function.body.as_deref(),
                wrapper_props_type,
                forward_ref,
            })
        }
        _ => None,
    }
}

fn unwrap_parentheses<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::ParenthesizedExpression(inner) => unwrap_parentheses(&inner.expression),
        _ => expression,
    }
}

/// The bare single-name second type argument of `forwardRef<Ref, Props>(...)`.
/// A single type argument, a generic, a qualified name, or a union abstains.
fn forward_ref_props_type<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    let params = &call.type_arguments.as_deref()?.params;
    bare_props_type_name(params.get(1)?)
}

struct HarvestedProp {
    prop: String,
    local: String,
    span: Span,
    /// Typed props carry a per-prop usage answer. Destructured props are
    /// resolved against the declaration's collected local references.
    used: Option<bool>,
    destructured: bool,
    /// The prop is optional by a destructuring default or a `?` type member.
    optional: bool,
    /// The prop has a destructuring default.
    has_default: bool,
}

fn push_records(
    component: &str,
    props: Vec<HarvestedProp>,
    used: &BTreeSet<&str>,
    records: &mut Vec<ComponentPropRecord>,
) {
    for prop in props {
        records.push(ComponentPropRecord {
            component: component.to_string(),
            used: prop
                .used
                .unwrap_or_else(|| used.contains(prop.local.as_str())),
            prop: prop.prop,
            local: prop.local,
            span: prop.span,
            destructured: prop.destructured,
        });
    }
}

/// Names whose *value* escapes the module. Rendering a component (`<Card />`)
/// does not escape it: the props are consumed at that site, and an unread prop
/// is still a local defect. Passing the component as a value does escape it: a
/// HOC (`memo(Card)`), a registry (`{ Card }`), or a `displayName` assignment.
/// The collector walks the whole program and skips JSX element names while
/// still visiting attribute values and children, so `<Card render={Card} />`
/// counts the attribute value but not the tag. Over-crediting here can only
/// suppress a finding, never create one.
fn escaped_components(program: &Program<'_>, summary: &FileSummary) -> BTreeSet<String> {
    let mut collector = ValueRefCollector::default();
    collector.visit_program(program);
    let mut escaped = collector.names;
    // A directly exported declaration's own name is public even when no value
    // reference names it.
    for declaration in &summary.declarations {
        if declaration.exported || declaration.default {
            escaped.extend(declaration.names.iter().cloned());
        }
    }
    escaped
}

#[derive(Default)]
struct ValueRefCollector {
    names: BTreeSet<String>,
}

impl<'a> Visit<'a> for ValueRefCollector {
    fn visit_identifier_reference(&mut self, reference: &IdentifierReference<'a>) {
        self.names.insert(reference.name.to_string());
    }

    fn visit_jsx_element_name(&mut self, _name: &JSXElementName<'a>) {
        // A render site consumes the component; it is not a value escape.
        // Attribute values and children are still visited by the default
        // JSX walk, so `<Card render={Card} />` still records the value use.
    }
}

/// The identifiers referenced by the component's declaration. The parser
/// already visited the whole declaration (including parameter defaults and the
/// body) while building the summary, so this reuses that work instead of
/// walking the AST a second time. A name collision with a shadowing local can
/// only suppress a finding.
fn used_locals<'a>(summary: &'a FileSummary, component: &str) -> BTreeSet<&'a str> {
    summary
        .declarations
        .iter()
        .find(|declaration| declaration.names.iter().any(|name| name == component))
        .map(|declaration| {
            declaration
                .references
                .identifiers
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default()
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_uppercase())
}

pub(super) struct TypeProp {
    pub(super) name: String,
    pub(super) span: Span,
    pub(super) optional: bool,
}

/// Same-file object types that can back a typed props parameter, keyed by type
/// name. A name whose declaration is merged, generic, inherited, or otherwise
/// not a plain object literal is absent so the component abstains.
pub(super) fn same_file_props_types(
    program: &Program<'_>,
    source: &str,
    line_index: &LineIndex,
) -> BTreeMap<String, Vec<TypeProp>> {
    let mut types: BTreeMap<String, Option<Vec<TypeProp>>> = BTreeMap::new();
    for statement in &program.body {
        match statement {
            Statement::TSInterfaceDeclaration(interface) => {
                record_interface(&mut types, interface, source, line_index);
            }
            Statement::TSTypeAliasDeclaration(alias) => {
                record_alias(&mut types, alias, source, line_index);
            }
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::TSInterfaceDeclaration(interface)) => {
                    record_interface(&mut types, interface, source, line_index);
                }
                Some(Declaration::TSTypeAliasDeclaration(alias)) => {
                    record_alias(&mut types, alias, source, line_index);
                }
                _ => {}
            },
            _ => {}
        }
    }
    types
        .into_iter()
        .filter_map(|(name, props)| props.map(|props| (name, props)))
        .collect()
}

fn record_interface(
    types: &mut BTreeMap<String, Option<Vec<TypeProp>>>,
    interface: &TSInterfaceDeclaration<'_>,
    source: &str,
    line_index: &LineIndex,
) {
    let members = if interface.extends.is_empty() && interface.type_parameters.is_none() {
        Some(interface.body.body.as_slice())
    } else {
        None
    };
    record_type(
        types,
        interface.id.name.as_str(),
        members,
        source,
        line_index,
    );
}

fn record_alias(
    types: &mut BTreeMap<String, Option<Vec<TypeProp>>>,
    alias: &TSTypeAliasDeclaration<'_>,
    source: &str,
    line_index: &LineIndex,
) {
    let members = match (&alias.type_annotation, alias.type_parameters.is_none()) {
        (TSType::TSTypeLiteral(literal), true) => Some(literal.members.as_slice()),
        _ => None,
    };
    record_type(types, alias.id.name.as_str(), members, source, line_index);
}

fn record_type(
    types: &mut BTreeMap<String, Option<Vec<TypeProp>>>,
    name: &str,
    members: Option<&[TSSignature<'_>]>,
    source: &str,
    line_index: &LineIndex,
) {
    let Some(members) = members else {
        // A generic, inherited, or non-literal declaration makes the whole name
        // opaque, even when another declaration with the same name is plain.
        types.insert(name.to_string(), None);
        return;
    };
    let mut props = Vec::new();
    for member in members {
        let TSSignature::TSPropertySignature(property) = member else {
            continue;
        };
        let Some(prop) = property.key.static_name() else {
            continue;
        };
        props.push(TypeProp {
            name: prop.into_owned(),
            span: line_index.span(source, property.span.start, property.span.end),
            optional: property.optional,
        });
    }
    if props.is_empty() {
        // No named props: the typed parameter cannot be enumerated.
        types.insert(name.to_string(), None);
        return;
    }
    if types.contains_key(name) {
        // Declaration merging: the union of members is not represented.
        types.insert(name.to_string(), None);
    } else {
        types.insert(name.to_string(), Some(props));
    }
}

/// Harvest the props of a component parameter list.
///
/// `None` means the signature cannot be read with confidence, so the whole
/// component abstains: a rest parameter, a bare identifier parameter without a
/// same-file object type (the imported-interface shape), more than one
/// parameter, an array pattern, an object rest, a computed key, a nested
/// destructure, or a whole-object use of a typed props binding.
fn harvest_props(
    candidate: &ComponentCandidate<'_>,
    object_types: &BTreeMap<String, Vec<TypeProp>>,
    source: &str,
    line_index: &LineIndex,
) -> Option<Vec<HarvestedProp>> {
    let params = candidate.params;
    if params.rest.is_some() {
        return None;
    }
    if params.items.is_empty() {
        return Some(Vec::new());
    }
    if params.items.len() > 1 && !candidate.forward_ref {
        return None;
    }
    if params.items.len() > 2 {
        return None;
    }
    let parameter = &params.items[0];
    match &parameter.pattern.kind {
        BindingPatternKind::ObjectPattern(object) => harvest_destructured(
            object,
            parameter.pattern.type_annotation.as_deref(),
            object_types,
            source,
            line_index,
        ),
        BindingPatternKind::BindingIdentifier(identifier) => harvest_typed(
            identifier.name.as_str(),
            parameter.pattern.type_annotation.as_deref(),
            candidate.wrapper_props_type,
            candidate.body,
            object_types,
        ),
        _ => None,
    }
}

/// The prop names a component actually reads, or `None` when the signature
/// cannot be read with confidence.
///
/// A declared-but-unread prop is not consumed. This matters for Pages Router
/// pages because the page component is exported, so the separate
/// `unused_component_prop` warning abstains on it; the page-data rule is the
/// only check that can connect the returned key to the missing read.
pub(super) fn consumed_prop_names(
    params: &FormalParameters<'_>,
    body: Option<&FunctionBody<'_>>,
    object_types: &BTreeMap<String, Vec<TypeProp>>,
    source: &str,
    line_index: &LineIndex,
) -> Option<BTreeSet<String>> {
    if params.rest.is_some() || params.items.len() > 1 {
        return None;
    }
    let Some(parameter) = params.items.first() else {
        return Some(BTreeSet::new());
    };
    match &parameter.pattern.kind {
        BindingPatternKind::ObjectPattern(object) => {
            let props = harvest_destructured(
                object,
                parameter.pattern.type_annotation.as_deref(),
                object_types,
                source,
                line_index,
            )?;
            let used = referenced_locals(body?);
            Some(
                props
                    .into_iter()
                    .filter(|prop| used.contains(prop.local.as_str()))
                    .map(|prop| prop.prop)
                    .collect(),
            )
        }
        BindingPatternKind::BindingIdentifier(identifier) => {
            let props = harvest_typed(
                identifier.name.as_str(),
                parameter.pattern.type_annotation.as_deref(),
                None,
                body,
                object_types,
            )?;
            Some(
                props
                    .into_iter()
                    .filter(|prop| prop.used == Some(true))
                    .map(|prop| prop.prop)
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Identifier references in a component body. Over-crediting a shadowed name
/// can only suppress a finding, never create one.
fn referenced_locals(body: &FunctionBody<'_>) -> BTreeSet<String> {
    #[derive(Default)]
    struct Collector {
        names: BTreeSet<String>,
    }

    impl<'a> Visit<'a> for Collector {
        fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
            self.names.insert(identifier.name.to_string());
        }
    }

    let mut collector = Collector::default();
    collector.visit_function_body(body);
    collector.names
}

fn harvest_destructured(
    object: &ObjectPattern<'_>,
    annotation: Option<&TSTypeAnnotation<'_>>,
    object_types: &BTreeMap<String, Vec<TypeProp>>,
    source: &str,
    line_index: &LineIndex,
) -> Option<Vec<HarvestedProp>> {
    if object.rest.is_some() {
        return None;
    }
    let type_optionality = annotation
        .and_then(|annotation| type_optionality(&annotation.type_annotation, object_types));
    let mut props = Vec::new();
    for property in &object.properties {
        if property.computed {
            return None;
        }
        let prop = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name.to_string(),
            PropertyKey::StringLiteral(literal) => literal.value.to_string(),
            _ => return None,
        };
        let (local, has_default) = match &property.value.kind {
            BindingPatternKind::BindingIdentifier(identifier) => {
                (identifier.name.to_string(), false)
            }
            BindingPatternKind::AssignmentPattern(assignment) => match &assignment.left.kind {
                BindingPatternKind::BindingIdentifier(identifier) => {
                    (identifier.name.to_string(), true)
                }
                _ => return None,
            },
            _ => return None,
        };
        let optional = has_default
            || type_optionality
                .as_ref()
                .and_then(|optionality| optionality.get(&prop))
                .copied()
                .unwrap_or(false);
        props.push(HarvestedProp {
            prop,
            local,
            span: line_index.span(source, property.span.start, property.span.end),
            used: None,
            destructured: true,
            optional,
            has_default,
        });
    }
    Some(props)
}

/// Optionality by prop name for a destructured parameter's type annotation.
/// Only a same-file object literal or a bare same-file type name is resolved;
/// an imported, generic, or otherwise opaque annotation leaves every prop
/// non-optional, so the absent-prop rule abstains.
fn type_optionality(
    ty: &TSType<'_>,
    object_types: &BTreeMap<String, Vec<TypeProp>>,
) -> Option<BTreeMap<String, bool>> {
    match ty {
        TSType::TSTypeLiteral(literal) => {
            let mut optionality = BTreeMap::new();
            for member in &literal.members {
                if let TSSignature::TSPropertySignature(property) = member
                    && let Some(name) = property.key.static_name()
                {
                    optionality.insert(name.into_owned(), property.optional);
                }
            }
            Some(optionality)
        }
        TSType::TSTypeReference(reference) if reference.type_arguments.is_none() => {
            match &reference.type_name {
                TSTypeName::IdentifierReference(identifier) => {
                    object_types.get(identifier.name.as_str()).map(|props| {
                        props
                            .iter()
                            .map(|prop| (prop.name.clone(), prop.optional))
                            .collect()
                    })
                }
                TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
            }
        }
        TSType::TSParenthesizedType(parenthesized) => {
            type_optionality(&parenthesized.type_annotation, object_types)
        }
        _ => None,
    }
}

/// Harvest props for a bare identifier parameter. The type comes from the
/// parameter's own annotation, or from the `forwardRef<Ref, Props>` wrapper's
/// second generic argument when the inner parameter is unannotated. An own
/// annotation always wins; an unresolvable own annotation abstains rather than
/// falling back to the wrapper type.
fn harvest_typed(
    props_local: &str,
    annotation: Option<&TSTypeAnnotation<'_>>,
    wrapper_props_type: Option<&str>,
    body: Option<&FunctionBody<'_>>,
    object_types: &BTreeMap<String, Vec<TypeProp>>,
) -> Option<Vec<HarvestedProp>> {
    let type_name = match annotation {
        Some(annotation) => bare_props_type_name(&annotation.type_annotation)?,
        None => wrapper_props_type?,
    };
    let members = object_types.get(type_name)?;
    let body = body?;
    let mut usage = TypedPropsUsage::default();
    let mut visitor = TypedPropsVisitor {
        props_local,
        usage: &mut usage,
    };
    visitor.visit_function_body(body);
    if usage.whole_object_use() {
        return None;
    }
    Some(
        members
            .iter()
            .map(|member| HarvestedProp {
                prop: member.name.clone(),
                local: props_local.to_string(),
                span: member.span,
                used: Some(usage.member_uses.contains(&member.name)),
                destructured: false,
                optional: member.optional,
                has_default: false,
            })
            .collect(),
    )
}

/// The bare single-identifier name of a props type annotation, or `None` for a
/// shape Shrike cannot resolve to a same-file object declaration: a
/// generic-with-args (`Props<T>`), a qualified name (`NS.Props`), an
/// intersection, a union, or an inline type literal.
fn bare_props_type_name<'a>(ty: &'a TSType<'a>) -> Option<&'a str> {
    match ty {
        TSType::TSTypeReference(reference) if reference.type_arguments.is_none() => {
            match &reference.type_name {
                TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
                TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
            }
        }
        TSType::TSParenthesizedType(parenthesized) => {
            bare_props_type_name(&parenthesized.type_annotation)
        }
        _ => None,
    }
}

/// Per-component typed-props body usage: which declared prop names are read via
/// `<props>.<name>` or a `const { name } = props` destructure, and whether the
/// props binding is consumed as a whole object anywhere it is not one of those
/// two clean shapes. A whole-object use makes the prop set opaque and abstains.
#[derive(Default)]
struct TypedPropsUsage {
    member_uses: BTreeSet<String>,
    total_refs: u32,
    accounted_refs: u32,
}

impl TypedPropsUsage {
    fn whole_object_use(&self) -> bool {
        self.total_refs > self.accounted_refs
    }
}

struct TypedPropsVisitor<'a, 'b> {
    props_local: &'a str,
    usage: &'b mut TypedPropsUsage,
}

impl<'a> Visit<'a> for TypedPropsVisitor<'_, '_> {
    fn visit_identifier_reference(&mut self, reference: &IdentifierReference<'a>) {
        if reference.name.as_str() == self.props_local {
            self.usage.total_refs += 1;
        }
    }

    fn visit_static_member_expression(&mut self, member: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &member.object
            && object.name.as_str() == self.props_local
        {
            self.usage
                .member_uses
                .insert(member.property.name.to_string());
            self.usage.accounted_refs += 1;
        }
        walk::walk_static_member_expression(self, member);
    }

    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if let (BindingPatternKind::ObjectPattern(pattern), Some(Expression::Identifier(init))) =
            (&declarator.id.kind, &declarator.init)
            && init.name.as_str() == self.props_local
            && pattern.rest.is_none()
        {
            for property in &pattern.properties {
                if property.computed {
                    continue;
                }
                if let Some(name) = property.key.static_name() {
                    self.usage.member_uses.insert(name.into_owned());
                }
            }
            self.usage.accounted_refs += 1;
        }
        walk::walk_variable_declarator(self, declarator);
    }
}

fn function_body_returns_jsx(body: Option<&FunctionBody<'_>>) -> bool {
    body.is_some_and(|body| body.statements.iter().any(statement_returns_jsx))
}

fn arrow_returns_jsx(arrow: &ArrowFunctionExpression<'_>) -> bool {
    if arrow.expression {
        return arrow.get_expression().is_some_and(is_jsx_expression);
    }
    arrow.body.statements.iter().any(statement_returns_jsx)
}

fn statement_returns_jsx(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::ReturnStatement(statement) => {
            statement.argument.as_ref().is_some_and(is_jsx_expression)
        }
        Statement::IfStatement(statement) => {
            statement_returns_jsx(&statement.consequent)
                || statement
                    .alternate
                    .as_ref()
                    .is_some_and(statement_returns_jsx)
        }
        Statement::BlockStatement(block) => block.body.iter().any(statement_returns_jsx),
        Statement::SwitchStatement(statement) => statement
            .cases
            .iter()
            .any(|case| case.consequent.iter().any(statement_returns_jsx)),
        Statement::TryStatement(statement) => {
            statement.block.body.iter().any(statement_returns_jsx)
                || statement
                    .handler
                    .as_ref()
                    .is_some_and(|handler| handler.body.body.iter().any(statement_returns_jsx))
                || statement
                    .finalizer
                    .as_ref()
                    .is_some_and(|finalizer| finalizer.body.iter().any(statement_returns_jsx))
        }
        _ => false,
    }
}

/// Whether an expression returns JSX, unwrapping the wrappers a component
/// commonly uses. A conditional or logical return is enough: the component is
/// a renderer even when one branch returns `null`.
fn is_jsx_expression(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
        Expression::ParenthesizedExpression(inner) => is_jsx_expression(&inner.expression),
        Expression::ConditionalExpression(conditional) => {
            is_jsx_expression(&conditional.consequent) || is_jsx_expression(&conditional.alternate)
        }
        Expression::LogicalExpression(logical) => is_jsx_expression(&logical.right),
        Expression::TSAsExpression(inner) => is_jsx_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => is_jsx_expression(&inner.expression),
        Expression::TSNonNullExpression(inner) => is_jsx_expression(&inner.expression),
        _ => false,
    }
}
