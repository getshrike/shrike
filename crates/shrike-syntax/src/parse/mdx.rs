//! Shallow MDX analysis.
//!
//! MDX files are Markdown documents with top-level ESM declarations. A full
//! MDX parser would need a Markdown AST, a JSX transform, and a component
//! scope model. For reachability we only need the ESM boundary: which modules
//! an MDX file imports and which names it exports. Those statements are plain
//! ESM, so we blank every non-ESM byte and hand the result to the normal
//! parser. Byte offsets and line numbers are preserved, so spans remain exact.

use shrike_core::{Diagnostic, LineIndex};

use crate::summary::{FileSummary, ImportKind, ImportRecord, ModuleRefs, ParseStatus};

/// Blank every line that is not part of a top-level `import`/`export`
/// statement, preserving byte offsets, newlines, and line numbers.
#[must_use]
pub(super) fn esm_source(source: &str) -> String {
    let ranges = line_ranges(source);
    let mut esm = vec![false; ranges.len()];
    let mut index = 0;
    // YAML frontmatter is metadata, not ESM.
    if ranges
        .first()
        .is_some_and(|(start, end)| source[*start..*end].trim() == "---")
    {
        index = 1;
        while index < ranges.len() && source[ranges[index].0..ranges[index].1].trim() != "---" {
            index += 1;
        }
        if index < ranges.len() {
            index += 1;
        }
    }
    let mut fence: Option<&str> = None;
    while index < ranges.len() {
        let line = source[ranges[index].0..ranges[index].1].trim_start();
        if let Some(marker) = fence {
            if line.starts_with(marker) {
                fence = None;
            }
            index += 1;
            continue;
        }
        if line.starts_with("```") {
            fence = Some("```");
            index += 1;
            continue;
        }
        if line.starts_with("~~~") {
            fence = Some("~~~");
            index += 1;
            continue;
        }
        let raw = &source[ranges[index].0..ranges[index].1];
        if !(raw.starts_with("import ") || raw.starts_with("export ")) {
            index += 1;
            continue;
        }
        let end = statement_end(source, &ranges, index);
        for slot in esm.iter_mut().take(end + 1).skip(index) {
            *slot = true;
        }
        index = end + 1;
    }

    let mut bytes = source.as_bytes().to_vec();
    for (line, (start, end)) in ranges.iter().enumerate() {
        if esm[line] {
            continue;
        }
        for byte in &mut bytes[*start..*end] {
            if *byte != b'\r' && *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(bytes).expect("blanking bytes preserves UTF-8")
}

/// Inclusive byte ranges for each line, excluding the line terminator.
fn line_ranges(source: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for segment in source.split_inclusive('\n') {
        let start = offset;
        let mut end = offset + segment.len();
        if segment.ends_with('\n') {
            end -= 1;
        }
        if end > start && source.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        ranges.push((start, end));
        offset += segment.len();
    }
    ranges
}

/// Find the last line of the ESM statement that starts at `start`.
fn statement_end(source: &str, ranges: &[(usize, usize)], start: usize) -> usize {
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut block_comment = false;
    let mut index = start;
    while index < ranges.len() {
        let line = &source[ranges[index].0..ranges[index].1];
        let mut chars = line.bytes().peekable();
        while let Some(byte) = chars.next() {
            if block_comment {
                if byte == b'*' && chars.peek() == Some(&b'/') {
                    chars.next();
                    block_comment = false;
                }
                continue;
            }
            if let Some(active) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == active {
                    quote = None;
                }
                continue;
            }
            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'/' if chars.peek() == Some(&b'/') => break,
                b'/' if chars.peek() == Some(&b'*') => {
                    chars.next();
                    block_comment = true;
                }
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
        if depth == 0 && !block_comment {
            let trimmed = line.trim_end();
            let terminal = trimmed.ends_with(';')
                || trimmed.ends_with('}')
                || trimmed.ends_with(')')
                || trimmed.ends_with(']')
                || trimmed.ends_with('"')
                || trimmed.ends_with('\'');
            if terminal {
                return index;
            }
            if index == start {
                // `import "pkg"` and single-line declarations without a
                // semicolon are complete when the next line starts a new block.
                if trimmed.starts_with("import \"") || trimmed.starts_with("import '") {
                    return index;
                }
            }
            if index > start && next_line_ends_statement(source, ranges, index + 1) {
                return index;
            }
            if index == start && next_line_ends_statement(source, ranges, index + 1) {
                return index;
            }
        }
        index += 1;
    }
    ranges.len().saturating_sub(1)
}

fn next_line_ends_statement(source: &str, ranges: &[(usize, usize)], next: usize) -> bool {
    let Some((start, end)) = ranges.get(next) else {
        return true;
    };
    let line = &source[*start..*end];
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed.starts_with('<')
        || trimmed.starts_with('#')
        || trimmed.starts_with("import ")
        || trimmed.starts_with("export ")
        || (!line.starts_with(' ') && !line.starts_with('\t') && !trimmed.ends_with(','))
}

/// Build a summary for an MDX file from its ESM boundary.
pub(super) fn parse(
    path: &str,
    source: &str,
    hash: String,
    line_index: &LineIndex,
    line_count: u32,
    generated: bool,
) -> FileSummary {
    let synthetic = esm_source(source);
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(
        &allocator,
        &synthetic,
        oxc_span::SourceType::mjs().with_jsx(true),
    )
    .parse();

    let mut summary = FileSummary {
        path: path.to_string(),
        content_hash: hash,
        module_kind: "mdx".to_string(),
        parse_status: ParseStatus::Ok,
        parse_message: None,
        line_count,
        directives: Vec::new(),
        server_action_module: false,
        inline_server_action_exports: Vec::new(),
        invalid_server_exports: Vec::new(),
        misplaced_directives: Vec::new(),
        imports: Vec::new(),
        declarations: Vec::new(),
        local_exports: Vec::new(),
        re_exports: Vec::new(),
        export_alls: Vec::new(),
        module_refs: ModuleRefs::default(),
        type_refs: Vec::new(),
        type_imports: Vec::new(),
        ambient_modules: Vec::new(),
        dynamic: Vec::new(),
        client_only_dynamic_specifiers: Vec::new(),
        unknown: Vec::new(),
        generated,
        diagnostics: Vec::new(),
    };

    if parsed.errors.is_empty() {
        for statement in &parsed.program.body {
            super::statements::extract_statement(statement, &synthetic, line_index, &mut summary);
        }
    } else {
        // The synthetic ESM boundary is normally well-formed. If a construct
        // defeats the extractor, keep the import sources so package usage and
        // module reachability stay correct; named bindings are left to the
        // conservative side.
        summary.diagnostics.push(
            Diagnostic::warning(
                "mdx_partial_parse",
                "MDX ESM boundary could not be parsed; only import sources were recovered",
            )
            .in_file(path),
        );
        summary.imports = fallback_imports(source, line_index);
    }

    super::refs::mark_local_exports(&mut summary);
    // The Markdown body is opaque. Every declaration in the file may be
    // referenced as a JSX component or expression, so treat the file itself
    // as referencing them. This can hide a genuinely unused local component
    // in MDX, but it never proposes deleting one that is rendered.
    let names: Vec<String> = summary
        .declarations
        .iter()
        .flat_map(|declaration| {
            let mut names = declaration.names.clone();
            names.push(declaration.name.clone());
            names
        })
        .chain(
            summary
                .local_exports
                .iter()
                .map(|export| export.local.clone()),
        )
        .collect();
    summary.module_refs.identifiers.extend(names);
    super::refs::finalize_refs(&mut summary);
    summary
}

fn fallback_imports(source: &str, line_index: &LineIndex) -> Vec<ImportRecord> {
    let mut imports = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
            if let Some(specifier) =
                quoted_specifier(trimmed).or_else(|| side_effect_specifier(trimmed))
            {
                let start = offset + indent;
                let end = start + trimmed.trim_end().len();
                imports.push(ImportRecord {
                    source: specifier,
                    bindings: Vec::new(),
                    type_only: false,
                    span: line_index.span(
                        source,
                        u32::try_from(start).unwrap_or(0),
                        u32::try_from(end.min(source.len())).unwrap_or(0),
                    ),
                    kind: ImportKind::Static,
                    side_effect_only: true,
                });
            }
        }
        offset += line.len();
    }
    imports
}

fn quoted_specifier(text: &str) -> Option<String> {
    let from = text.find(" from ").map(|index| index + 6)?;
    quoted_at(&text[from..])
}

/// Recover `import "pkg"` when the synthetic boundary cannot be parsed.
fn side_effect_specifier(text: &str) -> Option<String> {
    let rest = text.strip_prefix("import")?.trim_start();
    quoted_at(rest)
}

fn quoted_at(rest: &str) -> Option<String> {
    let quote = rest.as_bytes().first().copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let end = rest[1..].find(char::from(quote))?;
    Some(rest[1..1 + end].to_string())
}
