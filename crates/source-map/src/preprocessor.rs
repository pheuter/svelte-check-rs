//! Standard source-map support for configured Svelte preprocessors.

use crate::LineCol;
use std::sync::Arc;
use thiserror::Error;

/// Failure to decode a configured preprocessor's source map.
#[derive(Debug, Error)]
#[error("failed to decode preprocessor source map: {0}")]
pub struct PreprocessorMapError(#[from] swc_sourcemap::Error);

/// Maps positions in preprocessed component source back to the user's source.
///
/// Svelte preprocessors use the standard v3 source-map format. Keeping this
/// map separate from the transformer's byte-offset map makes the two stages
/// explicit: generated TypeScript -> preprocessed Svelte -> original Svelte.
#[derive(Debug, Clone)]
pub struct PreprocessorMap {
    map: Arc<swc_sourcemap::DecodedMap>,
}

impl PreprocessorMap {
    /// Parses a standard v3 source map returned by `svelte/compiler.preprocess`.
    pub fn parse(json: &str) -> Result<Self, PreprocessorMapError> {
        swc_sourcemap::decode_slice(json.as_bytes())
            .map(|map| Self { map: Arc::new(map) })
            .map_err(PreprocessorMapError::from)
    }

    /// Maps a zero-indexed position in generated/preprocessed source to its
    /// corresponding zero-indexed position in the original source.
    pub fn original_position(&self, generated: LineCol) -> Option<LineCol> {
        let token = self.map.lookup_token(generated.line, generated.col)?;
        token.has_source().then(|| {
            let (line, col) = token.get_src();
            LineCol { line, col }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_generated_positions_to_original_source() {
        // One generated line mapped to line 2 of the original file.
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sources":["input.svelte"],"names":[],"mappings":"AACA"}"#,
        )
        .expect("valid source map");

        assert_eq!(
            map.original_position(LineCol::new(0, 0)),
            Some(LineCol::new(1, 0))
        );
    }

    #[test]
    fn accepts_indexed_source_maps() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sections":[{"offset":{"line":0,"column":0},"map":{"version":3,"sources":["input.svelte"],"names":[],"mappings":"AACA"}}]}"#,
        )
        .expect("valid indexed source map");

        assert_eq!(
            map.original_position(LineCol::new(0, 0)),
            Some(LineCol::new(1, 0))
        );
    }
}
