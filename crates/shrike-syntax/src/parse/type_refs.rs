//! Type-position module references.
//!
//! Dependency evidence and symbol liveness are related but distinct:
//! `import("./types").Config` proves that `types` is a dependency, and it also
//! uses the `Config` export. Oxc records TypeScript import types in the AST;
//! JSDoc comments are not in the AST, so their `import("pkg").Name` tags are
//! scanned separately.

use oxc_ast::ast::{Program, TSImportType, TSImportTypeQualifier, TSLiteral, TSType};
use oxc_ast_visit::{Visit, walk};

use crate::summary::TypeImportRecord;

pub(super) struct CollectedTypeRefs {
    pub modules: Vec<String>,
    pub imports: Vec<TypeImportRecord>,
}

pub(super) fn collect(program: &Program<'_>, source: &str) -> CollectedTypeRefs {
    let mut collector = TypeRefCollector {
        modules: Vec::new(),
        imports: Vec::new(),
    };
    collector.visit_program(program);
    jsdoc_refs(source, &mut collector);
    collector.modules.sort();
    collector.modules.dedup();
    collector.imports.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.imported.cmp(&b.imported))
    });
    collector.imports.dedup();
    CollectedTypeRefs {
        modules: collector.modules,
        imports: collector.imports,
    }
}

struct TypeRefCollector {
    modules: Vec<String>,
    imports: Vec<TypeImportRecord>,
}

impl<'a> Visit<'a> for TypeRefCollector {
    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        if let TSType::TSLiteralType(literal) = &it.argument {
            if let TSLiteral::StringLiteral(value) = &literal.literal {
                let source = value.value.to_string();
                let imported = it.qualifier.as_ref().and_then(qualifier_root);
                self.modules.push(source.clone());
                self.imports.push(TypeImportRecord { source, imported });
            }
        }
        walk::walk_ts_import_type(self, it);
    }
}

/// The leftmost qualifier is the module export; any further segments are
/// properties of that export.
fn qualifier_root(qualifier: &TSImportTypeQualifier<'_>) -> Option<String> {
    match qualifier {
        TSImportTypeQualifier::Identifier(identifier) => Some(identifier.name.to_string()),
        TSImportTypeQualifier::QualifiedName(name) => qualifier_root(&name.left),
    }
}

/// Extract `import("pkg").Name` and `require("pkg").Name` from JSDoc blocks.
fn jsdoc_refs(source: &str, collector: &mut TypeRefCollector) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'/' && bytes[index + 1] == b'*' && bytes[index + 2] == b'*' {
            let start = index + 3;
            let mut end = start;
            while end + 1 < bytes.len() && !(bytes[end] == b'*' && bytes[end + 1] == b'/') {
                end += 1;
            }
            collect_from_comment(&source[start..end], collector);
            index = end + 2;
        } else {
            index += 1;
        }
    }
}

fn collect_from_comment(comment: &str, collector: &mut TypeRefCollector) {
    let bytes = comment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let keyword = if bytes[index..].starts_with(b"import") {
            "import"
        } else if bytes[index..].starts_with(b"require") {
            "require"
        } else {
            index += 1;
            continue;
        };
        index += keyword.len();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'(' {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let Some(quote) = bytes
            .get(index)
            .copied()
            .filter(|byte| *byte == b'"' || *byte == b'\'')
        else {
            continue;
        };
        index += 1;
        let start = index;
        while index < bytes.len() && bytes[index] != quote && bytes[index] != b'\n' {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != quote {
            continue;
        }
        let specifier = comment[start..index].trim();
        index += 1;
        if specifier.is_empty() {
            continue;
        }
        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        // JSDoc spells the import as a call: `import("./types").Config`.
        if bytes.get(cursor) == Some(&b')') {
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
        }
        let mut imported = None;
        if bytes.get(cursor) == Some(&b'.') {
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            let name_start = cursor;
            while cursor < bytes.len() && is_identifier_byte(bytes[cursor]) {
                cursor += 1;
            }
            if cursor > name_start {
                imported = Some(comment[name_start..cursor].to_string());
            }
        }
        collector.modules.push(specifier.to_string());
        collector.imports.push(TypeImportRecord {
            source: specifier.to_string(),
            imported,
        });
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}
