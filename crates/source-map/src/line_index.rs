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
    /// Byte length of each line, excluding its newline.
    line_byte_lengths: Vec<u32>,
    /// UTF-16 code-unit length of each line, excluding its newline.
    line_utf16_lengths: Vec<u32>,
    /// Unicode scalar-value length of each line, excluding its newline.
    line_char_lengths: Vec<u32>,
    /// Checkpoints for non-ASCII characters where column encodings differ.
    column_checkpoints: Vec<Vec<ColumnCheckpoint>>,
    /// Total source length in bytes.
    text_len: ByteOffset,
}

#[derive(Debug, Clone, Copy)]
struct ColumnCheckpoint {
    byte_start: u32,
    byte_end: u32,
    utf16_start: u32,
    utf16_end: u32,
    char_end: u32,
}

impl LineIndex {
    /// Creates a new line index from source text.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![TextSize::from(0)];
        let mut line_byte_lengths = Vec::new();
        let mut line_utf16_lengths = Vec::new();
        let mut line_char_lengths = Vec::new();
        let mut column_checkpoints = Vec::new();
        let mut current_checkpoints = Vec::new();
        let mut byte_col = 0u32;
        let mut utf16_col = 0u32;
        let mut char_col = 0u32;

        for (offset, c) in text.char_indices() {
            if c == '\n' {
                line_byte_lengths.push(byte_col);
                line_utf16_lengths.push(utf16_col);
                line_char_lengths.push(char_col);
                column_checkpoints.push(std::mem::take(&mut current_checkpoints));
                // Next line starts after the newline
                line_starts.push(TextSize::from((offset + 1) as u32));
                byte_col = 0;
                utf16_col = 0;
                char_col = 0;
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
                    char_end: char_col + 1,
                });
            }
            byte_col += byte_len;
            utf16_col += utf16_len;
            char_col += 1;
        }
        line_byte_lengths.push(byte_col);
        line_utf16_lengths.push(utf16_col);
        line_char_lengths.push(char_col);
        column_checkpoints.push(current_checkpoints);

        Self {
            line_starts,
            line_byte_lengths,
            line_utf16_lengths,
            line_char_lengths,
            column_checkpoints,
            text_len: TextSize::from(text.len() as u32),
        }
    }

    /// Returns the number of lines in the source.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
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
        if byte_position.col > self.line_byte_lengths[line] {
            return None;
        }

        let checkpoints = &self.column_checkpoints[line];
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

    /// Converts a byte offset to a zero-indexed line and Unicode scalar column.
    ///
    /// The native TypeScript CLI reports columns in Unicode scalar values.
    /// Returns `None` for offsets inside a UTF-8 code point.
    pub fn char_line_col(&self, offset: ByteOffset) -> Option<LineCol> {
        let byte_position = self.line_col(offset)?;
        let line = byte_position.line as usize;
        if byte_position.col > self.line_byte_lengths[line] {
            return None;
        }

        let checkpoints = &self.column_checkpoints[line];
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
                point.byte_end - point.char_end
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
        if line >= self.line_starts.len() || line_col.col > self.line_utf16_lengths[line] {
            return None;
        }

        let checkpoints = &self.column_checkpoints[line];
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

    /// Converts a zero-indexed line and Unicode scalar column to a byte offset.
    ///
    /// This is the inverse coordinate conversion for native TypeScript CLI
    /// positions. Returns `None` for out-of-bounds positions.
    pub fn offset_char(&self, line_col: LineCol) -> Option<ByteOffset> {
        let line = line_col.line as usize;
        if line >= self.line_starts.len() || line_col.col > self.line_char_lengths[line] {
            return None;
        }

        let checkpoints = &self.column_checkpoints[line];
        let completed = checkpoints.partition_point(|point| point.char_end <= line_col.col);
        let increase = completed
            .checked_sub(1)
            .map(|index| {
                let point = checkpoints[index];
                point.byte_end - point.char_end
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
            .map(|&next| next - TextSize::from(1)) // Before newline
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
    fn test_unicode_scalar_columns() {
        let index = LineIndex::new("a😀éz");

        assert_eq!(
            index.char_line_col(TextSize::from(5)),
            Some(LineCol::new(0, 2))
        );
        assert_eq!(
            index.char_line_col(TextSize::from(7)),
            Some(LineCol::new(0, 3))
        );
        assert_eq!(
            index.offset_char(LineCol::new(0, 2)),
            Some(TextSize::from(5))
        );
        assert_eq!(
            index.offset_char(LineCol::new(0, 3)),
            Some(TextSize::from(7))
        );
    }
}
