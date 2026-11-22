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
    /// The access reads an instance member: `new C().x`, `C.prototype.x`, or
    /// a destructured property of a constructed instance. A plain `C.x`
    /// remains a static access.
    #[serde(default)]
    pub instance: bool,
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

/// A prop key that a Next.js Pages Router data function statically returns but
/// the page component never reads.
///
/// The rule is advisory: a returned key is not a symbol and removing it is a
/// manual edit, never a batch deletion candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDataKeyRecord {
    /// The data function that returned the key: `getStaticProps`,
    /// `getServerSideProps`, or `getInitialProps`.
    pub function: String,
    pub key: String,
    /// Span of the returned property key, used to anchor the diagnostic.
    pub span: Span,
}

/// Next.js Pages Router metadata that its build reads from the loaded page
/// module rather than from static route analysis.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PagesRouterInfo {
    /// The local binding that backs `export default`, when it has a name.
    pub default_export_local: Option<String>,
    /// Whether the default-exported component carries `getInitialProps`.
    pub default_export_has_get_initial_props: bool,
    /// The statically extracted `runtime` property of `export const config`.
    pub pages_config_runtime: Option<ExportedConstValueRecord>,
    /// Returned prop keys that the page component does not consume.
    pub unused_page_data_keys: Vec<PageDataKeyRecord>,
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

/// One prop of a local React component whose props are statically harvestable.
///
/// Two shapes are recorded: a prop destructured from an inline props object,
/// and a property of a same-file object type annotating a bare `props`
/// parameter. `used` is true when the destructured local is referenced anywhere
/// in the component's own declaration, or when the typed prop is read through
/// `props.<name>` / `const { <name> } = props` in the component body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPropRecord {
    pub component: String,
    /// The public prop name written in the destructure pattern or type.
    pub prop: String,
    /// The local binding, which differs from `prop` for `{ tone: shade }`. For
    /// typed props this is the `props` parameter name.
    pub local: String,
    /// Span of the destructured property or typed property signature, used to
    /// anchor the diagnostic.
    pub span: Span,
    pub used: bool,
    /// `true` for an inline destructure; `false` for a same-file typed props
    /// parameter.
    pub destructured: bool,
}

/// One optional prop of a React component contract. The absent-prop rule
/// reports an optional prop that every statically known render site omits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentContractProp {
    pub name: String,
    /// Span of the destructured property or typed property signature.
    pub span: Span,
    pub optional: bool,
    /// The prop is read in the component body. An unread prop is covered by
    /// `unused_component_prop` instead.
    pub used: bool,
    /// The prop has a destructuring default.
    #[serde(default)]
    pub has_default: bool,
}

/// A top-level React component whose optional props can be checked against
/// statically known JSX render sites.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentContractRecord {
    pub component: String,
    /// A value reference other than a direct export or a JSX tag reaches the
    /// component, so the set of render sites is open.
    pub escaped: bool,
    pub props: Vec<ComponentContractProp>,
}

/// A JSX opening element with a simple identifier tag. The graph resolves the
/// tag to a component contract and records the supplied prop names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsxUseRecord {
    pub component: String,
    pub span: Span,
    /// Statically named attributes. A spread or namespaced attribute sets
    /// `opaque`.
    pub supplied: Vec<String>,
    pub opaque: bool,
}

/// A JSX member expression root such as `UI` in `<UI.Card />`. A namespace
/// import used this way can render any export of its target file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsxNamespaceUseRecord {
    pub root: String,
    pub span: Span,
}

/// One member of a top-level TypeScript enum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumMemberRecord {
    pub name: String,
    /// Span of the member name, used to anchor the diagnostic.
    pub span: Span,
    /// Sibling members read by this member's initializer, such as `A` in
    /// `B = A`. These are real reads even when the declaring member is itself
    /// unused, so they must not be reported as dead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initializer_members: Vec<String>,
}

/// A top-level TypeScript enum whose members can be checked individually.
///
/// The graph reports a member that no project reference reads as an
/// `unused_enum_member` warning. The enum itself must be live; a dead enum is
/// already one deletion candidate, and reporting its members separately would
/// duplicate the finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumRecord {
    pub name: String,
    pub span: Span,
    /// `declare enum`: there is no runtime object to inspect.
    pub ambient: bool,
    /// A member name was computed or duplicated, so the enum abstains.
    #[serde(default)]
    pub opaque: bool,
    pub members: Vec<EnumMemberRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClassMemberKind {
    Method,
    Property,
    Getter,
    Setter,
}

/// One method or property of a top-level class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMemberRecord {
    pub name: String,
    /// Span of the member name, used to anchor the diagnostic.
    pub span: Span,
    pub kind: ClassMemberKind,
    pub is_static: bool,
    /// `#name` or TypeScript `private`.
    pub is_private: bool,
    /// TypeScript `protected`: subclasses may read it, so it is not reported.
    pub is_protected: bool,
    pub is_abstract: bool,
    pub is_declare: bool,
    /// TypeScript `override`: a base-class reference may call it.
    pub is_override: bool,
    pub decorated: bool,
}

/// A top-level class whose members can be checked individually.
///
/// The graph reports a member that no project reference reads as an
/// `unused_class_member` warning. The class itself must be live; a dead class
/// is already one deletion candidate. The parser sets `opaque` whenever the
/// class has a construct it cannot attribute confidently (computed names,
/// decorators, an unknown base class, an index signature, or an escaping
/// class/instance value), and the graph then abstains.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassRecord {
    pub name: String,
    pub span: Span,
    /// The class is a default export; its symbol key is `default`.
    pub default: bool,
    pub ambient: bool,
    pub abstract_: bool,
    pub decorated: bool,
    #[serde(default)]
    pub opaque: bool,
    /// A plain superclass name (`Base`) or a dotted member expression
    /// (`React.Component`), when the parser can read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Whether the superclass expression is a member expression.
    #[serde(default)]
    pub extends_member: bool,
    #[serde(default)]
    pub implements: Vec<String>,
    pub members: Vec<ClassMemberRecord>,
    /// `this.x` reads in instance context.
    #[serde(default)]
    pub self_instance_reads: Vec<String>,
    /// `this.x` reads in a static method, property initializer, or static block.
    #[serde(default)]
    pub self_static_reads: Vec<String>,
    /// `super.x` reads; these credit the same name on the class or its base.
    #[serde(default)]
    pub super_reads: Vec<String>,
    /// `ClassName.x` reads inside the class body.
    #[serde(default)]
    pub class_static_reads: Vec<String>,
}

/// One exported member of a top-level TypeScript namespace. Nested namespace
/// members use the dotted path from the exported root (`Inner.deep`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceMemberRecord {
    pub name: String,
    /// Span of the member name, used to anchor the diagnostic.
    pub span: Span,
}

/// A top-level TypeScript namespace whose exported members can be checked
/// individually.
///
/// The graph reports a member that no project reference reads as an
/// `unused_namespace_member` warning. The namespace itself must be live and
/// exported; a dead namespace is already one deletion candidate. Ambient
/// namespaces are included when they are exported from a `.ts` file, because
/// their type members are still part of the module surface; declaration files
/// abstain entirely.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceRecord {
    pub name: String,
    pub span: Span,
    /// `declare namespace`: there is no runtime object, but its exported type
    /// members are still readable by consumers.
    pub ambient: bool,
    pub members: Vec<NamespaceMemberRecord>,
}

/// A read of a namespace member, including the full dotted chain that the
/// ordinary `MemberAccess` record loses for nested accesses such as
/// `Outer.Inner.deep`.
///
/// `object` is a local identifier or an inline import source spelled as
/// `import("./module")`. `path` is the member path after that root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceMemberUseRecord {
    pub object: String,
    pub path: String,
}

/// A local binding that holds an instance of a class.
///
/// `const counter = new Counter()` and `function use(shape: Shape)` both bind
/// an instance. Member accesses through the binding credit the class. `escaped`
/// is set when the binding is used as a whole value (passed, returned, spread,
/// or reassigned), because the receiving code can read any member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassBindingRecord {
    pub binding: String,
    pub class: String,
    pub span: Span,
    #[serde(default)]
    pub escaped: bool,
    /// The binding came from `new Class(...)` rather than a type annotation.
    #[serde(default)]
    pub via_new: bool,
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
    /// The local binding that backs the default export, when the default
    /// export is a named declaration, an identifier, or `export { X as default }`.
    #[serde(default)]
    pub default_export_local: Option<String>,
    /// Whether the default-exported component carries `getInitialProps`, as
    /// `Page.getInitialProps = ...` or a class static member. Next.js reads the
    /// property from the loaded page component.
    #[serde(default)]
    pub default_export_has_get_initial_props: bool,
    /// The `runtime` property of a Pages Router `export const config = {...}`,
    /// when Next.js can statically extract it.
    #[serde(default)]
    pub pages_config_runtime: Option<ExportedConstValueRecord>,
    /// Returned prop keys of a Pages Router data function that the page
    /// component never reads. The graph reports these as
    /// `unused_page_data_key` warnings only for live Pages Router pages.
    #[serde(default)]
    pub unused_page_data_keys: Vec<PageDataKeyRecord>,
    /// Directive-looking strings after the prologue. Recorded for the
    /// `misplaced_directive` diagnostic; they are not honored as directives.
    #[serde(default)]
    pub misplaced_directives: Vec<MisplacedDirectiveRecord>,
    /// Statically harvested props of local (non-exported) React components.
    /// The graph reports a prop whose local binding is never read as
    /// `unused_component_prop`; this is a warning, never a deletion candidate.
    #[serde(default)]
    pub component_props: Vec<ComponentPropRecord>,
    /// Top-level React component contracts, including exported components.
    /// The graph reports an optional prop that every statically known render
    /// site omits as an `absent_component_prop` warning.
    #[serde(default)]
    pub component_contracts: Vec<ComponentContractRecord>,
    /// JSX opening elements with simple identifier tags. The graph resolves
    /// each tag to a component contract and records the supplied props.
    #[serde(default)]
    pub jsx_uses: Vec<JsxUseRecord>,
    /// Roots of JSX member expressions such as `UI` in `<UI.Card />`.
    #[serde(default)]
    pub jsx_namespace_uses: Vec<JsxNamespaceUseRecord>,
    /// Identifier references outside JSX tag names and direct exports. A
    /// component used as a value (`memo(Card)`, `{ Card }`, `const Alias =
    /// Card`) can be rendered by a consumer this analysis cannot enumerate, so
    /// the graph blocks it from the absent-prop rule.
    #[serde(default)]
    pub component_value_uses: Vec<String>,
    /// Top-level TypeScript enums whose members can be checked individually.
    /// The graph reports a member that no project reference reads as an
    /// `unused_enum_member` warning; member removal is a manual review edit,
    /// never part of a batch deletion.
    #[serde(default)]
    pub enums: Vec<EnumRecord>,
    /// Top-level classes whose members can be checked individually. The graph
    /// reports a member that no project reference reads as an
    /// `unused_class_member` warning; member removal is a manual review edit,
    /// never part of a batch deletion.
    #[serde(default)]
    pub classes: Vec<ClassRecord>,
    /// Local bindings that hold class instances. Member accesses through a
    /// binding credit the class's instance members.
    #[serde(default)]
    pub class_bindings: Vec<ClassBindingRecord>,
    /// Identifier names used as whole runtime values rather than in a safe
    /// class position (`new`, `extends`, a static member object, or a type).
    /// An imported class named here can expose any member to the receiver, so
    /// the graph abstains on its members.
    #[serde(default)]
    pub class_value_escapes: Vec<String>,
    /// Top-level TypeScript namespaces whose exported members can be checked
    /// individually. The graph reports a member that no project reference
    /// reads as an `unused_namespace_member` warning; member removal is a
    /// manual review edit, never part of a batch deletion.
    #[serde(default)]
    pub namespaces: Vec<NamespaceRecord>,
    /// Full dotted namespace member reads. The ordinary `MemberAccess` record
    /// keeps only the first property of `Outer.Inner.deep`; this list keeps the
    /// complete path so a nested member can be credited precisely.
    #[serde(default)]
    pub namespace_member_uses: Vec<NamespaceMemberUseRecord>,
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
