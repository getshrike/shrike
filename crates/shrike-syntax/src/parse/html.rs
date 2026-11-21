//! HTML `<script type="module" src>` extraction.
//!
//! The browser loads every module script of a page, so its `src` is a static
//! import that does not appear in any JavaScript file. The scanner is
//! deliberately small: it skips comments and script bodies so tag-like text in
//! JavaScript strings is not mistaken for a document element, and it records
//! only external module scripts. Inline module bodies are still not parsed;
//! they are left unparsed rather than guessed.

use shrike_core::LineIndex;

use crate::summary::{FileSummary, ImportKind, ImportRecord, ModuleRefs, ParseStatus};

/// Parse one HTML document into side-effect imports for its module scripts.
pub(crate) fn parse(
    path: &str,
    source: &str,
    hash: String,
    line_index: &LineIndex,
    line_count: u32,
    generated: bool,
) -> FileSummary {
    let imports = module_script_sources(source)
        .into_iter()
        .filter_map(|(start, end, specifier)| {
            let specifier = normalize_module_source(&specifier)?;
            Some(ImportRecord {
                source: specifier,
                bindings: Vec::new(),
                type_only: false,
                span: line_index.span(source, start, end),
                kind: ImportKind::Static,
                side_effect_only: true,
            })
        })
        .collect();
    FileSummary {
        path: path.to_string(),
        content_hash: hash,
        module_kind: "html".to_string(),
        parse_status: ParseStatus::Ok,
        parse_message: None,
        line_count,
        directives: Vec::new(),
        server_action_module: false,
        inline_server_action_exports: Vec::new(),
        misplaced_directives: Vec::new(),
        imports,
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
    }
}

/// The `src` values of `<script type="module" src=...>` elements, with the
/// byte range of each complete tag.
fn module_script_sources(source: &str) -> Vec<(u32, u32, String)> {
    let mut found = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if source[index..].starts_with("<!--") {
            index = source[index + 4..]
                .find("-->")
                .map_or(bytes.len(), |next| index + 4 + next + 3);
            continue;
        }
        if !starts_with_ignore_ascii_case(&source[index..], "<script") {
            index += char_len(source, index);
            continue;
        }
        let name_end = index + "<script".len();
        if !bytes
            .get(name_end)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
        {
            index += char_len(source, index);
            continue;
        }
        let tag_end = find_tag_end(source, name_end);
        let attributes = &source[name_end..tag_end];
        // Skip the body so a `"<script"` string inside an inline script cannot
        // be read as another element.
        let next = source[tag_end..]
            .to_ascii_lowercase()
            .find("</script")
            .map_or(tag_end + 1, |close| tag_end + close + "</script".len());
        if let Some(src) = module_script_src(attributes) {
            let end = u32::try_from((tag_end + 1).min(bytes.len())).unwrap_or(0);
            found.push((u32::try_from(index).unwrap_or(0), end, src.to_string()));
        }
        index = next.max(index + char_len(source, index));
    }
    found
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn char_len(source: &str, index: usize) -> usize {
    source[index..].chars().next().map_or(1, char::len_utf8)
}

/// Byte offset of the `>` that ends a tag, skipping quoted attribute values.
fn find_tag_end(source: &str, from: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = from;
    let mut quote = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(active) if byte == active => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return index,
            None => {}
        }
        index += 1;
    }
    bytes.len()
}

/// The `src` value when the tag is a module script. Attribute order and quote
/// style do not matter; a classic script or a tag without `src` is ignored.
fn module_script_src(attributes: &str) -> Option<&str> {
    let mut src = None;
    let mut is_module = false;
    let bytes = attributes.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] == b'/' {
            index += 1;
            continue;
        }
        let name_start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'=' | b'>' | b'/')
        {
            index += 1;
        }
        let name = &attributes[name_start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let mut value = "";
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if let Some(&quote) = bytes.get(index).filter(|byte| matches!(byte, b'"' | b'\'')) {
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                value = &attributes[value_start..index];
                if index < bytes.len() {
                    index += 1;
                }
            } else {
                let value_start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && bytes[index] != b'>'
                {
                    index += 1;
                }
                value = &attributes[value_start..index];
            }
        }
        if name.eq_ignore_ascii_case("type") && value.eq_ignore_ascii_case("module") {
            is_module = true;
        } else if name.eq_ignore_ascii_case("src") && !value.is_empty() {
            src = Some(value.trim());
        }
    }
    is_module.then_some(src).flatten()
}

/// Turn a document `src` into a resolver specifier. Paths without a scheme are
/// document-relative; a leading `/` stays Vite root-relative.
fn normalize_module_source(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with('#')
        || value.starts_with("data:")
        || value.starts_with("//")
        || value.contains("://")
        || value.contains('%')
    {
        return None;
    }
    let value = value.split(['?', '#']).next().unwrap_or(value).trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with('/') || value.starts_with('.') {
        Some(value.to_string())
    } else {
        Some(format!("./{value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shrike_core::content_hash;

    fn summary(source: &str, path: &str) -> FileSummary {
        let line_index = LineIndex::new(source);
        let line_count = line_index.line_count();
        parse(
            path,
            source,
            content_hash(source),
            &line_index,
            line_count,
            false,
        )
    }

    #[test]
    fn extracts_external_module_scripts() {
        let source = r#"
<!doctype html>
<html>
  <body>
    <script src="/src/classic.js"></script>
    <script type="module" src="/src/main.ts"></script>
    <script src='./src/other.tsx' type='module' async></script>
    <!-- <script type="module" src="/src/commented.ts"></script> -->
    <script type="module">
      import "./inline";
    </script>
  </body>
</html>
"#;
        let parsed = summary(source, "index.html");
        assert_eq!(parsed.module_kind, "html");
        assert_eq!(parsed.parse_status, ParseStatus::Ok);
        let sources: Vec<&str> = parsed
            .imports
            .iter()
            .map(|import| import.source.as_str())
            .collect();
        assert_eq!(sources, vec!["/src/main.ts", "./src/other.tsx"]);
        assert!(parsed.imports.iter().all(|import| import.side_effect_only));
    }

    #[test]
    fn ignores_external_and_template_sources() {
        let parsed = summary(
            r#"<script type="module" src="https://cdn.test/a.js"></script>
               <script type="module" src="%BASE_URL%/src/a.ts"></script>
               <script type="module" src="/src/b.ts?raw#x"></script>"#,
            "index.html",
        );
        let sources: Vec<&str> = parsed
            .imports
            .iter()
            .map(|import| import.source.as_str())
            .collect();
        assert_eq!(sources, vec!["/src/b.ts"]);
    }

    #[test]
    fn ignores_script_like_text_inside_inline_bodies() {
        let parsed = summary(
            r#"<script>const template = '<script type="module" src="/src/fake.ts"></script>';</script>
               <script type="module" src="/src/real.ts"></script>"#,
            "index.html",
        );
        let sources: Vec<&str> = parsed
            .imports
            .iter()
            .map(|import| import.source.as_str())
            .collect();
        assert_eq!(sources, vec!["/src/real.ts"]);
    }
}
