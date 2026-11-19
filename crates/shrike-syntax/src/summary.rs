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
    pub exported: bool,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum DynamicSpecifier {
    /// `import('./a')`
    Literal(String),
    /// `` import(`./locales/${lang}.json`) ``: a static prefix to glob.
    TemplatePrefix(String),
    /// `import(someVariable)`
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicRecord {
    pub kind: DynamicKind,
    pub specifier: DynamicSpecifier,
    pub span: Span,
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
    pub imports: Vec<ImportRecord>,
    pub declarations: Vec<DeclarationRecord>,
    pub local_exports: Vec<LocalExportRecord>,
    pub re_exports: Vec<ReExportRecord>,
    pub export_alls: Vec<ExportAllRecord>,
    pub module_refs: ModuleRefs,
    pub dynamic: Vec<DynamicRecord>,
    pub unknown: Vec<UnknownRecord>,
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
