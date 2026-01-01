//! Source map builder for tracking position mappings during transformation.

use crate::{ByteOffset, Span};
use text_size::TextSize;

/// A single mapping from generated position to original position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    /// The span in the generated output.
    pub generated: Span,
    /// The span in the original source.
    pub original: Span,
}

/// A source map that tracks position mappings from generated code back to original source.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    /// List of mappings, sorted by generated position.
    mappings: Vec<Mapping>,
}

impl SourceMap {
    /// Creates a new empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a source map builder.
    pub fn builder() -> SourceMapBuilder {
        SourceMapBuilder::new()
    }

    /// Returns the number of mappings in this source map.
    #[inline]
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// Returns true if this source map has no mappings.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    /// Returns an iterator over all mappings.
    pub fn mappings(&self) -> impl Iterator<Item = &Mapping> {
        self.mappings.iter()
    }

    /// Finds the original position corresponding to a generated position.
    ///
    /// Returns `None` if no mapping covers the given position.
    pub fn original_position(&self, generated: ByteOffset) -> Option<ByteOffset> {
        // Binary search for the mapping containing this position
        let mapping = self.find_mapping_for_generated(generated)?;

        // Calculate the offset within the generated span
        let offset_in_span = u32::from(generated) - u32::from(mapping.generated.start);

        // Apply the same offset to the original span
        Some(mapping.original.start + TextSize::from(offset_in_span))
    }

    /// Finds the generated position corresponding to an original position.
    ///
    /// Returns `None` if no mapping covers the given position.
    pub fn generated_position(&self, original: ByteOffset) -> Option<ByteOffset> {
        // Linear search since mappings are sorted by generated position
        for mapping in &self.mappings {
            if mapping.original.contains(original) {
                let offset_in_span = u32::from(original) - u32::from(mapping.original.start);
                return Some(mapping.generated.start + TextSize::from(offset_in_span));
            }
        }
        None
    }

    /// Finds the mapping that contains the given generated position.
    fn find_mapping_for_generated(&self, generated: ByteOffset) -> Option<&Mapping> {
        // Binary search for the first mapping where generated.start <= position
        let idx = match self
            .mappings
            .binary_search_by(|m| m.generated.start.cmp(&generated))
        {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        self.mappings
            .get(idx)
            .filter(|m| m.generated.contains(generated))
    }
}

/// A builder for constructing source maps during transformation.
#[derive(Debug, Default)]
pub struct SourceMapBuilder {
    mappings: Vec<Mapping>,
    /// Current position in the generated output.
    generated_offset: ByteOffset,
}

impl SourceMapBuilder {
    /// Creates a new source map builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current generated offset.
    #[inline]
    pub fn generated_offset(&self) -> ByteOffset {
        self.generated_offset
    }

    /// Adds a mapping from an original span to the current generated position.
    ///
    /// The generated span will start at the current offset and have the given length.
    pub fn add_mapping(&mut self, original: Span, generated_len: u32) {
        let generated_start = self.generated_offset;
        let generated_end = generated_start + TextSize::from(generated_len);

        self.mappings.push(Mapping {
            generated: Span::new(generated_start, generated_end),
            original,
        });

        self.generated_offset = generated_end;
    }

    /// Adds verbatim source text, creating a 1:1 mapping.
    ///
    /// Use this when copying text from the original source unchanged.
    pub fn add_source(&mut self, original_start: ByteOffset, text: &str) {
        let len = text.len() as u32;
        let original = Span::new(original_start, original_start + TextSize::from(len));
        self.add_mapping(original, len);
    }

    /// Adds generated text without a corresponding original position.
    ///
    /// Use this for synthetic code that doesn't map back to source.
    pub fn add_generated(&mut self, text: &str) {
        self.generated_offset += TextSize::from(text.len() as u32);
    }

    /// Skips the given amount in generated output without adding a mapping.
    pub fn skip(&mut self, len: u32) {
        self.generated_offset += TextSize::from(len);
    }

    /// Adds transformed content where the generated text differs from the original.
    ///
    /// This creates a mapping from the original span to the generated text,
    /// even when they have different lengths. Useful for rune transformations.
    pub fn add_transformed(&mut self, original: Span, generated_text: &str) {
        let gen_len = generated_text.len() as u32;
        self.mappings.push(Mapping {
            generated: Span::new(
                self.generated_offset,
                self.generated_offset + TextSize::from(gen_len),
            ),
            original,
        });
        self.generated_offset += TextSize::from(gen_len);
    }

    /// Adds an expression with its original span, appending a semicolon.
    ///
    /// This is a convenience method for template expressions.
    pub fn add_expression(&mut self, original_span: Span, expression: &str) {
        // Map the expression
        self.add_transformed(original_span, expression);
        // Add untracked semicolon and newline
        self.add_generated(";\n");
    }

    /// Builds the final source map.
    pub fn build(mut self) -> SourceMap {
        // Sort mappings by generated position for efficient lookup
        self.mappings.sort_by_key(|m| m.generated.start);
        SourceMap {
            mappings: self.mappings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_source_map() {
        let map = SourceMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_builder_add_source() {
        let mut builder = SourceMapBuilder::new();
        builder.add_source(TextSize::from(0), "hello");
        builder.add_generated(" ");
        builder.add_source(TextSize::from(10), "world");

        let map = builder.build();
        assert_eq!(map.len(), 2);

        // "hello" at generated 0-5 maps to original 0-5
        assert_eq!(
            map.original_position(TextSize::from(0)),
            Some(TextSize::from(0))
        );
        assert_eq!(
            map.original_position(TextSize::from(4)),
            Some(TextSize::from(4))
        );

        // " " at generated 5-6 has no mapping
        assert_eq!(map.original_position(TextSize::from(5)), None);

        // "world" at generated 6-11 maps to original 10-15
        assert_eq!(
            map.original_position(TextSize::from(6)),
            Some(TextSize::from(10))
        );
    }

    #[test]
    fn test_reverse_lookup() {
        let mut builder = SourceMapBuilder::new();
        builder.add_source(TextSize::from(10), "hello");

        let map = builder.build();

        // Original position 10 → generated position 0
        assert_eq!(
            map.generated_position(TextSize::from(10)),
            Some(TextSize::from(0))
        );
        assert_eq!(
            map.generated_position(TextSize::from(14)),
            Some(TextSize::from(4))
        );
    }
}
