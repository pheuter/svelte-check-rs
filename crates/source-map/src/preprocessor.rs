//! Standard source-map support for configured Svelte preprocessors.

use crate::LineCol;
use camino::Utf8Path;
use percent_encoding::percent_decode_str;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use url::Url;

/// Failure to decode a configured preprocessor's source map.
#[derive(Debug, Error)]
pub enum PreprocessorMapError {
    /// The source-map codec rejected the input.
    #[error("failed to decode preprocessor source map: {0}")]
    Decode(#[from] swc_sourcemap::Error),
    /// The input violates v3 invariants not enforced by the codec.
    #[error("{0}")]
    Invalid(String),
}

/// Maps positions in preprocessed component source back to the user's source.
///
/// Svelte preprocessors use the standard v3 source-map format. Keeping this
/// map separate from the transformer's byte-offset map makes the two stages
/// explicit: generated TypeScript -> preprocessed Svelte -> original Svelte.
#[derive(Debug, Clone)]
pub struct PreprocessorMap {
    map: Arc<swc_sourcemap::DecodedMap>,
}

/// A mapped position together with the original source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreprocessorMappedPosition {
    /// Zero-indexed position in the mapped source.
    pub position: LineCol,
    /// Source path after applying the map's `sourceRoot`.
    pub source: String,
}

impl PreprocessorMap {
    /// Parses a standard v3 source map returned by `svelte/compiler.preprocess`.
    pub fn parse(json: &str) -> Result<Self, PreprocessorMapError> {
        let value: Value = serde_json::from_str(json)
            .map_err(|error| PreprocessorMapError::Invalid(format!("invalid JSON: {error}")))?;
        validate_v3_shape(&value)?;
        drop(value);
        let decoded = swc_sourcemap::decode_slice(json.as_bytes())?;
        validate_destination_offsets(&decoded, 0, 0)?;
        let map = match decoded {
            // Index lookups return section-local destination coordinates. A
            // regular map keeps the existing cross-line GLB guard correct for
            // sections with nonzero offsets (including nested indexes).
            swc_sourcemap::DecodedMap::Index(index) => {
                swc_sourcemap::DecodedMap::Regular(index.flatten()?)
            }
            other => other,
        };
        Ok(Self { map: Arc::new(map) })
    }

    /// Maps a zero-indexed position in generated/preprocessed source to its
    /// corresponding zero-indexed position in the original source.
    pub fn original_position(&self, generated: LineCol) -> Option<PreprocessorMappedPosition> {
        let token = self.map.lookup_token(generated.line, generated.col)?;
        // `lookup_token` is a global GLB lookup. An empty generated line must
        // not inherit a token from the preceding line.
        if token.get_dst_line() != generated.line || !token.has_source() {
            return None;
        }
        let (line, col) = token.get_src();
        if line == u32::MAX || col == u32::MAX {
            return None;
        }
        Some(PreprocessorMappedPosition {
            position: LineCol { line, col },
            source: token.get_source()?.to_string(),
        })
    }

    /// Maps only positions whose source identity refers to `expected_source`.
    pub fn original_position_in(
        &self,
        generated: LineCol,
        expected_source: &Utf8Path,
    ) -> Option<LineCol> {
        let token = self.map.lookup_token(generated.line, generated.col)?;
        if token.get_dst_line() != generated.line || !token.has_source() {
            return None;
        }
        let (line, col) = token.get_src();
        if line == u32::MAX || col == u32::MAX {
            return None;
        }
        source_matches(
            token.get_source()?,
            expected_source,
            token.sourcemap().sources().map(|source| source.as_ref()),
        )
        .then_some(LineCol { line, col })
    }
}

fn validate_v3_shape(value: &Value) -> Result<(), PreprocessorMapError> {
    let object = value
        .as_object()
        .ok_or_else(|| PreprocessorMapError::Invalid("map must be an object".to_string()))?;
    if object.get("version").and_then(Value::as_u64) != Some(3) {
        return Err(PreprocessorMapError::Invalid(
            "map and section versions must be 3".to_string(),
        ));
    }

    if let Some(sections) = object.get("sections") {
        if let Some(field) = [
            "mappings",
            "sources",
            "names",
            "sourceRoot",
            "sourcesContent",
        ]
        .into_iter()
        .find(|field| object.contains_key(*field))
        {
            return Err(PreprocessorMapError::Invalid(format!(
                "indexed maps must not contain regular-map field {field}"
            )));
        }
        let sections = sections.as_array().ok_or_else(|| {
            PreprocessorMapError::Invalid("sections must be an array".to_string())
        })?;
        for section in sections {
            let section = section.as_object().ok_or_else(|| {
                PreprocessorMapError::Invalid("section must be an object".to_string())
            })?;
            let offset = section
                .get("offset")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    PreprocessorMapError::Invalid("section offset is missing".to_string())
                })?;
            for coordinate in ["line", "column"] {
                let value = offset.get(coordinate).and_then(Value::as_u64);
                if value.is_none_or(|value| value > u64::from(u32::MAX)) {
                    return Err(PreprocessorMapError::Invalid(format!(
                        "section {coordinate} must fit in u32"
                    )));
                }
            }
            match (section.get("map"), section.get("url")) {
                (Some(map), None) => validate_v3_shape(map)?,
                (None, Some(_)) => {
                    return Err(PreprocessorMapError::Invalid(
                        "URL sections are not supported".to_string(),
                    ));
                }
                (Some(_), Some(_)) => {
                    return Err(PreprocessorMapError::Invalid(
                        "section must not contain both map and url".to_string(),
                    ));
                }
                (None, None) => {
                    return Err(PreprocessorMapError::Invalid(
                        "section embedded map is missing".to_string(),
                    ));
                }
            }
        }
    } else if !object.get("mappings").is_some_and(Value::is_string)
        || !object.get("sources").is_some_and(Value::is_array)
    {
        return Err(PreprocessorMapError::Invalid(
            "regular maps require string mappings and an array of sources".to_string(),
        ));
    }
    Ok(())
}

fn validate_destination_offsets(
    map: &swc_sourcemap::DecodedMap,
    base_line: u32,
    base_column: u32,
) -> Result<(), PreprocessorMapError> {
    let overflow =
        || PreprocessorMapError::Invalid("indexed destination offset overflows u32".to_string());
    match map {
        swc_sourcemap::DecodedMap::Regular(map) => {
            for token in map.tokens() {
                base_line
                    .checked_add(token.get_dst_line())
                    .ok_or_else(overflow)?;
                if token.get_dst_line() == 0 {
                    base_column
                        .checked_add(token.get_dst_col())
                        .ok_or_else(overflow)?;
                }
            }
        }
        swc_sourcemap::DecodedMap::Hermes(map) => {
            let _ = map;
            return Err(PreprocessorMapError::Invalid(
                "Hermes maps are not supported for Svelte preprocessors".to_string(),
            ));
        }
        swc_sourcemap::DecodedMap::Index(index) => {
            for section in index.sections() {
                let (line, column) = section.get_offset();
                let line = base_line.checked_add(line).ok_or_else(overflow)?;
                let column = if line == base_line {
                    base_column.checked_add(column).ok_or_else(overflow)?
                } else {
                    column
                };
                if let Some(map) = section.get_sourcemap() {
                    validate_destination_offsets(map, line, column)?;
                }
            }
        }
    }
    Ok(())
}

fn source_matches<'a>(
    source: &str,
    expected: &Utf8Path,
    all_sources: impl Iterator<Item = &'a str>,
) -> bool {
    let source = normalize_source_identity(source);
    let expected = normalize_path(expected.as_str());
    if !identity_matches(&source, &expected) {
        return false;
    }

    let basename_only = !source.trim_start_matches("./").contains('/');
    let selected_basename = source.rsplit('/').next();
    let matching_identities: HashSet<_> = all_sources
        .map(normalize_source_identity)
        .filter(|candidate| {
            if basename_only {
                candidate.rsplit('/').next() == selected_basename
            } else {
                identity_matches(candidate, &expected)
            }
        })
        .collect();
    matching_identities.len() == 1
}

fn identity_matches(source: &str, expected: &str) -> bool {
    if path_eq(source, expected) {
        return true;
    }
    let source = source.trim_start_matches("./");
    if is_absolute_identity(source) {
        return false;
    }
    if let Some(parent) = expected.rsplit_once('/').map(|(parent, _)| parent) {
        let resolved = normalize_path(&format!("{parent}/{source}"));
        if path_eq(&resolved, expected) {
            return true;
        }
    }
    expected == source || expected.ends_with(&format!("/{source}"))
}

fn normalize_source_identity(source: &str) -> String {
    if let Ok(url) = Url::parse(source) {
        if url.scheme() == "file" {
            let decoded_path = percent_decode_str(url.path()).decode_utf8_lossy();
            let path = match url.host_str() {
                Some(host) if !host.eq_ignore_ascii_case("localhost") => {
                    format!("//{host}{decoded_path}")
                }
                _ => decoded_path.into_owned(),
            };
            return normalize_path(&path);
        }
    }
    normalize_path(&percent_decode_str(source).decode_utf8_lossy())
}

fn normalize_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut prefix = "";
    let mut rest = path.as_str();
    if rest.starts_with("//") {
        prefix = "//";
        rest = rest.trim_start_matches('/');
    } else if rest.starts_with('/') {
        prefix = "/";
        rest = rest.trim_start_matches('/');
    }

    // `file:///C:/...` becomes `/C:/...` on Unix. Normalize it to the same
    // portable spelling as a Windows path supplied by a source map consumer.
    if prefix == "/" {
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            prefix = "";
        }
    }

    let mut parts = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|previous| *previous != "..") => {
                parts.pop();
            }
            ".." if prefix.is_empty() => parts.push(part),
            ".." => {}
            _ => parts.push(part),
        }
    }
    format!("{prefix}{}", parts.join("/"))
}

fn is_absolute_identity(path: &str) -> bool {
    path.starts_with('/')
        || path.starts_with("//")
        || path
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
}

fn path_eq(left: &str, right: &str) -> bool {
    left == right || (cfg!(windows) && left.eq_ignore_ascii_case(right))
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
            map.original_position_in(LineCol::new(0, 0), Utf8Path::new("/tmp/input.svelte")),
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
            map.original_position_in(LineCol::new(0, 0), Utf8Path::new("/tmp/input.svelte")),
            Some(LineCol::new(1, 0))
        );
    }

    #[test]
    fn rejects_hybrid_indexed_maps() {
        let json = r#"{"version":3,"sections":[],"sources":[],"names":[],"mappings":""}"#;

        let error = PreprocessorMap::parse(json).expect_err("hybrid map must be rejected");
        assert!(matches!(
            error,
            PreprocessorMapError::Invalid(message)
                if message == "indexed maps must not contain regular-map field mappings"
        ));
    }

    #[test]
    fn rejects_index_sections_with_both_map_and_url() {
        let json = r#"{"version":3,"sections":[{"offset":{"line":0,"column":0},"map":{"version":3,"sources":[],"names":[],"mappings":""},"url":"part.map"}]}"#;

        let error = PreprocessorMap::parse(json).expect_err("ambiguous section must be rejected");
        assert!(matches!(
            error,
            PreprocessorMapError::Invalid(message)
                if message == "section must not contain both map and url"
        ));
    }

    #[test]
    fn flattens_nonzero_and_nested_index_offsets() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sections":[{"offset":{"line":2,"column":4},"map":{"version":3,"sections":[{"offset":{"line":0,"column":0},"map":{"version":3,"sources":["input.svelte"],"names":[],"mappings":"AAAA;AACA"}}]}}]}"#,
        )
        .expect("valid nested indexed source map");

        assert_eq!(
            map.original_position_in(LineCol::new(2, 4), Utf8Path::new("/tmp/input.svelte")),
            Some(LineCol::new(0, 0))
        );
        assert_eq!(
            map.original_position_in(LineCol::new(3, 0), Utf8Path::new("/tmp/input.svelte")),
            Some(LineCol::new(1, 0))
        );
        assert!(map.original_position(LineCol::new(4, 0)).is_none());
    }

    #[test]
    fn rejects_unresolved_and_malformed_maps() {
        let malformed = [
            "",
            "{",
            "{}",
            r#"{"version":4,"sources":[],"names":[],"mappings":""}"#,
            r#"{"version":3,"sources":[],"names":[],"mappings":"!"}"#,
            r#"{"version":3,"sections":[{"offset":{"line":0,"column":0},"url":"part.map"}]}"#,
            r#"{"version":3,"sections":[{"offset":{"line":4294967295,"column":0},"map":{"version":3,"sources":["input.svelte"],"names":[],"mappings":";AAAA"}}]}"#,
        ];
        for json in malformed {
            assert!(PreprocessorMap::parse(json).is_err(), "accepted {json}");
        }
    }

    #[test]
    fn malformed_mapping_corpus_never_panics() {
        fn exercise(mapping: &mut String, remaining: usize) {
            let json = format!(
                r#"{{"version":3,"sources":["input.svelte"],"names":[],"mappings":"{mapping}"}}"#
            );
            let outcome = std::panic::catch_unwind(|| {
                if let Ok(map) = PreprocessorMap::parse(&json) {
                    for line in 0..3 {
                        for column in 0..8 {
                            let _ = map.original_position(LineCol::new(line, column));
                        }
                    }
                }
            });
            assert!(outcome.is_ok(), "mapping {mapping:?} panicked");

            if remaining == 0 {
                return;
            }
            for character in ['A', ';', ',', '/', '!', '9'] {
                mapping.push(character);
                exercise(mapping, remaining - 1);
                mapping.pop();
            }
        }

        exercise(&mut String::new(), 4);
    }

    #[test]
    fn low_resolution_same_line_lookup_is_stable() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sources":["input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("valid low-resolution map");
        for column in 0..128 {
            assert_eq!(
                map.original_position_in(
                    LineCol::new(0, column),
                    Utf8Path::new("/tmp/input.svelte")
                ),
                Some(LineCol::new(0, 0))
            );
        }
        assert!(map.original_position(LineCol::new(1, 0)).is_none());
    }

    #[test]
    fn rejects_cross_line_fallback_and_other_sources() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sources":["styles/_partial.scss"],"names":[],"mappings":"AAAA;"}"#,
        )
        .expect("valid source map");

        assert!(map.original_position(LineCol::new(1, 0)).is_none());
        assert!(map
            .original_position_in(LineCol::new(0, 0), Utf8Path::new("/tmp/input.svelte"))
            .is_none());
    }

    #[test]
    fn validates_source_root_against_the_component() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sourceRoot":"src","sources":["input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("valid source map");
        assert_eq!(
            map.original_position_in(
                LineCol::new(0, 0),
                Utf8Path::new("/workspace/src/input.svelte")
            ),
            Some(LineCol::new(0, 0))
        );
        assert!(map
            .original_position_in(
                LineCol::new(0, 0),
                Utf8Path::new("/workspace/other/input.svelte")
            )
            .is_none());
    }

    #[test]
    fn rejects_tokens_targeting_an_imported_source_in_multi_source_maps() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sources":["input.svelte","_partial.scss"],"names":[],"mappings":"ACAA"}"#,
        )
        .expect("valid source map");
        let mapped = map
            .original_position(LineCol::new(0, 0))
            .expect("mapped token");
        assert_eq!(mapped.source, "_partial.scss");
        assert!(map
            .original_position_in(LineCol::new(0, 0), Utf8Path::new("/tmp/input.svelte"))
            .is_none());
    }

    #[test]
    fn normalizes_relative_and_file_url_source_identities() {
        let relative = PreprocessorMap::parse(
            r#"{"version":3,"sources":["../src/input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("relative source map");
        assert_eq!(
            relative.original_position_in(
                LineCol::new(0, 0),
                Utf8Path::new("/workspace/src/input.svelte")
            ),
            Some(LineCol::new(0, 0))
        );

        let encoded = PreprocessorMap::parse(
            r#"{"version":3,"sources":["file:///tmp/my%20project/input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("file URL source map");
        assert_eq!(
            encoded.original_position_in(
                LineCol::new(0, 0),
                Utf8Path::new("/tmp/my project/input.svelte")
            ),
            Some(LineCol::new(0, 0))
        );

        let windows = PreprocessorMap::parse(
            r#"{"version":3,"sources":["file:///C:/work/input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("Windows file URL source map");
        assert_eq!(
            windows.original_position_in(LineCol::new(0, 0), Utf8Path::new("C:/work/input.svelte")),
            Some(LineCol::new(0, 0))
        );

        let relative_encoded = PreprocessorMap::parse(
            r#"{"version":3,"sources":["src/my%20file.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("percent-encoded relative source map");
        assert_eq!(
            relative_encoded.original_position_in(
                LineCol::new(0, 0),
                Utf8Path::new("/workspace/src/my file.svelte")
            ),
            Some(LineCol::new(0, 0))
        );
    }

    #[test]
    fn rejects_ambiguous_basename_only_sources() {
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sources":["input.svelte","nested/input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("valid source map");
        assert!(map
            .original_position_in(LineCol::new(0, 0), Utf8Path::new("/tmp/input.svelte"))
            .is_none());
    }

    #[test]
    fn rejects_ambiguous_suffixes_but_deduplicates_identical_identities() {
        let ambiguous = PreprocessorMap::parse(
            r#"{"version":3,"sources":["src/input.svelte","else/src/input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("valid source map");
        assert!(ambiguous
            .original_position_in(
                LineCol::new(0, 0),
                Utf8Path::new("/workspace/else/src/input.svelte")
            )
            .is_none());

        let duplicate = PreprocessorMap::parse(
            r#"{"version":3,"sources":["input.svelte","input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("valid source map");
        assert_eq!(
            duplicate.original_position_in(LineCol::new(0, 0), Utf8Path::new("/tmp/input.svelte")),
            Some(LineCol::new(0, 0))
        );
    }
}
