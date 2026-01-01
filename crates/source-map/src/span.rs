//! Span and byte offset types for source positions.

use text_size::{TextRange, TextSize};

/// A byte offset into a source string.
pub type ByteOffset = TextSize;

/// A span representing a range in source code.
///
/// Spans are half-open intervals `[start, end)` represented as byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span {
    /// The start byte offset (inclusive).
    pub start: ByteOffset,
    /// The end byte offset (exclusive).
    pub end: ByteOffset,
}

impl Span {
    /// Creates a new span from start and end byte offsets.
    #[inline]
    pub fn new(start: impl Into<ByteOffset>, end: impl Into<ByteOffset>) -> Self {
        Self {
            start: start.into(),
            end: end.into(),
        }
    }

    /// Creates an empty span at the given offset.
    #[inline]
    pub fn empty(offset: impl Into<ByteOffset>) -> Self {
        let offset = offset.into();
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Returns the length of this span in bytes.
    #[inline]
    pub fn len(&self) -> TextSize {
        self.end - self.start
    }

    /// Returns true if this span is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns true if this span contains the given offset.
    #[inline]
    pub fn contains(&self, offset: ByteOffset) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns true if this span contains the given span entirely.
    #[inline]
    pub fn contains_span(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Returns a span covering both this span and another.
    #[inline]
    pub fn cover(self, other: Span) -> Span {
        Span {
            start: std::cmp::min(self.start, other.start),
            end: std::cmp::max(self.end, other.end),
        }
    }

    /// Converts this span to a `TextRange`.
    #[inline]
    pub fn to_range(self) -> TextRange {
        TextRange::new(self.start, self.end)
    }
}

impl From<TextRange> for Span {
    fn from(range: TextRange) -> Self {
        Self {
            start: range.start(),
            end: range.end(),
        }
    }
}

impl From<Span> for TextRange {
    fn from(span: Span) -> Self {
        TextRange::new(span.start, span.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_new() {
        let span = Span::new(0u32, 10u32);
        assert_eq!(span.start, TextSize::from(0));
        assert_eq!(span.end, TextSize::from(10));
    }

    #[test]
    fn test_span_empty() {
        let span = Span::empty(5u32);
        assert!(span.is_empty());
        assert_eq!(span.len(), TextSize::from(0));
    }

    #[test]
    fn test_span_contains() {
        let span = Span::new(5u32, 15u32);
        assert!(!span.contains(TextSize::from(4)));
        assert!(span.contains(TextSize::from(5)));
        assert!(span.contains(TextSize::from(10)));
        assert!(!span.contains(TextSize::from(15)));
    }

    #[test]
    fn test_span_cover() {
        let a = Span::new(5u32, 10u32);
        let b = Span::new(8u32, 20u32);
        let covered = a.cover(b);
        assert_eq!(covered.start, TextSize::from(5));
        assert_eq!(covered.end, TextSize::from(20));
    }
}
