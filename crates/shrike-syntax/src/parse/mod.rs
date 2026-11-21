//! `oxc_parser` integration and summary extraction.
//!
//! Rule B2: semantics come from the AST, never from a text scan. The AST is
//! dropped at the end of this function; only the summary survives.

use oxc_parser::Parser;
use oxc_span::SourceType;
use shrike_core::{Diagnostic, LineIndex, content_hash};

use crate::summary::{FileSummary, ModuleRefs, ParseStatus};

mod collector;
mod declarations;
mod html;
mod mdx;
mod next_dynamic;
mod next_segment;
mod refs;
mod server_actions;
mod statements;
mod type_refs;

#[cfg(test)]
mod tests;

/// Parse a file from disk. I/O errors are reported as a skipped summary.
#[must_use]
pub fn parse_file(path: &str, source: &str) -> FileSummary {
    parse_source(path, source)
}

/// Parse source text and build a deterministic summary.
#[must_use]
pub fn parse_source(path: &str, source: &str) -> FileSummary {
    let hash = content_hash(source);
    let module_kind = module_kind(path).to_string();
    let line_index = LineIndex::new(source);
    let line_count = line_index.line_count();
    let generated = looks_generated(source);

    if path.to_ascii_lowercase().ends_with(".html") {
        return html::parse(path, source, hash, &line_index, line_count, generated);
    }

    if path.to_ascii_lowercase().ends_with(".mdx") || path.to_ascii_lowercase().ends_with(".md") {
        return mdx::parse(path, source, hash, &line_index, line_count, generated);
    }

    let source_type = match SourceType::from_path(path) {
        Ok(source_type) => {
            let lower = path.to_ascii_lowercase();
            if lower.ends_with(".js") {
                // Modern `.js` files are ESM and commonly contain JSX
                // (Next.js, Gatsby, Create React App). Treat them as modules so
                // top-level await and import/export parse correctly.
                SourceType::mjs().with_jsx(true)
            } else if lower.ends_with(".mjs") || lower.ends_with(".cjs") {
                source_type.with_jsx(true)
            } else {
                source_type
            }
        }
        Err(_) => {
            let component = matches!(
                path.rsplit_once('.').map(|(_, extension)| extension),
                Some("vue" | "svelte" | "astro")
            );
            let diagnostics = if component {
                vec![
                    Diagnostic::info(
                        "framework_component",
                        "framework component body is not parsed; it is never a deletion candidate",
                    )
                    .in_file(path),
                ]
            } else {
                vec![
                    Diagnostic::warning(
                        "unsupported_extension",
                        "file is not a supported JavaScript or TypeScript source",
                    )
                    .in_file(path),
                ]
            };
            return FileSummary {
                path: path.to_string(),
                content_hash: hash,
                module_kind,
                parse_status: ParseStatus::Skipped,
                parse_message: Some("unsupported source extension".to_string()),
                line_count,
                directives: Vec::new(),
                server_action_module: false,
                inline_server_action_exports: Vec::new(),
                invalid_server_exports: Vec::new(),
                next_segment_exports: Vec::new(),
                exported_const_values: Vec::new(),
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
                diagnostics,
            };
        }
    };

    let allocator = oxc_allocator::Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        let message = parsed
            .errors
            .first()
            .map_or_else(|| "parse error".to_string(), ToString::to_string);
        let message = message.lines().next().unwrap_or("parse error").to_string();
        return FileSummary {
            path: path.to_string(),
            content_hash: hash,
            module_kind,
            parse_status: ParseStatus::Error,
            parse_message: Some(message.clone()),
            line_count,
            directives: Vec::new(),
            server_action_module: false,
            inline_server_action_exports: Vec::new(),
            invalid_server_exports: Vec::new(),
            next_segment_exports: Vec::new(),
            exported_const_values: Vec::new(),
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
            diagnostics: vec![Diagnostic::error("parse_error", message).in_file(path)],
        };
    }

    let program = parsed.program;
    let directives: Vec<String> = program
        .directives
        .iter()
        .map(|directive| directive.directive.to_string())
        .collect();

    let mut summary = FileSummary {
        path: path.to_string(),
        content_hash: hash,
        module_kind,
        parse_status: ParseStatus::Ok,
        parse_message: None,
        line_count,
        directives,
        server_action_module: false,
        inline_server_action_exports: Vec::new(),
        invalid_server_exports: Vec::new(),
        next_segment_exports: Vec::new(),
        exported_const_values: Vec::new(),
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

    for statement in &program.body {
        statements::extract_statement(statement, source, &line_index, &mut summary);
    }
    summary.server_action_module = server_actions::is_server_action_module(&program);
    summary.inline_server_action_exports = server_actions::inline_server_action_exports(&program);
    summary.invalid_server_exports =
        server_actions::invalid_server_exports(&program, source, &line_index);
    summary.next_segment_exports =
        next_segment::next_segment_exports(&program, source, &line_index);
    summary.exported_const_values =
        next_segment::exported_const_values(&program, source, &line_index);
    summary.client_only_dynamic_specifiers =
        next_dynamic::client_only_dynamic_specifiers(&program, &summary.imports);
    next_dynamic::assign_dynamic_spans(&program, source, &line_index, &mut summary);

    let type_refs = type_refs::collect(&program, source);
    summary.type_refs = type_refs.modules;
    summary.type_imports = type_refs.imports;
    refs::narrow_dynamic_bindings(&mut summary);
    refs::mark_local_exports(&mut summary);
    refs::finalize_refs(&mut summary);
    summary
}

fn module_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".d.ts") {
        "dts"
    } else if lower.ends_with(".tsx") {
        "tsx"
    } else if lower.ends_with(".ts") {
        "ts"
    } else if lower.ends_with(".jsx") {
        "jsx"
    } else if lower.ends_with(".mjs") {
        "mjs"
    } else if lower.ends_with(".cjs") {
        "cjs"
    } else if lower.ends_with(".mdx") || lower.ends_with(".md") {
        // `pageExtensions` can make plain Markdown routable. The ESM boundary
        // is identical, so reuse the MDX module kind rather than widening the
        // snapshot schema for a second Markdown spelling.
        "mdx"
    } else if lower.ends_with(".vue") {
        "vue"
    } else if lower.ends_with(".svelte") {
        "svelte"
    } else if lower.ends_with(".astro") {
        "astro"
    } else if lower.ends_with(".html") {
        "html"
    } else {
        "js"
    }
}

fn looks_generated(source: &str) -> bool {
    let head: String = source.chars().take(4096).collect();
    let lower = head.to_ascii_lowercase();
    lower.contains("@generated")
        || lower.contains("do not edit")
        || lower.contains("auto-generated")
        || lower.contains("autogenerated")
        || lower.contains("this file is generated")
}
