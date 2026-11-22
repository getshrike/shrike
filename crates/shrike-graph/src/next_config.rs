//! Literal scanning for Next.js configuration values that change the inventory.
//!
//! Next configuration is executable JavaScript, but the conventions that decide
//! which files exist as routes are literal arrays. This module reads only those
//! literals. Comments, strings, and unrelated identifiers are skipped, so a
//! commented-out `pageExtensions` cannot widen discovery.

use std::collections::BTreeSet;

const CONFIG_NAMES: &[&str] = &[
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "next.config.ts",
    "next.config.mts",
    "next.config.cts",
];

/// Whether a path is one of Next.js' supported configuration filenames.
#[must_use]
pub(crate) fn is_config_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    CONFIG_NAMES.contains(&file)
}

/// The literal `pageExtensions` values in a Next configuration source.
///
/// Multiple assignments are unioned. Non-literal elements are skipped rather
/// than guessed; an extension this scanner does not see is treated as disabled.
#[must_use]
pub(crate) fn page_extensions(source: &str) -> BTreeSet<String> {
    let bytes = source.as_bytes();
    let mut extensions = BTreeSet::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index);
            }
            b'\'' | b'"' | b'`' => index = skip_string(bytes, index),
            byte if is_identifier_start(byte) => {
                let start = index;
                index += 1;
                while index < bytes.len() && is_identifier_continue(bytes[index]) {
                    index += 1;
                }
                if &source[start..index] == "pageExtensions" {
                    if let Some((values, next)) = parse_value(source, index) {
                        extensions.extend(values);
                        index = next;
                    }
                }
            }
            _ => index += 1,
        }
    }
    extensions
}

fn parse_value(source: &str, from: usize) -> Option<(Vec<String>, usize)> {
    let bytes = source.as_bytes();
    let mut index = skip_trivia(source, from);
    if !matches!(bytes.get(index), Some(b':' | b'=')) {
        return None;
    }
    index = skip_trivia(source, index + 1);
    if bytes.get(index) != Some(&b'[') {
        return None;
    }
    index += 1;
    let mut values = Vec::new();
    loop {
        index = skip_trivia(source, index);
        match bytes.get(index) {
            None => return Some((values, index)),
            Some(b']') => return Some((values, index + 1)),
            Some(b',') => index += 1,
            Some(b'\'' | b'"' | b'`') => {
                if let Some((value, next)) = literal_string(source, index) {
                    if is_extension(&value) {
                        values.push(value.to_ascii_lowercase());
                    }
                    index = next;
                } else {
                    index = skip_element(source, index);
                }
            }
            Some(_) => index = skip_element(source, index),
        }
    }
}

fn skip_trivia(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'/') {
            index = skip_line_comment(bytes, index);
            continue;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'*') {
            index = skip_block_comment(bytes, index);
            continue;
        }
        return index;
    }
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

/// Read a quoted extension. Template literals with interpolation are dynamic
/// and therefore rejected.
fn literal_string(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let quote = *bytes.get(start)?;
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => {
                let value = &source[start + 1..index];
                if value.contains("${") {
                    return None;
                }
                return Some((value.to_string(), index + 1));
            }
            b'\n' | b'\r' => return None,
            _ => index += 1,
        }
    }
    None
}

/// Skip one array element up to the next top-level comma or closing bracket.
fn skip_element(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = start;
    let mut depth = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index);
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index);
                continue;
            }
            b'\'' | b'"' | b'`' => {
                index = skip_string(bytes, index);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b'}' => depth = depth.saturating_sub(1),
            b']' if depth == 0 => return index,
            b',' if depth == 0 => return index,
            _ => {}
        }
        index += 1;
    }
    index
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

/// A page extension may contain dots: `pageExtensions: ["page.tsx"]` makes
/// `app/page.page.tsx` a route. Each dot-separated segment must still be a
/// plain extension token, so a computed value or a path cannot be smuggled in.
fn is_extension(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_and_wrapped_arrays() {
        let source = r#"
            const nextConfig = { pageExtensions: ["js", "tsx", "md", "mdx"] };
            export default withMDX(nextConfig);
        "#;
        assert_eq!(
            page_extensions(source),
            BTreeSet::from([
                "js".to_string(),
                "md".to_string(),
                "mdx".to_string(),
                "tsx".to_string()
            ])
        );
        let nested =
            r#"module.exports = withSentryConfig({ pageExtensions: ['md'] }, { org: 'x' });"#;
        assert_eq!(page_extensions(nested), BTreeSet::from(["md".to_string()]));
    }

    #[test]
    fn ignores_comments_strings_and_unrelated_assignments() {
        let source = r#"
            // pageExtensions: ["md"]
            /* pageExtensions: ["mdx"] */
            const text = "pageExtensions: ['md']";
            const config = { pageExtensions: ["ts", "tsx"] };
        "#;
        assert_eq!(
            page_extensions(source),
            BTreeSet::from(["ts".to_string(), "tsx".to_string()])
        );
    }

    #[test]
    fn skips_dynamic_elements_but_keeps_literals() {
        let source = r#"
            const config = {
              pageExtensions: [base, "md", ...extra, `m${dynamic}`],
            };
        "#;
        assert_eq!(page_extensions(source), BTreeSet::from(["md".to_string()]));
    }

    #[test]
    fn reads_dotted_page_extensions() {
        let source =
            r#"const config = { pageExtensions: ["page.tsx", "route.mjs", "../bad", "."] };"#;
        assert_eq!(
            page_extensions(source),
            BTreeSet::from(["page.tsx".to_string(), "route.mjs".to_string()])
        );
    }

    #[test]
    fn recognizes_only_supported_config_filenames() {
        assert!(is_config_path("next.config.ts"));
        assert!(is_config_path("apps/web/next.config.mjs"));
        assert!(!is_config_path("src/next.config.example.ts"));
        assert!(!is_config_path("next.config.json"));
    }
}
