//! Line index for efficient offset ↔ line/column conversion.

use crate::ByteOffset;
use text_size::TextSize;

/// A line and column position (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LineCol {
    /// 0-indexed line number.
    pub line: u32,
    /// 0-indexed column (byte offset within the line).
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
}

impl LineIndex {
    /// Creates a new line index from source text.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![TextSize::from(0)];

        for (offset, c) in text.char_indices() {
            if c == '\n' {
                // Next line starts after the newline
                line_starts.push(TextSize::from((offset + 1) as u32));
            }
        }

        Self { line_starts }
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
}
