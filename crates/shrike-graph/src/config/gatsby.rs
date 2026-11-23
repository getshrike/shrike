//! Static Gatsby page-template extraction.
//!
//! Gatsby loads a page component only when `createPage({ component })` names
//! it. Templates under `src/templates` are not discovered by directory
//! convention, so a template that no `createPage` call references is genuinely
//! dead. Knip protects the whole directory; Shrike follows the actual page
//! registrations instead.
//!
//! `gatsby-node.{js,ts,...}` is executed by Gatsby, so Shrike reads it
//! statically: literal paths, template literals, `path.resolve`/`path.join`,
//! `require.resolve`, and local bindings are resolved. When a component value
//! cannot be proved, the package falls back to protecting every module under
//! `src/templates`, which fails toward a missing candidate rather than
//! inventing a live page.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    BindingPatternKind, CallExpression, Expression, ImportDeclaration, ImportDeclarationSpecifier,
    PropertyKey, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SourceType;

use super::{find_property, unwrap, with_config_ast};

/// Static page-template references found in a Gatsby node file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TemplateScan {
    /// Module specifiers relative to the `gatsby-node` file.
    pub specifiers: Vec<String>,
    /// At least one `createPage` component could not be resolved statically.
    pub dynamic: bool,
}

/// Whether `path` is a Gatsby node file. Gatsby accepts JavaScript, ESM,
/// CommonJS, and TypeScript here.
#[must_use]
pub(crate) fn is_node_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    stem == "gatsby-node"
        && matches!(
            extension,
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
        )
}

/// Read the page components registered by `createPage` calls.
#[must_use]
pub(crate) fn template_references(path: &str, source: &str) -> TemplateScan {
    // A node file Shrike cannot parse may contain any registration. Fail
    // closed toward the conventional template directory.
    let fail_closed = || TemplateScan {
        specifiers: Vec::new(),
        dynamic: true,
    };
    let Some(kind) = SourceType::from_path(path).ok() else {
        return fail_closed();
    };
    with_config_ast(kind, source, |program| {
        let mut bindings = Bindings::default();
        bindings.visit_program(program);
        let mut visitor = PageVisitor {
            bindings: &bindings,
            scan: TemplateScan::default(),
        };
        visitor.visit_program(program);
        visitor.scan.specifiers.sort();
        visitor.scan.specifiers.dedup();
        visitor.scan
    })
    .unwrap_or_else(fail_closed)
}

#[derive(Clone, Debug)]
enum Binding {
    /// A path expression that is already relative to the node file.
    Path(String),
    /// A module imported or required by the node file.
    Module(String),
}

#[derive(Default)]
struct Bindings {
    values: BTreeMap<String, Binding>,
    path_objects: BTreeSet<String>,
    path_functions: BTreeSet<String>,
}

impl<'a> Visit<'a> for Bindings {
    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if let BindingPatternKind::BindingIdentifier(identifier) = &declarator.id.kind
            && let Some(init) = &declarator.init
        {
            if is_path_require(init) {
                self.path_objects.insert(identifier.name.to_string());
            } else if let Some(binding) = eval_binding(self, init) {
                self.values.insert(identifier.name.to_string(), binding);
            }
        }
        if let BindingPatternKind::ObjectPattern(pattern) = &declarator.id.kind
            && declarator.init.as_ref().is_some_and(is_path_require)
        {
            for property in &pattern.properties {
                let name = match &property.key {
                    PropertyKey::StaticIdentifier(identifier) => identifier.name.as_str(),
                    PropertyKey::StringLiteral(literal) => literal.value.as_str(),
                    _ => continue,
                };
                if matches!(name, "resolve" | "join")
                    && let BindingPatternKind::BindingIdentifier(local) = &property.value.kind
                {
                    self.path_functions.insert(local.name.to_string());
                }
            }
        }
        walk::walk_variable_declarator(self, declarator);
    }

    fn visit_import_declaration(&mut self, declaration: &ImportDeclaration<'a>) {
        let source = declaration.source.value.as_str();
        if matches!(source, "path" | "node:path") {
            if let Some(specifiers) = &declaration.specifiers {
                for specifier in specifiers {
                    match specifier {
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            self.path_objects.insert(specifier.local.name.to_string());
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            self.path_objects.insert(specifier.local.name.to_string());
                        }
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                            if matches!(specifier.imported.name().as_str(), "resolve" | "join") {
                                self.path_functions.insert(specifier.local.name.to_string());
                            }
                        }
                    }
                }
            }
        } else if source.starts_with('.') {
            // A local import used as `component` is either a module object or a
            // re-exported path. Recording the module specifier is the same
            // reachability result either way.
            if let Some(specifiers) = &declaration.specifiers {
                for specifier in specifiers {
                    let local = match specifier {
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            &specifier.local
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            &specifier.local
                        }
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => &specifier.local,
                    };
                    self.values
                        .insert(local.name.to_string(), Binding::Module(source.to_string()));
                }
            }
        }
        walk::walk_import_declaration(self, declaration);
    }
}

struct PageVisitor<'a> {
    bindings: &'a Bindings,
    scan: TemplateScan,
}

impl<'a> Visit<'a> for PageVisitor<'_> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if is_create_page(call) {
            let component = call
                .arguments
                .first()
                .and_then(|argument| argument.as_expression())
                .and_then(|expression| match unwrap(expression) {
                    Expression::ObjectExpression(object) => find_property(object, "component"),
                    _ => None,
                });
            match component {
                Some(component) => {
                    if !collect_components(self.bindings, component, &mut self.scan.specifiers) {
                        self.scan.dynamic = true;
                    }
                }
                None => self.scan.dynamic = true,
            }
        }
        walk::walk_call_expression(self, call);
    }
}

fn is_create_page(call: &CallExpression<'_>) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(member) => member.property.name == "createPage",
        Expression::Identifier(identifier) => identifier.name == "createPage",
        _ => false,
    }
}

/// Resolve a `component` value into module specifiers. Returns `false` when any
/// part is opaque, so the caller can fail closed.
fn collect_components(
    bindings: &Bindings,
    expression: &Expression<'_>,
    components: &mut Vec<String>,
) -> bool {
    match unwrap(expression) {
        Expression::ConditionalExpression(conditional) => {
            let consequent = collect_components(bindings, &conditional.consequent, components);
            let alternate = collect_components(bindings, &conditional.alternate, components);
            consequent && alternate
        }
        Expression::Identifier(identifier) => match bindings.values.get(identifier.name.as_str()) {
            Some(Binding::Path(path)) => {
                push_component(components, path);
                true
            }
            Some(Binding::Module(source)) => {
                push_component(components, source);
                true
            }
            None => false,
        },
        Expression::CallExpression(call) => {
            if let Some(source) = require_source(call) {
                push_component(components, source);
                return true;
            }
            match eval_path(bindings, expression) {
                Some(path) => {
                    push_component(components, &path);
                    true
                }
                None => false,
            }
        }
        expression => match eval_path(bindings, expression) {
            Some(path) => {
                push_component(components, &path);
                true
            }
            None => false,
        },
    }
}

fn eval_binding(bindings: &Bindings, expression: &Expression<'_>) -> Option<Binding> {
    if let Some(path) = eval_path(bindings, expression) {
        return Some(Binding::Path(path));
    }
    let Expression::CallExpression(call) = unwrap(expression) else {
        return None;
    };
    require_source(call).map(|source| Binding::Module(source.to_string()))
}

fn push_component(components: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() || value.starts_with('/') {
        return;
    }
    if value.starts_with('.') {
        components.push(value.to_string());
    } else {
        components.push(format!("./{value}"));
    }
}

fn require_source<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != "require" {
        return None;
    }
    let Expression::StringLiteral(literal) = call.arguments.first()?.as_expression()? else {
        return None;
    };
    let source = literal.value.as_str();
    source.starts_with('.').then_some(source)
}

fn is_path_require(expression: &Expression<'_>) -> bool {
    let Expression::CallExpression(call) = unwrap(expression) else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    if callee.name != "require" {
        return false;
    }
    call.arguments
        .first()
        .and_then(|argument| argument.as_expression())
        .is_some_and(|argument| {
            matches!(
                argument,
                Expression::StringLiteral(literal)
                    if matches!(literal.value.as_str(), "path" | "node:path")
            )
        })
}

fn eval_path(bindings: &Bindings, expression: &Expression<'_>) -> Option<String> {
    match unwrap(expression) {
        Expression::StringLiteral(literal) => {
            let value = literal.value.as_str();
            (!value.starts_with('/')).then(|| value.to_string())
        }
        Expression::TemplateLiteral(template) => {
            let mut value = String::new();
            for (index, quasi) in template.quasis.iter().enumerate() {
                value.push_str(quasi.value.cooked.as_ref()?);
                if let Some(expression) = template.expressions.get(index) {
                    value.push_str(&eval_path(bindings, expression)?);
                }
            }
            Some(value)
        }
        Expression::Identifier(identifier) if identifier.name == "__dirname" => Some(String::new()),
        Expression::Identifier(identifier) => match bindings.values.get(identifier.name.as_str()) {
            Some(Binding::Path(value)) => Some(value.clone()),
            _ => None,
        },
        Expression::CallExpression(call) => eval_call(bindings, call),
        _ => None,
    }
}

fn eval_call(bindings: &Bindings, call: &CallExpression<'_>) -> Option<String> {
    if let Expression::StaticMemberExpression(member) = &call.callee {
        if let Expression::Identifier(object) = &member.object {
            if object.name == "require" && member.property.name == "resolve" {
                let argument = call.arguments.first()?.as_expression()?;
                return eval_path(bindings, argument);
            }
            if bindings.path_objects.contains(object.name.as_str())
                && matches!(member.property.name.as_str(), "resolve" | "join")
            {
                return eval_path_call(bindings, call);
            }
            if object.name == "process"
                && member.property.name == "cwd"
                && call.arguments.is_empty()
            {
                return Some(String::new());
            }
        }
    }
    if let Expression::Identifier(callee) = &call.callee
        && bindings.path_functions.contains(callee.name.as_str())
    {
        return eval_path_call(bindings, call);
    }
    None
}

fn eval_path_call(bindings: &Bindings, call: &CallExpression<'_>) -> Option<String> {
    let mut parts = Vec::new();
    for argument in &call.arguments {
        parts.push(eval_path(bindings, argument.as_expression()?)?);
    }
    Some(join(&parts))
}

/// Join path parts and collapse `.`/`..` segments. Leading `..` segments are
/// kept: a template may live outside its package in a monorepo.
fn join(parts: &[String]) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for part in parts {
        for segment in part.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    if segments.last().is_some_and(|last| *last != "..") {
                        segments.pop();
                    } else {
                        segments.push("..");
                    }
                }
                segment => segments.push(segment),
            }
        }
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(source: &str) -> TemplateScan {
        template_references("gatsby-node.ts", source)
    }

    #[test]
    fn reads_literal_resolve_and_require_components() {
        let result = scan(
            r#"
            import path from "node:path";
            const local = path.resolve("src/templates/local.tsx");
            export async function createPages({ actions }) {
              actions.createPage({
                path: "/literal",
                component: "./src/templates/literal.tsx",
              });
              actions.createPage({
                path: "/resolved",
                component: path.resolve(__dirname, "src/templates/resolved.tsx"),
              });
              actions.createPage({
                path: "/required",
                component: require.resolve("./src/templates/required.tsx"),
              });
              actions.createPage({
                path: "/bound",
                component: local,
              });
            }
            "#,
        );
        assert_eq!(
            result.specifiers,
            vec![
                "./src/templates/literal.tsx",
                "./src/templates/local.tsx",
                "./src/templates/required.tsx",
                "./src/templates/resolved.tsx",
            ]
        );
        assert!(!result.dynamic);
    }

    #[test]
    fn reads_template_literals_and_conditional_components() {
        let result = scan(
            r#"
            import path from "path";
            const template = "post";
            export const createPages = ({ actions: { createPage } }) => {
              createPage({
                path: "/post",
                component: path.join(__dirname, `src/templates/${template}.tsx`),
              });
              createPage({
                path: "/other",
                component: Math.random() > 0.5 ? "./src/templates/a.tsx" : "./src/templates/b.tsx",
              });
            };
            "#,
        );
        assert_eq!(
            result.specifiers,
            vec![
                "./src/templates/a.tsx",
                "./src/templates/b.tsx",
                "./src/templates/post.tsx"
            ]
        );
        assert!(!result.dynamic);
    }

    #[test]
    fn reads_local_import_bindings() {
        let result = scan(
            r#"
            import BlogPost from "./src/templates/blog-post";
            export const createPages = ({ actions }) => {
              actions.createPage({ path: "/blog", component: BlogPost });
            };
            "#,
        );
        assert_eq!(result.specifiers, vec!["./src/templates/blog-post"]);
        assert!(!result.dynamic);
    }

    #[test]
    fn unresolved_components_fail_closed() {
        assert!(
            scan("export const createPages = ({ actions, page }) => actions.createPage(page);\n")
                .dynamic
        );
        assert!(
            scan(
                "export const createPages = ({ actions }) => actions.createPage({ path: '/', component: getTemplate() });\n"
            )
            .dynamic
        );
        assert!(
            scan(
                "export const createPages = ({ actions }) => actions.createPage({ ...page, context: {} });\n"
            )
            .dynamic
        );
        assert!(
            template_references("gatsby-node.ts", "export const createPages = (").dynamic,
            "an unparsable node file falls back to the template directory"
        );
    }

    #[test]
    fn ignores_non_component_calls() {
        let result = scan(
            r#"
            export const createPages = ({ actions }) => {
              actions.createRedirect({ fromPath: "/a", toPath: "/b" });
              other.createPage({ path: "/ignored", component: "./src/not-a-template.tsx" });
            };
            "#,
        );
        assert_eq!(
            result.specifiers,
            vec!["./src/not-a-template.tsx"],
            "a createPage call is the registration API regardless of the object name"
        );
        assert!(!result.dynamic);
    }

    #[test]
    fn recognizes_node_file_names() {
        for path in [
            "gatsby-node.js",
            "gatsby-node.ts",
            "apps/site/gatsby-node.tsx",
            "apps/site/gatsby-node.mjs",
        ] {
            assert!(is_node_path(path), "{path}");
        }
        for path in ["gatsby-config.ts", "gatsby-node.test.ts", "gatsby-nodes.ts"] {
            assert!(!is_node_path(path), "{path}");
        }
    }
}
