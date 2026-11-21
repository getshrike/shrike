//! Serializable per-file summaries.
//!
//! These records are the only syntax data retained after parsing. They are
//! deliberately small and contain no source text.

use serde::{Deserialize, Serialize};
use shrike_core::{Diagnostic, Span};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParseStatus {
    #[default]
    Ok,
    Error,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingKind {
    Named,
    Default,
    Namespace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportKind {
    Static,
    Require,
    ImportEquals,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBinding {
    /// The name in the exporting module (`default` for a default import).
    pub imported: String,
    /// The local alias bound in the importing module.
    pub local: String,
    pub kind: BindingKind,
    pub type_only: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub source: String,
    pub bindings: Vec<ImportBinding>,
    pub type_only: bool,
    pub span: Span,
    pub kind: ImportKind,
    pub side_effect_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeclarationKind {
    Function,
    Class,
    Variable,
    Type,
    Interface,
    Enum,
    Namespace,
    Default,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberAccess {
    pub object: String,
    /// `None` means a computed access with a non-literal key.
    pub property: Option<String>,
    pub computed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRefs {
    pub identifiers: Vec<String>,
    /// Identifier references that are not the object of a member access.
    ///
    /// A namespace import used this way (`const all = { ...UI }`, or a
    /// destructured `const { Button } = UI`) can reach any export, so the graph
    /// treats it as a module-scoped unknown instead of guessing a member.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub free: Vec<String>,
    pub members: Vec<MemberAccess>,
    pub jsx: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationRecord {
    pub name: String,
    /// All names bound by this declaration (a variable declaration may bind
    /// several names; they are treated as one deletion unit).
    pub names: Vec<String>,
    pub kind: DeclarationKind,
    /// Span of the complete statement, including an `export` keyword.
    pub span: Span,
    /// Whether the module exports this declaration, including through a
    /// separate `export { name }` statement. `mark_local_exports` sets this
    /// after parsing.
    pub exported: bool,
    /// Whether the declaration itself carries the `export` keyword. A local
    /// `export { name as other }` sets `exported` but not this flag, so callers
    /// can tell the declaration's own name apart from its exported alias.
    #[serde(default)]
    pub directly_exported: bool,
    pub default: bool,
    pub type_only: bool,
    pub r#async: bool,
    pub directives: Vec<String>,
    pub references: ModuleRefs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalExportRecord {
    pub local: String,
    pub exported: String,
    pub span: Span,
    pub statement_span: Span,
    pub type_only: bool,
}

/// A value export of a leading `"use server"` module that is definitely not a
/// function. Next.js fails the build when an action export is not callable, so
/// the parser records the exported name and the declaration or specifier span.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvalidServerExportRecord {
    pub name: String,
    pub span: Span,
}

/// A top-level export Next.js's app-route static-info analyzer counts when it
/// validates route segment configuration: `generateStaticParams`,
/// `unstable_paramMatching`, and `unstable_generateParamMatching`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextSegmentExportRecord {
    pub name: String,
    pub span: Span,
}

/// A directly exported `const` whose initializer Next.js can statically
/// extract for route segment configuration. Arrays and objects are recorded as
/// shapes; the conflict checks do not need their contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedConstValueRecord {
    pub name: String,
    pub value: ExportedConstValue,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum ExportedConstValue {
    String(String),
    Number(String),
    Boolean(bool),
    Null,
    Array,
    Object,
    RegExp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReExportRecord {
    pub imported: String,
    pub exported: String,
    pub source: String,
    pub span: Span,
    pub statement_span: Span,
    pub type_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAllRecord {
    pub source: String,
    /// `Some(name)` for `export * as name from ...`.
    pub exported: Option<String>,
    pub span: Span,
    pub type_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DynamicKind {
    Import,
    Require,
    /// A module loaded by URL at runtime rather than imported:
    /// `new Worker(new URL("./worker.ts", import.meta.url))`,
    /// `navigator.serviceWorker.register(...)`, `importScripts(...)`, or a
    /// worklet `addModule(...)`. The target is an entry module.
    Worker,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum DynamicSpecifier {
    /// `import('./a')`
    Literal(String),
    /// `` import(`./locales/${lang}.json`) ``: a static prefix to glob.
    TemplatePrefix(String),
    /// `import.meta.glob('./pages/*.tsx')` and `require.context('./icons')`.
    /// `patterns` and `excludes` are the literal glob patterns as written, with
    /// the leading `!` removed from excludes. The importing file's directory is
    /// the base for relative patterns.
    Glob {
        patterns: Vec<String>,
        excludes: Vec<String>,
    },
    /// `import(someVariable)`
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicRecord {
    pub kind: DynamicKind,
    pub specifier: DynamicSpecifier,
    pub span: Span,
    /// Export names proven by a static destructuring or member access at the
    /// import site, such as `const { getDb } = await import("./db")` or
    /// `(await import("./x")).default`. `None` means the namespace may be used
    /// in an unknown shape and every export must stay live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<Vec<String>>,
    /// Local `const` binding that receives the module namespace, such as
    /// `const ns = await import("./m")`. The parser narrows it after the whole
    /// module is visited: a binding used only through static members consumes
    /// exactly those exports; any bare, computed, exported, or otherwise
    /// escaping use leaves it `None` and widens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "package", rename_all = "camelCase")]
pub enum UnknownScope {
    Repo,
    Package(String),
    Module(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownRecord {
    pub reason: String,
    pub scope: UnknownScope,
    pub span: Span,
}

/// A type-position import that names a specific export, such as
/// `import("./types").Config` or JSDoc `@type {import("./types").Config}`.
/// The module specifier alone is dependency evidence; the qualifier keeps the
/// type declaration live.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeImportRecord {
    pub source: String,
    pub imported: Option<String>,
}

/// A `"use client"` or `"use server"` string that is not in the leading
/// prologue. RSC bundlers silently ignore it, so it does not create the
/// intended client or server boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MisplacedDirectiveRecord {
    pub directive: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub path: String,
    pub content_hash: String,
    pub module_kind: String,
    pub parse_status: ParseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_message: Option<String>,
    pub line_count: u32,
    pub directives: Vec<String>,
    /// Whether a leading `"use server"` module exports only async functions.
    /// The bundler replaces such a module with action references at the client
    /// boundary, so its imports never enter the client bundle.
    #[serde(default)]
    pub server_action_module: bool,
    /// Names of exports whose function body carries an inline `"use server"`
    /// directive. `"default"` represents a default-exported action.
    #[serde(default)]
    pub inline_server_action_exports: Vec<String>,
    /// Value exports of a leading `"use server"` module that are definitely not
    /// functions. Next.js rejects a non-callable action export at build time.
    #[serde(default)]
    pub invalid_server_exports: Vec<InvalidServerExportRecord>,
    /// Exports Next.js's app-route static-info analyzer recognizes for route
    /// segment configuration.
    #[serde(default)]
    pub next_segment_exports: Vec<NextSegmentExportRecord>,
    /// Directly exported `const` values that Next.js can extract for route
    /// segment configuration.
    #[serde(default)]
    pub exported_const_values: Vec<ExportedConstValueRecord>,
    /// Directive-looking strings after the prologue. Recorded for the
    /// `misplaced_directive` diagnostic; they are not honored as directives.
    #[serde(default)]
    pub misplaced_directives: Vec<MisplacedDirectiveRecord>,
    pub imports: Vec<ImportRecord>,
    pub declarations: Vec<DeclarationRecord>,
    pub local_exports: Vec<LocalExportRecord>,
    pub re_exports: Vec<ReExportRecord>,
    pub export_alls: Vec<ExportAllRecord>,
    pub module_refs: ModuleRefs,
    /// Module specifiers referenced from type positions: TypeScript
    /// `import("pkg")` types and JSDoc `@type {import("pkg").X}` tags. These
    /// are dependency evidence, not runtime graph edges.
    #[serde(default)]
    pub type_refs: Vec<String>,
    /// Type-position imports with an export qualifier. These create symbol
    /// liveness edges without pretending to be runtime imports.
    #[serde(default)]
    pub type_imports: Vec<TypeImportRecord>,
    pub dynamic: Vec<DynamicRecord>,
    /// Specifiers of `import()` calls inside
    /// `next/dynamic(() => import("./x"), { ssr: false })`.
    ///
    /// This is the sanctioned client-only escape hatch: the leak scan does not
    /// follow such an edge when the same specifier has no runtime static edge.
    #[serde(default)]
    pub client_only_dynamic_specifiers: Vec<String>,
    pub unknown: Vec<UnknownRecord>,
    /// `declare module "..."` and `declare global` names. Their bodies are
    /// type-level side effects and are always live once the file is compiled.
    #[serde(default)]
    pub ambient_modules: Vec<String>,
    pub generated: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl FileSummary {
    #[must_use]
    pub fn is_parsed(&self) -> bool {
        self.parse_status == ParseStatus::Ok
    }

    #[must_use]
    pub fn has_directive(&self, directive: &str) -> bool {
        self.directives.iter().any(|value| value == directive)
    }

    /// Names exported by this file, including local aliases and re-exports.
    #[must_use]
    pub fn exported_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for declaration in &self.declarations {
            if declaration.exported {
                for name in &declaration.names {
                    names.push(name.clone());
                }
                if declaration.default {
                    names.push("default".to_string());
                }
            }
        }
        for local in &self.local_exports {
            names.push(local.exported.clone());
        }
        for re_export in &self.re_exports {
            names.push(re_export.exported.clone());
        }
        for export_all in &self.export_alls {
            if let Some(name) = &export_all.exported {
                names.push(name.clone());
            }
        }
        names.sort();
        names.dedup();
        names
    }

    /// The local symbol that backs the default export, when one exists.
    #[must_use]
    pub fn default_export_name(&self) -> Option<String> {
        self.declarations
            .iter()
            .find(|declaration| declaration.default)
            .map(|declaration| declaration.name.clone())
    }
}
