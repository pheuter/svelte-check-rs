//! tsgo output parser.

use crate::runner::{TransformedFiles, TsgoError};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use source_map::{LineCol, LineIndex};

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
    // Format: file.ts:line:column - error TS1234: message
    let parts: Vec<&str> = line.splitn(2, " - ").collect();
    if parts.len() != 2 {
        return None;
    }

    let location = parts[0];
    let message_part = parts[1];

    // Parse location (file:line:column)
    let loc_parts: Vec<&str> = location.rsplitn(3, ':').collect();
    if loc_parts.len() < 3 {
        return None;
    }

    let column: u32 = loc_parts[0].parse().ok()?;
    let line_num: u32 = loc_parts[1].parse().ok()?;
    let file_path = loc_parts[2..].join(":");

    // Parse severity and code
    let (severity, rest) = if let Some(rest) = message_part.strip_prefix("error") {
        (DiagnosticSeverity::Error, rest)
    } else if let Some(rest) = message_part.strip_prefix("warning") {
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
        map_to_original(&file_path, line_num, column, files);

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
    let virtual_path = camino::Utf8Path::new(file_path);

    if let Some(file) = files.get(virtual_path) {
        // Create a line index for the generated content
        let line_index = LineIndex::new(&file.tsx_content);

        // Convert line/column to byte offset
        if let Some(offset) = line_index.offset(LineCol {
            line: line.saturating_sub(1),
            col: column.saturating_sub(1),
        }) {
            // Try to map back using source map
            if let Some(_original_offset) = file.source_map.original_position(offset) {
                // We would need the original source to convert back to line/col
                // For now, return the generated position
                return (file.original_path.to_string(), line, column);
            }
        }

        return (file.original_path.to_string(), line, column);
    }

    // File not in our transformed set, return as-is
    (file_path.to_string(), line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostic_line() {
        let line = "src/App.svelte.tsx:10:5 - error TS2322: Type 'string' is not assignable to type 'number'";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files).unwrap();
        assert_eq!(diag.file.as_str(), "src/App.svelte.tsx");
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
