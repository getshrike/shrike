//! Byte spans and a line index.
//!
//! Spans are byte offsets into UTF-8 source text. Line and column are 1-based;
//! columns count Unicode scalar values, not bytes.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub column: u32,
}

impl Span {
    #[must_use]
    pub const fn new(start: u32, end: u32, line: u32, column: u32) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

/// Precomputed line starts for a source file.
#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push((offset + 1) as u32);
            }
        }
        Self { starts }
    }

    /// Number of lines in the file (at least one).
    #[must_use]
    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }

    /// Byte offset of the start of a 1-based line.
    #[must_use]
    pub fn line_start(&self, line: u32) -> u32 {
        if line == 0 {
            return 0;
        }
        self.starts
            .get((line - 1) as usize)
            .copied()
            .unwrap_or_else(|| self.starts.last().copied().unwrap_or(0))
    }

    /// Byte offset just past the newline of a 1-based line. For the last line
    /// without a trailing newline this is the end of the source.
    #[must_use]
    pub fn line_end(&self, line: u32, source_len: u32) -> u32 {
        if line == 0 {
            return 0;
        }
        let next = self.starts.get(line as usize).copied();
        next.unwrap_or(source_len)
    }

    /// Build a span for a byte range. Columns are counted in characters.
    #[must_use]
    pub fn span(&self, source: &str, start: u32, end: u32) -> Span {
        let start = start.min(source.len() as u32);
        let end = end.min(source.len() as u32).max(start);
        let line = self.line_of(start);
        let line_start = self.line_start(line);
        let column = source
            .get(line_start as usize..start as usize)
            .map_or(1, |prefix| prefix.chars().count() as u32 + 1);
        Span::new(start, end, line, column)
    }

    /// 1-based line containing a byte offset.
    #[must_use]
    pub fn line_of(&self, offset: u32) -> u32 {
        match self.starts.binary_search(&offset) {
            Ok(index) => index as u32 + 1,
            Err(index) => index.max(1) as u32,
        }
    }

    /// Expand a byte range to complete lines, preserving the trailing newline.
    #[must_use]
    pub fn expand_to_lines(&self, start: u32, end: u32, source_len: u32) -> (u32, u32) {
        let start_line = self.line_of(start);
        let end_line = self.line_of(end.saturating_sub(1).max(start));
        (
            self.line_start(start_line),
            self.line_end(end_line, source_len),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_use_character_columns() {
        let source = "const π = 1;\nexport const x = π;\n";
        let index = LineIndex::new(source);
        let start = source.find("x =").unwrap() as u32;
        let span = index.span(source, start, start + 1);
        assert_eq!(span.line, 2);
        assert_eq!(span.column, 14);
    }

    #[test]
    fn expands_to_full_lines() {
        let source = "a\nb\nc\n";
        let index = LineIndex::new(source);
        let (start, end) = index.expand_to_lines(2, 3, source.len() as u32);
        assert_eq!(&source[start as usize..end as usize], "b\n");
    }

    #[test]
    fn line_index_handles_multibyte_text() {
        let source = "// héllo\nconst value = 1;\n";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_of(10), 2);
        let span = index.span(source, 10, 15);
        assert_eq!(span.line, 2);
        assert_eq!(span.column, 1);
    }
}
