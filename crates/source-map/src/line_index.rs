//! Line index for efficient offset ↔ line/column conversion.

use crate::ByteOffset;
use text_size::TextSize;

/// A line and column position (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LineCol {
    /// 0-indexed line number.
    pub line: u32,
    /// 0-indexed column in the coordinate system used by the producing API.
    pub col: u32,
}

impl LineCol {
    /// Creates a new line/column position.
    #[inline]
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

/// An index for efficient conversion between byte offsets and line/column positions.
///
/// The index stores the byte offset of the start of each line, enabling O(log n)
/// lookups in both directions.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the start of each line.
    /// `line_starts[i]` is the offset where line `i` begins.
    line_starts: Vec<ByteOffset>,
    /// Byte width of the line terminator following each line (0 for the last).
    line_break_widths: Vec<u8>,
    /// Encoding metadata only for lines containing non-ASCII characters.
    /// ASCII lines derive every column representation from `line_starts`.
    encoded_lines: Vec<EncodedLine>,
    /// Total source length in bytes.
    text_len: ByteOffset,
}

#[derive(Debug, Clone)]
struct EncodedLine {
    line: u32,
    utf16_len: u32,
    checkpoints: Vec<ColumnCheckpoint>,
}

#[derive(Debug, Clone, Copy)]
struct ColumnCheckpoint {
    byte_start: u32,
    byte_end: u32,
    utf16_start: u32,
    utf16_end: u32,
}

impl LineIndex {
    /// Creates an LF-delimited line index for Svelte and v3 source-map
    /// coordinates.
    pub fn new(text: &str) -> Self {
        Self::new_with_javascript_line_terminators(text, false)
    }

    /// Creates an index using the full JavaScript/TypeScript line-terminator
    /// set (LF, CRLF, CR, U+2028, and U+2029).
    pub fn new_typescript(text: &str) -> Self {
        Self::new_with_javascript_line_terminators(text, true)
    }

    fn new_with_javascript_line_terminators(text: &str, javascript: bool) -> Self {
        let mut line_starts = vec![TextSize::from(0)];
        let mut line_break_widths = Vec::new();
        let mut encoded_lines = Vec::new();
        let mut current_checkpoints = Vec::new();
        let mut line = 0u32;
        let mut byte_col = 0u32;
        let mut utf16_col = 0u32;
        let mut chars = text.char_indices().peekable();
        while let Some((offset, c)) = chars.next() {
            let line_break_width = match c {
                '\r' if chars.peek().is_some_and(|(_, next)| *next == '\n') => {
                    javascript.then(|| {
                        chars.next();
                        2
                    })
                }
                '\n' => Some(1),
                '\r' if javascript => Some(1),
                '\u{2028}' | '\u{2029}' if javascript => Some(c.len_utf8() as u8),
                _ => None,
            };
            if let Some(line_break_width) = line_break_width {
                if !current_checkpoints.is_empty() {
                    encoded_lines.push(EncodedLine {
                        line,
                        utf16_len: utf16_col,
                        checkpoints: std::mem::take(&mut current_checkpoints),
                    });
                }
                line_break_widths.push(line_break_width);
                line_starts.push(TextSize::from(offset as u32 + u32::from(line_break_width)));
                byte_col = 0;
                utf16_col = 0;
                line += 1;
                continue;
            }

            let byte_len = c.len_utf8() as u32;
            let utf16_len = c.len_utf16() as u32;
            if byte_len != utf16_len {
                current_checkpoints.push(ColumnCheckpoint {
                    byte_start: byte_col,
                    byte_end: byte_col + byte_len,
                    utf16_start: utf16_col,
                    utf16_end: utf16_col + utf16_len,
                });
            }
            byte_col += byte_len;
            utf16_col += utf16_len;
        }
        if !current_checkpoints.is_empty() {
            encoded_lines.push(EncodedLine {
                line,
                utf16_len: utf16_col,
                checkpoints: current_checkpoints,
            });
        }
        line_break_widths.push(0);

        Self {
            line_starts,
            line_break_widths,
            encoded_lines,
            text_len: TextSize::from(text.len() as u32),
        }
    }

    fn encoded_line(&self, line: u32) -> Option<&EncodedLine> {
        self.encoded_lines
            .binary_search_by_key(&line, |encoded| encoded.line)
            .ok()
            .map(|index| &self.encoded_lines[index])
    }

    fn byte_line_len(&self, line: usize) -> Option<u32> {
        let start = *self.line_starts.get(line)?;
        let end = self
            .line_starts
            .get(line + 1)
            .map(|next| u32::from(*next).saturating_sub(u32::from(self.line_break_widths[line])))
            .unwrap_or_else(|| u32::from(self.text_len));
        Some(end.saturating_sub(u32::from(start)))
    }

    /// Returns the number of lines in the source.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    #[cfg(test)]
    fn heap_size_bytes(&self) -> usize {
        self.line_starts.capacity() * std::mem::size_of::<ByteOffset>()
            + self.line_break_widths.capacity() * std::mem::size_of::<u8>()
            + self.encoded_lines.capacity() * std::mem::size_of::<EncodedLine>()
            + self
                .encoded_lines
                .iter()
                .map(|line| line.checkpoints.capacity() * std::mem::size_of::<ColumnCheckpoint>())
                .sum::<usize>()
    }

    /// Converts a byte offset to a line/column position.
    ///
    /// Returns `None` if the offset is out of bounds.
    pub fn line_col(&self, offset: ByteOffset) -> Option<LineCol> {
        if offset > self.text_len {
            return None;
        }
        // Binary search for the line containing this offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };

        if line >= self.line_starts.len() {
            return None;
        }

        let line_start = self.line_starts[line];
        let col = u32::from(offset) - u32::from(line_start);

        Some(LineCol {
            line: line as u32,
            col,
        })
    }

    /// Converts a byte offset to a zero-indexed line and UTF-16 column.
    ///
    /// JavaScript source maps, TypeScript, and LSP positions use UTF-16 code
    /// units for columns. Returns `None` for offsets inside a UTF-8 code point.
    pub fn utf16_line_col(&self, offset: ByteOffset) -> Option<LineCol> {
        let byte_position = self.line_col(offset)?;
        let line = byte_position.line as usize;
        if byte_position.col > self.byte_line_len(line)? {
            return None;
        }

        let Some(encoded) = self.encoded_line(byte_position.line) else {
            return Some(byte_position);
        };
        let checkpoints = &encoded.checkpoints;
        let completed = checkpoints.partition_point(|point| point.byte_end <= byte_position.col);
        if let Some(point) = checkpoints.get(completed) {
            if byte_position.col > point.byte_start && byte_position.col < point.byte_end {
                return None;
            }
        }
        let reduction = completed
            .checked_sub(1)
            .map(|index| {
                let point = checkpoints[index];
                point.byte_end - point.utf16_end
            })
            .unwrap_or(0);

        Some(LineCol {
            line: byte_position.line,
            col: byte_position.col - reduction,
        })
    }

    /// Converts a line/column position to a byte offset.
    ///
    /// Returns `None` if the line is out of bounds.
    pub fn offset(&self, line_col: LineCol) -> Option<ByteOffset> {
        let line = line_col.line as usize;
        if line >= self.line_starts.len() {
            return None;
        }

        let line_start = self.line_starts[line];
        Some(line_start + TextSize::from(line_col.col))
    }

    /// Converts a zero-indexed line and UTF-16 column to a byte offset.
    ///
    /// Returns `None` for out-of-bounds positions or columns inside a UTF-16
    /// surrogate pair.
    pub fn offset_utf16(&self, line_col: LineCol) -> Option<ByteOffset> {
        let line = line_col.line as usize;
        if line >= self.line_starts.len() {
            return None;
        }

        let Some(encoded) = self.encoded_line(line_col.line) else {
            return (line_col.col <= self.byte_line_len(line)?)
                .then(|| self.line_starts[line] + TextSize::from(line_col.col));
        };
        if line_col.col > encoded.utf16_len {
            return None;
        }
        let checkpoints = &encoded.checkpoints;
        let completed = checkpoints.partition_point(|point| point.utf16_end <= line_col.col);
        if let Some(point) = checkpoints.get(completed) {
            if line_col.col > point.utf16_start && line_col.col < point.utf16_end {
                return None;
            }
        }
        let increase = completed
            .checked_sub(1)
            .map(|index| {
                let point = checkpoints[index];
                point.byte_end - point.utf16_end
            })
            .unwrap_or(0);

        Some(self.line_starts[line] + TextSize::from(line_col.col + increase))
    }

    /// Returns the byte offset where a line starts.
    pub fn line_start(&self, line: u32) -> Option<ByteOffset> {
        self.line_starts.get(line as usize).copied()
    }

    /// Returns the byte offset where a line ends (before the newline).
    pub fn line_end(&self, line: u32, text: &str) -> Option<ByteOffset> {
        let line = line as usize;
        if line >= self.line_starts.len() {
            return None;
        }

        let _start = self.line_starts[line];
        let end = self
            .line_starts
            .get(line + 1)
            .map(|&next| next - TextSize::from(u32::from(self.line_break_widths[line])))
            .unwrap_or_else(|| TextSize::from(text.len() as u32)); // End of file

        Some(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line() {
        let index = LineIndex::new("hello world");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_col(TextSize::from(0)), Some(LineCol::new(0, 0)));
        assert_eq!(index.line_col(TextSize::from(5)), Some(LineCol::new(0, 5)));
    }

    #[test]
    fn test_multiple_lines() {
        let index = LineIndex::new("hello\nworld\nfoo");
        assert_eq!(index.line_count(), 3);

        // First line
        assert_eq!(index.line_col(TextSize::from(0)), Some(LineCol::new(0, 0)));
        assert_eq!(index.line_col(TextSize::from(5)), Some(LineCol::new(0, 5)));

        // Second line
        assert_eq!(index.line_col(TextSize::from(6)), Some(LineCol::new(1, 0)));
        assert_eq!(index.line_col(TextSize::from(10)), Some(LineCol::new(1, 4)));

        // Third line
        assert_eq!(index.line_col(TextSize::from(12)), Some(LineCol::new(2, 0)));
    }

    #[test]
    fn ascii_metadata_remains_sparse_at_project_scale() {
        let text = "const value = 1;\n".repeat(100_000);
        let index = LineIndex::new(&text);
        assert!(index.encoded_lines.is_empty());
        assert!(
            index.heap_size_bytes() <= index.line_count() * 8,
            "{} bytes for {} lines",
            index.heap_size_bytes(),
            index.line_count()
        );
    }

    #[test]
    fn separates_svelte_and_typescript_line_terminators() {
        for separator in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            let text = format!("a{separator}b");
            let typescript = LineIndex::new_typescript(&text);
            assert_eq!(typescript.line_count(), 2, "separator {separator:?}");
            assert_eq!(
                typescript.utf16_line_col(TextSize::from((1 + separator.len()) as u32)),
                Some(LineCol::new(1, 0)),
                "separator {separator:?}"
            );
            assert_eq!(
                typescript.line_end(0, &text),
                Some(TextSize::from(1)),
                "separator {separator:?}"
            );

            let svelte = LineIndex::new(&text);
            assert_eq!(
                svelte.line_count(),
                if separator.contains('\n') { 2 } else { 1 },
                "separator {separator:?}"
            );
        }
    }

    #[test]
    fn test_offset_roundtrip() {
        let text = "hello\nworld\nfoo";
        let index = LineIndex::new(text);

        for offset in 0..text.len() {
            let offset = TextSize::from(offset as u32);
            let line_col = index.line_col(offset).unwrap();
            let back = index.offset(line_col).unwrap();
            assert_eq!(offset, back);
        }
    }

    #[test]
    fn test_line_start() {
        let index = LineIndex::new("hello\nworld\n");
        assert_eq!(index.line_start(0), Some(TextSize::from(0)));
        assert_eq!(index.line_start(1), Some(TextSize::from(6)));
        assert_eq!(index.line_start(2), Some(TextSize::from(12)));
    }

    #[test]
    fn test_utf16_columns_around_non_ascii_characters() {
        let index = LineIndex::new("a😀éz\n中x");

        assert_eq!(
            index.utf16_line_col(TextSize::from(1)),
            Some(LineCol::new(0, 1))
        );
        assert_eq!(
            index.utf16_line_col(TextSize::from(5)),
            Some(LineCol::new(0, 3))
        );
        assert_eq!(
            index.utf16_line_col(TextSize::from(7)),
            Some(LineCol::new(0, 4))
        );
        assert_eq!(
            index.utf16_line_col(TextSize::from(8)),
            Some(LineCol::new(0, 5))
        );
        assert_eq!(
            index.utf16_line_col(TextSize::from(12)),
            Some(LineCol::new(1, 1))
        );

        assert_eq!(
            index.offset_utf16(LineCol::new(0, 3)),
            Some(TextSize::from(5))
        );
        assert_eq!(
            index.offset_utf16(LineCol::new(0, 4)),
            Some(TextSize::from(7))
        );
        assert_eq!(
            index.offset_utf16(LineCol::new(1, 1)),
            Some(TextSize::from(12))
        );
    }

    #[test]
    fn test_utf16_rejects_positions_inside_encoded_characters() {
        let index = LineIndex::new("😀");

        assert_eq!(index.utf16_line_col(TextSize::from(1)), None);
        assert_eq!(index.offset_utf16(LineCol::new(0, 1)), None);
    }

    #[test]
    fn utf16_roundtrips_for_a_deterministic_unicode_corpus() {
        fn check(text: &str) {
            for index in [LineIndex::new(text), LineIndex::new_typescript(text)] {
                for byte in 0..=text.len() {
                    let offset = TextSize::from(byte as u32);
                    let position = index.utf16_line_col(offset);
                    if let Some(position) = position {
                        assert_eq!(
                            index.offset_utf16(position),
                            Some(offset),
                            "roundtrip failed at byte {byte} in {text:?}"
                        );
                    } else if text.is_char_boundary(byte) {
                        assert!(
                            byte > 0
                                && byte < text.len()
                                && text.as_bytes()[byte - 1] == b'\r'
                                && text.as_bytes()[byte] == b'\n',
                            "rejected a valid boundary at byte {byte} in {text:?}"
                        );
                    }
                }
            }
        }

        fn enumerate(prefix: &mut String, depth: usize) {
            check(prefix);
            if depth == 0 {
                return;
            }
            for fragment in ["a", "é", "中", "😀", "\n", "\r", "\u{2028}", "\u{2029}"] {
                let length = prefix.len();
                prefix.push_str(fragment);
                enumerate(prefix, depth - 1);
                prefix.truncate(length);
            }
        }

        enumerate(&mut String::new(), 3);
    }
}
