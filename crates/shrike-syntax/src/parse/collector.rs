use oxc_ast::ast::{
    Argument, ArrayExpressionElement, BindingPatternKind, CallExpression, ComputedMemberExpression,
    Expression, FormalParameters, FunctionBody, IdentifierReference, ImportExpression,
    JSXIdentifier, JSXMemberExpression, JSXMemberExpressionObject, MemberExpression, NewExpression,
    ObjectPattern, PropertyKey, StaticMemberExpression, TSQualifiedName, TSTypeName,
    VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_ast_visit::walk::{
    walk_call_expression, walk_computed_member_expression, walk_import_expression,
    walk_new_expression, walk_static_member_expression, walk_ts_qualified_name,
    walk_variable_declaration, walk_variable_declarator,
};
use shrike_core::Span;

use crate::summary::{
    DynamicKind, DynamicRecord, DynamicSpecifier, MemberAccess, ModuleRefs, UnknownRecord,
    UnknownScope,
};

#[derive(Default)]
pub(super) struct RefCollector {
    pub(super) identifiers: Vec<String>,
    pub(super) free: Vec<String>,
    pub(super) members: Vec<MemberAccess>,
    pub(super) jsx: Vec<String>,
    pub(super) dynamic: Vec<DynamicRecord>,
    pub(super) unknown: Vec<UnknownRecord>,
    /// Import/require nodes already recorded by the declarator handler. The
    /// generic visitors skip these so one import never produces a precise
    /// record and a widening record at the same time.
    handled_dynamic_spans: Vec<oxc_span::Span>,
    /// True while walking a `const` declaration. Only `const` namespace
    /// bindings can be narrowed: `let` and `var` may be reassigned after the
    /// import, so their member uses do not prove which module is read.
    const_declaration: bool,
}

impl RefCollector {
    pub(super) fn into_parts(self) -> (ModuleRefs, Vec<DynamicRecord>, Vec<UnknownRecord>) {
        (
            ModuleRefs {
                identifiers: self.identifiers,
                free: self.free,
                members: self.members,
                jsx: self.jsx,
            },
            self.dynamic,
            self.unknown,
        )
    }
}

impl<'a> Visit<'a> for RefCollector {
    fn visit_identifier_reference(&mut self, it: &oxc_ast::ast::IdentifierReference<'a>) {
        self.identifiers.push(it.name.to_string());
        self.free.push(it.name.to_string());
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
            // Record the object as an ordinary reference, but not as a free
            // identifier: this use is precisely resolved through `members`.
            self.identifiers.push(object.name.to_string());
            self.members.push(MemberAccess {
                object: object.name.to_string(),
                property: Some(it.property.name.to_string()),
                computed: false,
                instance: false,
            });
            return;
        }
        if let Some(class) = instance_class_name(&it.object) {
            // `new Counter().increment()` and `Counter.prototype.increment`
            // read an instance member, not a static one.
            self.identifiers.push(class.name.to_string());
            self.members.push(MemberAccess {
                object: class.name.to_string(),
                property: Some(it.property.name.to_string()),
                computed: false,
                instance: true,
            });
            if let Expression::NewExpression(new) = &it.object {
                for argument in &new.arguments {
                    self.visit_argument(argument);
                }
            }
            return;
        }
        walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            self.identifiers.push(object.name.to_string());
            let property = literal_member_name(&it.expression);
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
                instance: false,
            });
            // The key expression may contain references (`UI[key]`), but the
            // object itself was handled above.
            self.visit_expression(&it.expression);
            return;
        }
        if let Some(class) = instance_class_name(&it.object) {
            let property = literal_member_name(&it.expression);
            self.identifiers.push(class.name.to_string());
            self.members.push(MemberAccess {
                object: class.name.to_string(),
                property,
                computed: true,
                instance: true,
            });
            if let Expression::NewExpression(new) = &it.object {
                for argument in &new.arguments {
                    self.visit_argument(argument);
                }
            }
            self.visit_expression(&it.expression);
            return;
        }
        walk_computed_member_expression(self, it);
    }

    fn visit_ts_qualified_name(&mut self, it: &TSQualifiedName<'a>) {
        // A qualified type name (`Status.Active`, `types.Config`) is the
        // type-position spelling of a member access. Recording it as a member
        // keeps the root identifier out of `free`, so an enum member used only
        // in a type annotation is credited precisely instead of widening every
        // member of the enum.
        if let TSTypeName::IdentifierReference(object) = &it.left {
            self.identifiers.push(object.name.to_string());
            self.members.push(MemberAccess {
                object: object.name.to_string(),
                property: Some(it.right.name.to_string()),
                computed: false,
                instance: false,
            });
            return;
        }
        walk_ts_qualified_name(self, it);
    }

    fn visit_jsx_member_expression(&mut self, it: &JSXMemberExpression<'a>) {
        // `<UI.Button />` is a JSX member expression, not a static member
        // expression. Record the root object and the final property so a
        // namespace import resolves to the component export it actually uses.
        match &it.object {
            JSXMemberExpressionObject::IdentifierReference(object) => {
                self.identifiers.push(object.name.to_string());
                self.members.push(MemberAccess {
                    object: object.name.to_string(),
                    property: Some(it.property.name.to_string()),
                    computed: false,
                    instance: false,
                });
            }
            JSXMemberExpressionObject::MemberExpression(inner) => {
                self.visit_jsx_member_expression(inner);
            }
            JSXMemberExpressionObject::ThisExpression(_) => {}
        }
        // Do not walk the property: it is a JSX identifier, not a reference to
        // a local binding. Walking it would mark an unrelated same-named local
        // as used.
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        let previous = self.const_declaration;
        self.const_declaration = it.kind == VariableDeclarationKind::Const;
        walk_variable_declaration(self, it);
        self.const_declaration = previous;
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let Some(records) = promise_all_dynamic(it) {
            for (kind, specifier, span, imported, binding) in records {
                self.dynamic.push(DynamicRecord {
                    kind,
                    specifier,
                    span: Span::default(),
                    imported,
                    binding,
                });
                self.handled_dynamic_spans.push(span);
            }
        } else if let Some((kind, specifier, span, imported, binding)) =
            declaration_dynamic(it, self.const_declaration)
        {
            self.dynamic.push(DynamicRecord {
                kind,
                specifier,
                span: Span::default(),
                imported,
                binding,
            });
            self.handled_dynamic_spans.push(span);
        }
        walk_variable_declarator(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if !self.handled_dynamic_spans.contains(&it.span) {
            self.dynamic.push(DynamicRecord {
                kind: DynamicKind::Import,
                specifier: dynamic_specifier(&it.source),
                span: Span::default(),
                imported: None,
                binding: None,
            });
        }
        walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some((record, span)) = then_dynamic(it) {
            self.dynamic.push(record);
            self.handled_dynamic_spans.push(span);
        }
        if let Some(specifier) = import_meta_glob_specifier(it) {
            self.dynamic.push(DynamicRecord {
                kind: DynamicKind::Import,
                specifier,
                span: Span::default(),
                imported: None,
                binding: None,
            });
        } else if let Some(specifier) = require_context_specifier(it) {
            self.dynamic.push(DynamicRecord {
                kind: DynamicKind::Require,
                specifier,
                span: Span::default(),
                imported: None,
                binding: None,
            });
        } else if let Some(specifiers) = worker_call_specifiers(it) {
            for specifier in specifiers {
                self.dynamic.push(DynamicRecord {
                    kind: DynamicKind::Worker,
                    specifier,
                    span: Span::default(),
                    imported: None,
                    binding: None,
                });
            }
        } else if let Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" && !self.handled_dynamic_spans.contains(&it.span) {
                if let Some(argument) = it.arguments.first() {
                    self.dynamic.push(DynamicRecord {
                        kind: DynamicKind::Require,
                        specifier: argument_specifier(argument),
                        span: Span::default(),
                        imported: None,
                        binding: None,
                    });
                }
            }
        }
        walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if worker_constructor(&it.callee) {
            if let Some(argument) = it.arguments.first().and_then(|arg| arg.as_expression()) {
                self.dynamic.push(DynamicRecord {
                    kind: DynamicKind::Worker,
                    specifier: worker_specifier(argument),
                    span: Span::default(),
                    imported: None,
                    binding: None,
                });
            }
        }
        walk_new_expression(self, it);
    }
}

/// Worker and worklet constructors load a module by URL instead of importing
/// it. The bundler still includes the target, so the module is live and its
/// top-level code runs.
fn worker_constructor(callee: &Expression<'_>) -> bool {
    let name = match callee {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    };
    matches!(name, Some("Worker" | "SharedWorker"))
}

fn has_property_name(expression: &Expression<'_>, name: &str) -> bool {
    match expression {
        Expression::Identifier(identifier) => identifier.name == name,
        Expression::StaticMemberExpression(member) => member.property.name == name,
        _ => false,
    }
}

fn is_worklet(expression: &Expression<'_>) -> bool {
    [
        "audioWorklet",
        "paintWorklet",
        "layoutWorklet",
        "animationWorklet",
        "worklet",
    ]
    .iter()
    .any(|name| has_property_name(expression, name))
}

/// `navigator.serviceWorker.register(...)`, `audioWorklet.addModule(...)`,
/// `CSS.paintWorklet.addModule(...)`, and `importScripts(...)` are runtime
/// module loads with the same reachability semantics as a worker constructor.
fn worker_call_specifiers(call: &CallExpression<'_>) -> Option<Vec<DynamicSpecifier>> {
    match &call.callee {
        Expression::Identifier(identifier) if identifier.name == "importScripts" => Some(
            call.arguments
                .iter()
                .filter_map(|argument| argument.as_expression())
                .map(worker_specifier)
                .collect(),
        ),
        Expression::StaticMemberExpression(member)
            if member.property.name == "register"
                && has_property_name(&member.object, "serviceWorker") =>
        {
            Some(
                call.arguments
                    .first()
                    .and_then(|argument| argument.as_expression())
                    .map(worker_specifier)
                    .into_iter()
                    .collect(),
            )
        }
        Expression::StaticMemberExpression(member)
            if member.property.name == "addModule" && is_worklet(&member.object) =>
        {
            Some(
                call.arguments
                    .first()
                    .and_then(|argument| argument.as_expression())
                    .map(worker_specifier)
                    .into_iter()
                    .collect(),
            )
        }
        _ => None,
    }
}

fn worker_specifier(expression: &Expression<'_>) -> DynamicSpecifier {
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
                DynamicSpecifier::TemplatePrefix(template.quasis.first().map_or_else(
                    String::new,
                    |quasi| {
                        quasi
                            .value
                            .cooked
                            .as_ref()
                            .map_or_else(|| quasi.value.raw.to_string(), ToString::to_string)
                    },
                ))
            }
        }
        // `new URL("./worker.ts", import.meta.url)` is the bundler-recognized
        // form. A URL with another base is a runtime location, not a module
        // this analysis can place.
        Expression::NewExpression(new) if url_constructor(&new.callee) => {
            let Some(first) = new
                .arguments
                .first()
                .and_then(|argument| argument.as_expression())
            else {
                return DynamicSpecifier::Unknown;
            };
            let has_module_base = new
                .arguments
                .get(1)
                .and_then(|argument| argument.as_expression())
                .is_some_and(import_meta_url);
            if !has_module_base {
                return DynamicSpecifier::Unknown;
            }
            worker_specifier(first)
        }
        Expression::ParenthesizedExpression(inner) => worker_specifier(&inner.expression),
        Expression::TSAsExpression(inner) => worker_specifier(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => worker_specifier(&inner.expression),
        Expression::TSTypeAssertion(inner) => worker_specifier(&inner.expression),
        Expression::TSNonNullExpression(inner) => worker_specifier(&inner.expression),
        Expression::TSInstantiationExpression(inner) => worker_specifier(&inner.expression),
        // A worker URL computed from a variable can point at any analyzed
        // module. Fail closed rather than report its contents dead.
        _ => DynamicSpecifier::Unknown,
    }
}

fn url_constructor(callee: &Expression<'_>) -> bool {
    match callee {
        Expression::Identifier(identifier) => identifier.name == "URL",
        Expression::StaticMemberExpression(member) => member.property.name == "URL",
        _ => false,
    }
}

fn import_meta_url(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::StaticMemberExpression(member) => {
            member.property.name == "url"
                && matches!(
                    &member.object,
                    Expression::MetaProperty(meta)
                        if meta.meta.name == "import" && meta.property.name == "meta"
                )
        }
        Expression::ParenthesizedExpression(inner) => import_meta_url(&inner.expression),
        Expression::TSAsExpression(inner) => import_meta_url(&inner.expression),
        Expression::TSNonNullExpression(inner) => import_meta_url(&inner.expression),
        _ => false,
    }
}

type DynamicDeclaration = (
    DynamicKind,
    DynamicSpecifier,
    oxc_span::Span,
    Option<Vec<String>>,
    Option<String>,
);

/// A dynamic import whose export use is statically visible at the declaration
/// site, for example `const { getDb } = await import("./db")` or
/// `const editor = (await import("./editor")).default`. `imported` stays
/// `None` for any shape that could use the namespace as a value.
///
/// A `const namespace = await import("./m")` binding is recorded separately:
/// its uses may appear later in the module, so `binding` names the local that
/// the post-visit pass narrows from the module's static member accesses.
fn declaration_dynamic(
    declarator: &VariableDeclarator<'_>,
    const_declaration: bool,
) -> Option<DynamicDeclaration> {
    let base = dynamic_base(declarator.init.as_ref()?)?;
    let imported = match &base.property {
        Some(property) => {
            if matches!(property.as_str(), "then" | "catch" | "finally") {
                None
            } else {
                Some(vec![property.clone()])
            }
        }
        None => match &declarator.id.kind {
            // Destructuring a promise is invalid, so only treat an object
            // pattern as an export list when the import is awaited. `require`
            // is synchronous.
            BindingPatternKind::ObjectPattern(pattern)
                if base.kind == DynamicKind::Require || base.awaited =>
            {
                object_pattern_imports(pattern)
            }
            _ => None,
        },
    };
    // Only a const binding is safe to narrow: `let`/`var` can be reassigned
    // after the import, so later member uses do not prove which value is read.
    let binding = if const_declaration && base.property.is_none() {
        match &declarator.id.kind {
            BindingPatternKind::BindingIdentifier(identifier)
                if base.kind == DynamicKind::Require || base.awaited =>
            {
                Some(identifier.name.to_string())
            }
            _ => None,
        }
    } else {
        None
    };
    Some((base.kind, base.specifier, base.span, imported, binding))
}

/// `const [{ getDb }, repository] = await Promise.all([import("./db"),
/// import("./repository")])` is a common Next.js/server-action lazy-loading
/// shape. Narrow each literal import to the exports its array binding consumes,
/// exactly as a direct awaited destructure would. A rest element, spread,
/// hole, or non-import element leaves the affected import widening.
fn promise_all_dynamic(declarator: &VariableDeclarator<'_>) -> Option<Vec<DynamicDeclaration>> {
    let Expression::AwaitExpression(awaited) = declarator.init.as_ref()? else {
        return None;
    };
    let Expression::CallExpression(call) = &awaited.argument else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(object) = &member.object else {
        return None;
    };
    if object.name != "Promise" || member.property.name != "all" {
        return None;
    }
    let Some(Argument::ArrayExpression(array)) = call.arguments.first() else {
        return None;
    };
    let BindingPatternKind::ArrayPattern(pattern) = &declarator.id.kind else {
        return None;
    };
    // A rest binding consumes every remaining namespace. Leave all imports to
    // the default widening record rather than guess.
    if pattern.rest.is_some() {
        return None;
    }
    let mut records = Vec::new();
    for (index, element) in array.elements.iter().enumerate() {
        let Some(binding) = pattern.elements.get(index).and_then(Option::as_ref) else {
            continue;
        };
        let ArrayExpressionElement::ImportExpression(import) = element else {
            continue;
        };
        let imported = match &binding.kind {
            BindingPatternKind::ObjectPattern(object) => object_pattern_imports(object),
            BindingPatternKind::BindingIdentifier(_) => None,
            _ => continue,
        };
        // An object pattern with a rest or computed key could read an unknown
        // export; skip it so the default record widens.
        if matches!(&binding.kind, BindingPatternKind::ObjectPattern(_)) && imported.is_none() {
            continue;
        }
        records.push((
            DynamicKind::Import,
            dynamic_specifier(&import.source),
            import.span,
            imported,
            None,
        ));
    }
    if records.is_empty() {
        None
    } else {
        Some(records)
    }
}

/// `import("./m").then((m) => m.export)` is the lazy-loading shape used by
/// React.lazy and router configs. The promise resolves to the module
/// namespace, so a callback whose parameter is only ever read through static
/// members consumes exactly those exports. Any other use of the parameter
/// (bare reference, computed access, spread, call, return) widens to the whole
/// namespace.
fn then_dynamic(call: &CallExpression<'_>) -> Option<(DynamicRecord, oxc_span::Span)> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name != "then" {
        return None;
    }
    let import = direct_import_expression(&member.object)?;
    let imported = match call.arguments.first()? {
        Argument::ArrowFunctionExpression(arrow) => {
            then_callback_exports(&arrow.params, Some(&arrow.body))
        }
        Argument::FunctionExpression(function) => {
            then_callback_exports(&function.params, function.body.as_deref())
        }
        _ => None,
    }?;
    Some((
        DynamicRecord {
            kind: DynamicKind::Import,
            specifier: dynamic_specifier(&import.source),
            span: Span::default(),
            imported: Some(imported),
            binding: None,
        },
        import.span,
    ))
}

/// The import expression directly under `.then`, unwrapping only syntactic
/// wrappers. An awaited import is a namespace value, not a promise, so
/// `.then` on it is not a module callback and must not narrow.
fn direct_import_expression<'a>(
    expression: &'a Expression<'a>,
) -> Option<&'a ImportExpression<'a>> {
    match expression {
        Expression::ImportExpression(import) => Some(import),
        Expression::ParenthesizedExpression(inner) => direct_import_expression(&inner.expression),
        Expression::TSAsExpression(inner) => direct_import_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => direct_import_expression(&inner.expression),
        Expression::TSTypeAssertion(inner) => direct_import_expression(&inner.expression),
        Expression::TSNonNullExpression(inner) => direct_import_expression(&inner.expression),
        Expression::TSInstantiationExpression(inner) => direct_import_expression(&inner.expression),
        _ => None,
    }
}

fn then_callback_exports(
    params: &FormalParameters<'_>,
    body: Option<&FunctionBody<'_>>,
) -> Option<Vec<String>> {
    let body = body?;
    if params.rest.is_some() || params.items.len() != 1 {
        return None;
    }
    match &params.items[0].pattern.kind {
        BindingPatternKind::ObjectPattern(pattern) => object_pattern_imports(pattern),
        BindingPatternKind::BindingIdentifier(parameter) => {
            let mut uses = ThenCallbackUses {
                parameter: parameter.name.as_str(),
                names: Vec::new(),
                widened: false,
            };
            uses.visit_function_body(body);
            // An unused parameter can still be read through `eval` or a
            // function expression's `arguments`; widen instead of assuming no
            // export is consumed.
            if uses.widened || uses.names.is_empty() {
                return None;
            }
            uses.names.sort();
            uses.names.dedup();
            Some(uses.names)
        }
        _ => None,
    }
}

struct ThenCallbackUses<'p> {
    parameter: &'p str,
    names: Vec<String>,
    widened: bool,
}

impl<'a> Visit<'a> for ThenCallbackUses<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if it.name == self.parameter {
            self.widened = true;
        }
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            if object.name == self.parameter {
                self.names.push(it.property.name.to_string());
                return;
            }
        }
        walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if let Expression::Identifier(object) = &it.object {
            if object.name == self.parameter {
                self.widened = true;
                // The key expression can contain other references.
                self.visit_expression(&it.expression);
                return;
            }
        }
        walk_computed_member_expression(self, it);
    }
}

struct DynamicBase {
    kind: DynamicKind,
    specifier: DynamicSpecifier,
    span: oxc_span::Span,
    /// True when the namespace value is awaited (or produced by `require`), so
    /// member access on it is an export access rather than a Promise property.
    awaited: bool,
    /// The first property after the namespace value, if the value is consumed
    /// directly by a member expression.
    property: Option<String>,
}

fn dynamic_base(expression: &Expression<'_>) -> Option<DynamicBase> {
    match expression {
        Expression::ParenthesizedExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSAsExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSTypeAssertion(inner) => dynamic_base(&inner.expression),
        Expression::TSNonNullExpression(inner) => dynamic_base(&inner.expression),
        Expression::TSInstantiationExpression(inner) => dynamic_base(&inner.expression),
        Expression::AwaitExpression(inner) => {
            let mut base = dynamic_base(&inner.argument)?;
            base.awaited = true;
            Some(base)
        }
        Expression::ImportExpression(import) => Some(DynamicBase {
            kind: DynamicKind::Import,
            specifier: dynamic_specifier(&import.source),
            span: import.span,
            awaited: false,
            property: None,
        }),
        Expression::CallExpression(call) if is_require(call) => {
            let argument = call.arguments.first()?;
            Some(DynamicBase {
                kind: DynamicKind::Require,
                specifier: argument_specifier(argument),
                span: call.span,
                // `require` returns the module exports object directly.
                awaited: true,
                property: None,
            })
        }
        Expression::StaticMemberExpression(member) => {
            let mut base = dynamic_base(&member.object)?;
            if base.property.is_some() {
                return Some(base);
            }
            if base.kind == DynamicKind::Import && !base.awaited {
                return None;
            }
            base.property = Some(member.property.name.to_string());
            Some(base)
        }
        Expression::ComputedMemberExpression(member) => {
            let Expression::StringLiteral(property) = &member.expression else {
                return None;
            };
            let mut base = dynamic_base(&member.object)?;
            if base.property.is_some() {
                return Some(base);
            }
            if base.kind == DynamicKind::Import && !base.awaited {
                return None;
            }
            base.property = Some(property.value.to_string());
            Some(base)
        }
        _ => None,
    }
}

fn literal_member_name(expression: &Expression<'_>) -> Option<String> {
    match expression {
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

/// The class whose instance is the object of a member access:
/// `new Counter().increment` or `Counter.prototype.increment`.
fn instance_class_name<'a, 'b>(
    expression: &'b Expression<'a>,
) -> Option<&'b oxc_ast::ast::IdentifierReference<'a>> {
    match expression {
        Expression::NewExpression(new) => match &new.callee {
            Expression::Identifier(class) => Some(class),
            _ => None,
        },
        Expression::StaticMemberExpression(member) if member.property.name == "prototype" => {
            match &member.object {
                Expression::Identifier(class) => Some(class),
                _ => None,
            }
        }
        _ => None,
    }
}

fn is_require(call: &CallExpression<'_>) -> bool {
    matches!(&call.callee, Expression::Identifier(callee) if callee.name == "require")
}

/// Export names bound by an object destructuring pattern. Any rest element,
/// computed key, or non-literal key could read an unknown export, so those
/// shapes widen instead.
fn object_pattern_imports(pattern: &ObjectPattern<'_>) -> Option<Vec<String>> {
    if pattern.rest.is_some() {
        return None;
    }
    let mut names = Vec::new();
    for property in &pattern.properties {
        if property.computed {
            return None;
        }
        let name = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name.to_string(),
            PropertyKey::StringLiteral(literal) => literal.value.to_string(),
            PropertyKey::NumericLiteral(literal) => literal.raw.as_ref().map_or_else(
                || literal.value.to_string(),
                std::string::ToString::to_string,
            ),
            _ => return None,
        };
        names.push(name);
    }
    names.sort();
    names.dedup();
    if names.is_empty() { None } else { Some(names) }
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

/// `import.meta.glob(...)`: Vite's compile-time module glob. The argument is
/// either one literal pattern or an array of literal patterns; entries that
/// start with `!` are excludes for every positive entry in the same call. Any
/// non-literal shape stays unknown so no module is guessed dead.
fn import_meta_glob_specifier(call: &CallExpression<'_>) -> Option<DynamicSpecifier> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if !matches!(member.property.name.as_str(), "glob" | "globEager") {
        return None;
    }
    let Expression::MetaProperty(meta) = &member.object else {
        return None;
    };
    if meta.meta.name != "import" || meta.property.name != "meta" {
        return None;
    }
    Some(glob_specifier(call.arguments.first()?))
}

/// `require.context(directory, recursive?, regexp?)`: webpack's directory
/// import. The optional regular expression only narrows the set, so ignoring
/// it widens the match and stays conservative. A non-literal directory or
/// recursion flag cannot be bounded.
fn require_context_specifier(call: &CallExpression<'_>) -> Option<DynamicSpecifier> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name != "context" {
        return None;
    }
    let Expression::Identifier(object) = &member.object else {
        return None;
    };
    if object.name != "require" {
        return None;
    }
    let Some(Argument::StringLiteral(directory)) = call.arguments.first() else {
        return Some(DynamicSpecifier::Unknown);
    };
    let recursive = match call.arguments.get(1) {
        None => true,
        Some(Argument::BooleanLiteral(literal)) => literal.value,
        Some(_) => return Some(DynamicSpecifier::Unknown),
    };
    let directory = directory.value.trim_end_matches('/');
    let directory = escape_glob_literal(directory);
    let pattern = if directory.is_empty() || directory == "." {
        if recursive {
            "./**/*".to_string()
        } else {
            "./*".to_string()
        }
    } else if recursive {
        format!("{directory}/**/*")
    } else {
        format!("{directory}/*")
    };
    Some(DynamicSpecifier::Glob {
        patterns: vec![pattern],
        excludes: Vec::new(),
    })
}

/// `require.context` takes a literal directory, so a directory whose name looks
/// like glob syntax must not widen the match. Each metacharacter becomes a
/// one-character class, as Vite and Fallow do for literal pattern text.
fn escape_glob_literal(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '{' | '}') {
            output.push('[');
            output.push(character);
            output.push(']');
        } else {
            output.push(character);
        }
    }
    output
}

fn glob_specifier(argument: &Argument<'_>) -> DynamicSpecifier {
    let values: Vec<String> = match argument {
        Argument::StringLiteral(literal) => vec![literal.value.to_string()],
        Argument::ArrayExpression(array) => {
            let mut values = Vec::with_capacity(array.elements.len());
            for element in &array.elements {
                let ArrayExpressionElement::StringLiteral(literal) = element else {
                    return DynamicSpecifier::Unknown;
                };
                values.push(literal.value.to_string());
            }
            values
        }
        _ => return DynamicSpecifier::Unknown,
    };
    let mut patterns = Vec::new();
    let mut excludes = Vec::new();
    for value in values {
        if let Some(exclude) = value.strip_prefix('!') {
            if !exclude.is_empty() {
                excludes.push(exclude.to_string());
            }
        } else if !value.is_empty() {
            patterns.push(value);
        }
    }
    DynamicSpecifier::Glob { patterns, excludes }
}

#[allow(dead_code)]
fn _span_is_used(span: oxc_span::Span) -> u32 {
    span.start
}

#[allow(dead_code)]
fn _member_is_used(member: &MemberExpression<'_>) -> bool {
    matches!(member, MemberExpression::StaticMemberExpression(_))
}
