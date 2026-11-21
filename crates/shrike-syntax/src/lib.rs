#![forbid(unsafe_code)]
//! Syntax and per-file semantic summaries.
//!
//! Parsing is done with `oxc_parser`. The AST is dropped immediately after the
//! summary is built: only names, spans, directives, imports, exports, and
//! reference facts survive. No source text is retained beyond names and spans.

pub mod parse;
pub mod summary;

pub use parse::{parse_file, parse_source};
pub use summary::{
    BindingKind, DeclarationKind, DeclarationRecord, DynamicKind, DynamicRecord, DynamicSpecifier,
    ExportAllRecord, ExportedConstValue, ExportedConstValueRecord, FileSummary, ImportBinding,
    ImportKind, ImportRecord, LocalExportRecord, MemberAccess, ModuleRefs, NextSegmentExportRecord,
    ParseStatus, ReExportRecord, UnknownRecord, UnknownScope,
};
