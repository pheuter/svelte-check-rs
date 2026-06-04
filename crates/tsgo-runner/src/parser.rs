//! tsgo output parser.

use crate::runner::{TransformedFiles, TsgoError};
use camino::{Utf8Path, Utf8PathBuf};
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
    /// Whether this diagnostic has no source position (e.g. tsconfig
    /// options/global diagnostics like `error TS2318: ...`). When true, the
    /// diagnostic is attributed to the resolved tsconfig path and printed at
    /// line/column 0 with no source snippet (parity with upstream
    /// `writers.ts` `positionUnknown`).
    #[serde(default)]
    pub position_unknown: bool,
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
///
/// `tsconfig_path` is the resolved tsconfig used to attribute positionless
/// diagnostics (options/global errors that tsc prints without a
/// `file(line,col):` prefix), mirroring upstream's `mapCliDiagnosticsToLsp`
/// `tsconfigPath` argument.
pub fn parse_tsgo_output(
    output: &str,
    files: &TransformedFiles,
    tsconfig_path: &Utf8Path,
) -> Result<Vec<TsgoDiagnostic>, TsgoError> {
    let mut diagnostics = Vec::new();

    // tsgo outputs diagnostics in the format:
    // file.ts:line:column - error TS1234: message
    for line in output.lines() {
        if let Some(diag) = parse_diagnostic_line(line, files, tsconfig_path) {
            diagnostics.push(diag);
        }
    }

    Ok(diagnostics)
}

/// Parses a single diagnostic line.
fn parse_diagnostic_line(
    line: &str,
    files: &TransformedFiles,
    tsconfig_path: &Utf8Path,
) -> Option<TsgoDiagnostic> {
    // tsgo outputs: file.tsx(line,column): error TS1234: message
    // We need to parse this format

    // Find the diagnostic position suffix. Paths can contain parentheses in
    // SvelteKit route groups, e.g. `src/routes/(app)/+page.server.ts(10,5)`.
    let positioned = line.match_indices("):").find_map(|(candidate_end, _)| {
        let rest = line[candidate_end + 2..].trim_start();
        if !(rest.starts_with("error") || rest.starts_with("warning")) {
            return None;
        }
        line[..candidate_end]
            .rfind('(')
            .map(|candidate_start| (candidate_start, candidate_end))
    });

    // No `file(line,col):` prefix: this may be a positionless options/global
    // diagnostic (e.g. `error TS2318: Cannot find global type 'Array'.`).
    let (paren_start, paren_end) = match positioned {
        Some(pos) => pos,
        None => return parse_positionless_diagnostic_line(line, tsconfig_path),
    };

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
        position_unknown: false,
    })
}

/// Parses a positionless tsc pretty line of the form
/// `<error|warning> TS<code>: <message>` (no `file(line,col):` prefix).
///
/// These are options/global diagnostics (e.g. `error TS2318: Cannot find
/// global type 'Array'.` emitted when `lib` is invalid). They are attributed
/// to the resolved tsconfig path with a zero/unknown position so the
/// orchestrator can surface them without a source snippet.
fn parse_positionless_diagnostic_line(
    line: &str,
    tsconfig_path: &Utf8Path,
) -> Option<TsgoDiagnostic> {
    let trimmed = line.trim();

    let (severity, rest) = if let Some(rest) = trimmed.strip_prefix("error ") {
        (DiagnosticSeverity::Error, rest)
    } else if let Some(rest) = trimmed.strip_prefix("warning ") {
        (DiagnosticSeverity::Warning, rest)
    } else {
        return None;
    };

    // Require a `TS<code>:` token so we don't swallow arbitrary text.
    let rest = rest.trim_start();
    let code_end = rest.find(':')?;
    let code = rest[..code_end].trim();
    if !code.starts_with("TS") || code.len() < 3 || !code[2..].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let message = rest[code_end + 1..].trim().to_string();
    if message.is_empty() {
        return None;
    }

    let zero = DiagnosticPosition {
        line: 0,
        column: 0,
        offset: 0,
    };

    Some(TsgoDiagnostic {
        file: tsconfig_path.to_owned(),
        start: zero.clone(),
        end: zero,
        message,
        code: code.to_string(),
        severity,
        position_unknown: true,
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
                if marker == "/node_modules/.cache/svelte-check-rs/" {
                    if let Some((_, project_rel)) = rel.split_once('/') {
                        if !project_rel.is_empty() {
                            return Some(project_rel.to_string());
                        }
                    }
                }
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

    const DUMMY_TSCONFIG: &str = "/repo/tsconfig.json";

    #[test]
    fn test_parse_diagnostic_line() {
        let line =
            "src/App.svelte.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files, Utf8Path::new(DUMMY_TSCONFIG)).unwrap();
        assert_eq!(diag.file.as_str(), "src/App.svelte.ts");
        assert_eq!(diag.start.line, 10);
        assert_eq!(diag.start.column, 5);
        assert_eq!(diag.code, "TS2322");
        assert!(diag.message.contains("Type"));
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert!(!diag.position_unknown);
    }

    #[test]
    fn test_parse_diagnostic_line_with_parentheses_in_path() {
        let line = "src/routes/(app)/imports/+page.server.ts(87,5): error TS2322: Type 'string' is not assignable to type 'number'";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files, Utf8Path::new(DUMMY_TSCONFIG)).unwrap();
        assert_eq!(
            diag.file.as_str(),
            "src/routes/(app)/imports/+page.server.ts"
        );
        assert_eq!(diag.start.line, 87);
        assert_eq!(diag.start.column, 5);
        assert_eq!(diag.code, "TS2322");
        assert!(diag.message.contains("Type"));
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert!(!diag.position_unknown);
    }

    #[test]
    fn test_parse_positionless_global_diagnostic() {
        let line = "error TS2318: Cannot find global type 'Array'.";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files, Utf8Path::new(DUMMY_TSCONFIG)).unwrap();
        assert_eq!(diag.file.as_str(), DUMMY_TSCONFIG);
        assert_eq!(diag.start.line, 0);
        assert_eq!(diag.start.column, 0);
        assert_eq!(diag.end.line, 0);
        assert_eq!(diag.code, "TS2318");
        assert!(diag.message.contains("Cannot find global type"));
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert!(diag.position_unknown);
    }

    #[test]
    fn test_parse_positionless_options_diagnostic() {
        // The real TS6046 message is enormous; a representative prefix is fine.
        let line = "error TS6046: Argument for '--lib' option must be: 'es5', 'es6'.";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files, Utf8Path::new(DUMMY_TSCONFIG)).unwrap();
        assert_eq!(diag.file.as_str(), DUMMY_TSCONFIG);
        assert_eq!(diag.code, "TS6046");
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert!(diag.position_unknown);
    }

    #[test]
    fn test_parse_positionless_warning_diagnostic() {
        let line = "warning TS5102: Option 'foo' has been removed.";
        let files = TransformedFiles::new();

        let diag = parse_diagnostic_line(line, &files, Utf8Path::new(DUMMY_TSCONFIG)).unwrap();
        assert_eq!(diag.code, "TS5102");
        assert_eq!(diag.severity, DiagnosticSeverity::Warning);
        assert!(diag.position_unknown);
    }

    #[test]
    fn test_non_diagnostic_line_returns_none() {
        let files = TransformedFiles::new();
        // Summary lines and arbitrary text must not be parsed as diagnostics.
        assert!(
            parse_diagnostic_line("Found 3 errors.", &files, Utf8Path::new(DUMMY_TSCONFIG))
                .is_none()
        );
        assert!(parse_diagnostic_line(
            "error something not a code",
            &files,
            Utf8Path::new(DUMMY_TSCONFIG)
        )
        .is_none());
        assert!(parse_diagnostic_line("", &files, Utf8Path::new(DUMMY_TSCONFIG)).is_none());
    }

    #[test]
    fn test_parse_empty_output() {
        let files = TransformedFiles::new();
        let diagnostics = parse_tsgo_output("", &files, Utf8Path::new(DUMMY_TSCONFIG)).unwrap();
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_strip_cache_prefix_skips_project_namespace() {
        let path = "/repo/node_modules/.cache/svelte-check-rs/abcdef/src/App.svelte.ts";
        assert_eq!(
            strip_cache_prefix(path),
            Some("src/App.svelte.ts".to_string())
        );
    }
}
