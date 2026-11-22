//! Static webpack entry extraction.
//!
//! Knip executes `webpack.config.*` and reads the resulting object, so dynamic
//! entry expressions work there. Shrike stays static: it parses the config with
//! oxc and follows literal values, simple string constants, and the common
//! `path.resolve`/`path.join` forms. Anything the evaluator cannot prove is
//! skipped, which fails toward missing an entry rather than inventing one.
//!
//! The extractor deliberately looks at every object literal with an `entry`
//! property. Webpack configs are frequently split into fragments that are later
//! combined with `webpack-merge`, and following the actual merge graph would add
//! little over reading each fragment. A stray `entry` key inside a plugin
//! option resolves to no analyzed file and has no effect.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    BindingPatternKind, CallExpression, Expression, ImportDeclaration, ImportDeclarationSpecifier,
    ObjectExpression, ObjectPropertyKind, PropertyKey, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SourceType;

/// Where an entry specifier is anchored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EntryBase {
    /// Relative to the directory that holds the webpack config.
    ConfigDir,
    /// Relative to the package root, which is webpack's default `context`.
    PackageDir,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WebpackEntry {
    pub specifier: String,
    pub base: EntryBase,
}

/// Webpack config filenames. Root configs match at any depth; target-specific
/// names are accepted in the conventional config directories, where a helper
/// module without an `entry` object simply yields nothing.
#[must_use]
pub(crate) fn is_config_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return false;
    };
    if !matches!(extension, "js" | "ts" | "mjs" | "cjs" | "mts" | "cts") {
        return false;
    }
    if stem == "webpack.config" || (stem.starts_with("webpack.") && stem.ends_with(".config")) {
        return true;
    }
    if !stem.starts_with("webpack.") {
        return false;
    }
    let directory = path.rsplit_once('/').map_or("", |(directory, _)| directory);
    let parent = directory.rsplit('/').next().unwrap_or(directory);
    matches!(parent, "config" | "build" | "webpack")
}

/// Extract the statically visible entry points of a webpack configuration.
#[must_use]
pub(crate) fn entries(path: &str, source: &str) -> Vec<WebpackEntry> {
    let source_type =
        SourceType::from_path(path).unwrap_or_else(|_| SourceType::mjs().with_typescript(true));
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Vec::new();
    }
    let mut bindings = Bindings::default();
    bindings.visit_program(&parsed.program);
    let mut visitor = EntryVisitor {
        bindings: &bindings,
        entries: Vec::new(),
    };
    visitor.visit_program(&parsed.program);
    visitor.entries.sort();
    visitor.entries.dedup();
    visitor.entries
}

#[derive(Clone, Debug)]
enum BindingValue {
    Path(PathValue),
    Entries(Vec<PathValue>),
}

#[derive(Default)]
struct Bindings {
    values: BTreeMap<String, BindingValue>,
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
            } else if matches!(
                unwrap_expression(init),
                Expression::ArrayExpression(_) | Expression::ObjectExpression(_)
            ) {
                let mut values = Vec::new();
                collect_entry_values(self, init, &mut values);
                if !values.is_empty() {
                    self.values
                        .insert(identifier.name.to_string(), BindingValue::Entries(values));
                }
            } else if let Some(value) = eval_path(self, init) {
                self.values
                    .insert(identifier.name.to_string(), BindingValue::Path(value));
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
        if matches!(declaration.source.value.as_str(), "path" | "node:path") {
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
        }
        walk::walk_import_declaration(self, declaration);
    }
}

struct EntryVisitor<'a> {
    bindings: &'a Bindings,
    entries: Vec<WebpackEntry>,
}

impl<'a> Visit<'a> for EntryVisitor<'_> {
    fn visit_object_expression(&mut self, object: &ObjectExpression<'a>) {
        let mut entries = Vec::new();
        collect_config_entries(self.bindings, object, &mut entries);
        self.entries.extend(entries);
        walk::walk_object_expression(self, object);
    }
}

fn is_path_require(expression: &Expression<'_>) -> bool {
    let Expression::CallExpression(call) = expression else {
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

fn collect_config_entries(
    bindings: &Bindings,
    object: &ObjectExpression<'_>,
    entries: &mut Vec<WebpackEntry>,
) {
    let Some(entry) = find_property(object, "entry") else {
        return;
    };
    let context = find_property(object, "context")
        .and_then(|expression| eval_path(bindings, expression))
        .map(|value| match value {
            // Webpack resolves a literal `context` against the process cwd,
            // which is the package root when a script runs the config.
            PathValue::Relative(path) => PathValue::PackageDir(path),
            value => value,
        });
    let mut raw = Vec::new();
    collect_entry_values(bindings, entry, &mut raw);
    for value in raw {
        let anchored = match value {
            PathValue::Relative(specifier) => match &context {
                Some(PathValue::ConfigDir(base)) => {
                    PathValue::ConfigDir(join(&[base.as_str(), specifier.as_str()]))
                }
                Some(PathValue::PackageDir(base)) => {
                    PathValue::PackageDir(join(&[base.as_str(), specifier.as_str()]))
                }
                _ => PathValue::PackageDir(join(&[specifier.as_str()])),
            },
            value => value,
        };
        let (specifier, base) = match anchored {
            PathValue::ConfigDir(specifier) => (specifier, EntryBase::ConfigDir),
            PathValue::PackageDir(specifier) => (specifier, EntryBase::PackageDir),
            PathValue::Relative(specifier) => (specifier, EntryBase::PackageDir),
        };
        if !specifier.is_empty() {
            entries.push(WebpackEntry { specifier, base });
        }
    }
}

fn collect_entry_values(
    bindings: &Bindings,
    expression: &Expression<'_>,
    values: &mut Vec<PathValue>,
) {
    match unwrap_expression(expression) {
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(expression) = element.as_expression() {
                    collect_entry_values(bindings, expression, values);
                }
            }
        }
        Expression::ObjectExpression(object) => {
            if let Some(import) = find_property(object, "import") {
                collect_entry_values(bindings, import, values);
            } else {
                for property in &object.properties {
                    if let ObjectPropertyKind::ObjectProperty(property) = property
                        && !property.computed
                    {
                        collect_entry_values(bindings, &property.value, values);
                    }
                }
            }
        }
        Expression::Identifier(identifier) => {
            if let Some(value) = bindings.values.get(identifier.name.as_str()) {
                match value {
                    BindingValue::Path(value) => values.push(value.clone()),
                    BindingValue::Entries(entries) => values.extend(entries.iter().cloned()),
                }
            }
        }
        expression => {
            if let Some(value) = eval_path(bindings, expression) {
                values.push(value);
            }
        }
    }
}

#[derive(Clone, Debug)]
enum PathValue {
    /// A literal written in the source, before a `context` is applied.
    Relative(String),
    /// A path anchored at the config file's directory.
    ConfigDir(String),
    /// A path anchored at the package root (webpack's cwd).
    PackageDir(String),
}

fn eval_path(bindings: &Bindings, expression: &Expression<'_>) -> Option<PathValue> {
    match unwrap_expression(expression) {
        Expression::StringLiteral(literal) => {
            let value = literal.value.as_str();
            if value.starts_with('/') {
                return None;
            }
            Some(PathValue::Relative(value.to_string()))
        }
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            let value = template.quasis.first()?.value.cooked.as_ref()?;
            if value.starts_with('/') {
                return None;
            }
            Some(PathValue::Relative(value.to_string()))
        }
        Expression::Identifier(identifier) if identifier.name == "__dirname" => {
            Some(PathValue::ConfigDir(String::new()))
        }
        Expression::Identifier(identifier) => {
            match bindings.values.get(identifier.name.as_str())? {
                BindingValue::Path(value) => Some(value.clone()),
                BindingValue::Entries(_) => None,
            }
        }
        Expression::CallExpression(call) => eval_call(bindings, call),
        _ => None,
    }
}

fn eval_call(bindings: &Bindings, call: &CallExpression<'_>) -> Option<PathValue> {
    if let Expression::StaticMemberExpression(member) = &call.callee {
        if let Expression::Identifier(object) = &member.object {
            if bindings.path_objects.contains(object.name.as_str())
                && matches!(member.property.name.as_str(), "resolve" | "join")
            {
                return eval_path_call(bindings, call);
            }
            if object.name == "process"
                && member.property.name == "cwd"
                && call.arguments.is_empty()
            {
                return Some(PathValue::PackageDir(String::new()));
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

fn eval_path_call(bindings: &Bindings, call: &CallExpression<'_>) -> Option<PathValue> {
    let mut base = EntryBase::PackageDir;
    let mut parts = Vec::new();
    for argument in &call.arguments {
        let expression = argument.as_expression()?;
        match eval_path(bindings, expression)? {
            PathValue::Relative(value) => parts.push(value),
            PathValue::ConfigDir(value) => {
                base = EntryBase::ConfigDir;
                parts.push(value);
            }
            PathValue::PackageDir(value) => parts.push(value),
        }
    }
    let joined = join_refs(&parts);
    Some(match base {
        EntryBase::ConfigDir => PathValue::ConfigDir(joined),
        EntryBase::PackageDir => PathValue::PackageDir(joined),
    })
}

fn unwrap_expression<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::ParenthesizedExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSAsExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap_expression(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap_expression(&inner.expression),
        _ => expression,
    }
}

fn find_property<'a>(object: &'a ObjectExpression<'a>, name: &str) -> Option<&'a Expression<'a>> {
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.computed {
            return None;
        }
        let matches = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name == name,
            PropertyKey::StringLiteral(literal) => literal.value == name,
            _ => false,
        };
        matches.then_some(&property.value)
    })
}

/// Join path parts and collapse `.`/`..` segments. Leading `..` segments are
/// kept: a config may point outside its package in a monorepo.
fn join_refs(parts: &[String]) -> String {
    join(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}

fn join(parts: &[&str]) -> String {
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

    fn values(source: &str) -> Vec<WebpackEntry> {
        entries("webpack.config.js", source)
    }

    #[test]
    fn reads_string_array_and_object_entries() {
        assert_eq!(
            values("module.exports = { entry: './src/index.js' };\n"),
            vec![WebpackEntry {
                specifier: "src/index.js".into(),
                base: EntryBase::PackageDir
            }]
        );
        assert_eq!(
            values("module.exports = { entry: ['./src/a.js', './src/b.js'] };\n"),
            vec![
                WebpackEntry {
                    specifier: "src/a.js".into(),
                    base: EntryBase::PackageDir
                },
                WebpackEntry {
                    specifier: "src/b.js".into(),
                    base: EntryBase::PackageDir
                }
            ]
        );
        assert_eq!(
            values(
                "module.exports = { entry: { app: './src/app.js', admin: ['./src/admin.js'], vendor: { import: './src/vendor.js' } } };\n"
            ),
            vec![
                WebpackEntry {
                    specifier: "src/admin.js".into(),
                    base: EntryBase::PackageDir
                },
                WebpackEntry {
                    specifier: "src/app.js".into(),
                    base: EntryBase::PackageDir
                },
                WebpackEntry {
                    specifier: "src/vendor.js".into(),
                    base: EntryBase::PackageDir
                }
            ]
        );
    }

    #[test]
    fn applies_context_and_path_helpers() {
        let source = r#"
            const path = require("path");
            const source = path.resolve(__dirname, "src");
            module.exports = {
              context: source,
              entry: { app: ["./boot.js"] },
            };
        "#;
        assert_eq!(
            values(source),
            vec![WebpackEntry {
                specifier: "src/boot.js".into(),
                base: EntryBase::ConfigDir
            }]
        );

        let direct = r#"
            import { resolve } from "node:path";
            export default {
              entry: resolve(__dirname, "src/index.ts"),
            };
        "#;
        assert_eq!(
            values(direct),
            vec![WebpackEntry {
                specifier: "src/index.ts".into(),
                base: EntryBase::ConfigDir
            }]
        );
    }

    #[test]
    fn ignores_dynamic_and_absolute_entries() {
        assert!(values("module.exports = { entry: () => './src/index.js' };\n").is_empty());
        assert!(
            values("module.exports = { entry: process.env.ENTRY };\n").is_empty(),
            "an unresolved environment value is not a source entry"
        );
        assert!(
            values("module.exports = { entry: '/srv/app/index.js' };\n").is_empty(),
            "an absolute filesystem path cannot be anchored in the repository"
        );
    }

    #[test]
    fn recognizes_webpack_config_names() {
        assert!(is_config_path("webpack.config.js"));
        assert!(is_config_path("apps/web/webpack.config.ts"));
        assert!(is_config_path("webpack.prod.config.mjs"));
        assert!(is_config_path("config/webpack.client.js"));
        assert!(is_config_path("build/webpack.server.ts"));
        assert!(!is_config_path("src/webpack.config.example.ts"));
        assert!(
            entries(
                "config/webpack.paths.js",
                "module.exports = { root: __dirname, src: 'src' };\n"
            )
            .is_empty(),
            "a helper module in a config directory has no entry object"
        );
    }
}
