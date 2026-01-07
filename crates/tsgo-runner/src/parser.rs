//! tsgo output parser.

use crate::runner::{TransformedFiles, TsgoError};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use source_map::LineCol;

/// A diagnostic from tsgo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsgoDiagnostic {
    /// The file path.
    pub file: Utf8PathBuf,
    /// The start position.
    pub start: DiagnosticPosition,
    /// The end position.
    pub end: DiagnosticPosition,
    /// The error message.
    pub message: String,
    /// The TypeScript error code.
    pub code: String,
    /// The severity.
    pub severity: DiagnosticSeverity,
}

/// A position in a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticPosition {
    /// 1-indexed line number.
    pub line: u32,
    /// 1-indexed column number.
    pub column: u32,
    /// Byte offset in the file.
    pub offset: u32,
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Suggestion,
}

/// Parsed tsgo output.
#[derive(Debug, Default)]
pub struct TsgoOutput {
    /// The diagnostics.
    pub diagnostics: Vec<TsgoDiagnostic>,
}

/// Parses tsgo output into diagnostics.
pub fn parse_tsgo_output(
    output: &str,
    files: &TransformedFiles,
) -> Result<Vec<TsgoDiagnostic>, TsgoError> {
    let mut diagnostics = Vec::new();

    // tsgo outputs diagnostics in the format:
    // file.ts:line:column - error TS1234: message
    for line in output.lines() {
        if let Some(diag) = parse_diagnostic_line(line, files) {
            diagnostics.push(diag);
        }
    }

    Ok(diagnostics)
}

/// Parses a single diagnostic line.
fn parse_diagnostic_line(line: &str, files: &TransformedFiles) -> Option<TsgoDiagnostic> {
    // tsgo outputs: file.tsx(line,column): error TS1234: message
    // We need to parse this format

    // Find the opening paren for position
    let paren_start = line.find('(')?;
    let paren_end = line.find(')')?;

    let file_path = &line[..paren_start];
    let position = &line[paren_start + 1..paren_end];

    // Parse line,column from position
    let pos_parts: Vec<&str> = position.split(',').collect();
    if pos_parts.len() != 2 {
        return None;
    }
    let line_num: u32 = pos_parts[0].trim().parse().ok()?;
    let column: u32 = pos_parts[1].trim().parse().ok()?;

    // The rest after "): " contains the diagnostic
    let rest = &line[paren_end + 1..];
    let rest = rest.trim_start_matches(':').trim();

    // Parse severity and code
    let (severity, rest) = if let Some(rest) = rest.strip_prefix("error") {
        (DiagnosticSeverity::Error, rest)
    } else if let Some(rest) = rest.strip_prefix("warning") {
        (DiagnosticSeverity::Warning, rest)
    } else {
        return None;
    };

    // Parse TS code
    let rest = rest.trim();
    let code_end = rest.find(':').unwrap_or(rest.len());
    let code = rest[..code_end].trim().to_string();
    let message = rest[code_end..].trim_start_matches(':').trim().to_string();

    // Map back to original file if needed
    let (original_file, original_line, original_column) =
        map_to_original(file_path, line_num, column, files);

    Some(TsgoDiagnostic {
        file: Utf8PathBuf::from(original_file),
        start: DiagnosticPosition {
            line: original_line,
            column: original_column,
            offset: 0, // Would need full source to calculate
        },
        end: DiagnosticPosition {
            line: original_line,
            column: original_column + 1,
            offset: 0,
        },
        message,
        code,
        severity,
    })
}

/// Maps a generated position back to the original source.
fn map_to_original(
    file_path: &str,
    line: u32,
    column: u32,
    files: &TransformedFiles,
) -> (String, u32, u32) {
    // Try to find the transformed file
    // The file_path may include temp directory prefixes (e.g., /tmp/.../src/App.svelte.ts)
    // We need to match against our virtual paths which are relative (e.g., src/App.svelte.ts)

    // Fast path: tsgo outputs absolute paths under node_modules/.cache/svelte-check-rs
    if let Some(rel) = strip_cache_prefix(file_path) {
        if let Some(file) = files.get(camino::Utf8Path::new(&rel)) {
            return do_source_mapping(file, line, column);
        }
    }

    // First try direct lookup
    let virtual_path = camino::Utf8Path::new(file_path);
    if let Some(file) = files.get(virtual_path) {
        return do_source_mapping(file, line, column);
    }

    // Try to match by suffix - the temp path might be /tmp/.../src/routes/file.svelte.ts
    // and we're looking for src/routes/file.svelte.ts
    for (key, file) in &files.files {
        // Check if the tsgo output path ends with our virtual path
        if file_path.ends_with(key.as_str()) {
            return do_source_mapping(file, line, column);
        }
    }

    // Try to match by filename as a last resort (for very short paths)
    // but only if there's exactly one match
    if let Some(file_name) = virtual_path.file_name() {
        let matches: Vec<_> = files
            .files
            .iter()
            .filter(|(key, _)| key.file_name() == Some(file_name))
            .collect();

        if matches.len() == 1 {
            return do_source_mapping(matches[0].1, line, column);
        }
    }

    if !virtual_path.is_absolute() {
        if let Some(cleaned) = normalize_relative_path(file_path) {
            return (cleaned, line, column);
        }
    }

    // File not in our transformed set, return as-is
    (file_path.to_string(), line, column)
}

/// Performs source map lookup to get original position.
fn do_source_mapping(
    file: &crate::runner::TransformedFile,
    line: u32,
    column: u32,
) -> (String, u32, u32) {
    // Convert line/column to byte offset (tsgo uses 1-indexed)
    if let Some(generated_offset) = file.generated_line_index.offset(LineCol {
        line: line.saturating_sub(1),
        col: column.saturating_sub(1),
    }) {
        // Try to map back using source map
        if let Some(original_offset) = file.source_map.original_position(generated_offset) {
            // Convert original byte offset back to line/column
            if let Some(original_line_col) = file.original_line_index.line_col(original_offset) {
                // Return 1-indexed line/column for tsgo format
                return (
                    file.original_path.to_string(),
                    original_line_col.line + 1,
                    original_line_col.col + 1,
                );
            }
        }
    }

    // Fallback: return original file but keep generated position
    (file.original_path.to_string(), line, column)
}

fn strip_cache_prefix(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let markers = [
        "/node_modules/.cache/svelte-check-rs/",
        "/.svelte-check-rs/cache/",
    ];
    for marker in markers {
        if let Some(idx) = normalized.find(marker) {
            let rel = &normalized[idx + marker.len()..];
            if !rel.is_empty() {
                return Some(rel.to_string());
            }
        }
    }
    None
}

fn normalize_relative_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !parts.is_empty() {
                    parts.pop();
                }
            }
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostic_line() {
        let line =
            "src/App.svelte.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files).unwrap();
        assert_eq!(diag.file.as_str(), "src/App.svelte.ts");
        assert_eq!(diag.start.line, 10);
        assert_eq!(diag.start.column, 5);
        assert_eq!(diag.code, "TS2322");
        assert!(diag.message.contains("Type"));
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn test_parse_empty_output() {
        let files = TransformedFiles::new();
        let diagnostics = parse_tsgo_output("", &files).unwrap();
        assert!(diagnostics.is_empty());
    }
}
