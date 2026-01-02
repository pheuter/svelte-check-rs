//! Output formatting.

use crate::cli::OutputFormat;
use camino::Utf8Path;
use serde::Serialize;
use source_map::{LineCol, LineIndex};
use svelte_diagnostics::{Diagnostic, Severity};

/// A formatted diagnostic for output.
#[derive(Debug, Serialize)]
pub struct FormattedDiagnostic {
    /// The diagnostic type (Error, Warning, etc.).
    #[serde(rename = "type")]
    pub diagnostic_type: String,
    /// The file path.
    pub filename: String,
    /// The start position.
    pub start: Position,
    /// The end position.
    pub end: Position,
    /// The message.
    pub message: String,
    /// The diagnostic code.
    pub code: String,
    /// The source (svelte, ts, css).
    pub source: String,
}

/// A position in the source.
#[derive(Debug, Serialize)]
pub struct Position {
    /// 1-indexed line number.
    pub line: u32,
    /// 1-indexed column number.
    pub column: u32,
    /// Byte offset.
    pub offset: u32,
}

/// Formats diagnostics for output.
pub struct Formatter {
    format: OutputFormat,
}

impl Formatter {
    /// Creates a new formatter.
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Formats a collection of diagnostics.
    pub fn format(&self, diagnostics: &[Diagnostic], file_path: &Utf8Path, source: &str) -> String {
        match self.format {
            OutputFormat::Human => self.format_human(diagnostics, file_path, source),
            OutputFormat::HumanVerbose => self.format_human_verbose(diagnostics, file_path, source),
            OutputFormat::Json => self.format_json(diagnostics, file_path, source),
            OutputFormat::Machine => self.format_machine(diagnostics, file_path, source),
        }
    }

    /// Formats as human-readable output.
    fn format_human(
        &self,
        diagnostics: &[Diagnostic],
        file_path: &Utf8Path,
        source: &str,
    ) -> String {
        let line_index = LineIndex::new(source);
        let mut output = String::new();

        for diag in diagnostics {
            let start = line_index
                .line_col(diag.span.start)
                .unwrap_or(LineCol::new(0, 0));

            let severity = match diag.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Hint => "Hint",
            };

            output.push_str(&format!(
                "{}:{}:{}\n{}: {} ({})\n\n",
                file_path,
                start.line + 1,
                start.col + 1,
                severity,
                diag.message,
                diag.code
            ));
        }

        output
    }

    /// Formats as human-readable output with code snippets.
    fn format_human_verbose(
        &self,
        diagnostics: &[Diagnostic],
        file_path: &Utf8Path,
        source: &str,
    ) -> String {
        let line_index = LineIndex::new(source);
        let lines: Vec<&str> = source.lines().collect();
        let mut output = String::new();

        for diag in diagnostics {
            let start = line_index
                .line_col(diag.span.start)
                .unwrap_or(LineCol::new(0, 0));

            let severity = match diag.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Hint => "Hint",
            };

            output.push_str(&format!(
                "{}:{}:{}\n{}: {} ({})\n",
                file_path,
                start.line + 1,
                start.col + 1,
                severity,
                diag.message,
                diag.code
            ));

            // Add code snippet
            let line_num = start.line as usize;
            if line_num < lines.len() {
                output.push_str(&format!("  {} | {}\n", line_num + 1, lines[line_num]));

                // Add pointer
                let padding = " ".repeat(start.col as usize);
                output.push_str(&format!(
                    "  {} | {}^\n",
                    " ".repeat((line_num + 1).to_string().len()),
                    padding
                ));
            }

            output.push('\n');
        }

        output
    }

    /// Formats as JSON output.
    fn format_json(
        &self,
        diagnostics: &[Diagnostic],
        file_path: &Utf8Path,
        source: &str,
    ) -> String {
        let formatted = Self::format_json_diagnostics(diagnostics, file_path, source);
        serde_json::to_string_pretty(&formatted).unwrap_or_default()
    }

    /// Formats diagnostics into JSON-ready structs.
    pub fn format_json_diagnostics(
        diagnostics: &[Diagnostic],
        file_path: &Utf8Path,
        source: &str,
    ) -> Vec<FormattedDiagnostic> {
        let line_index = LineIndex::new(source);
        diagnostics
            .iter()
            .map(|diag| {
                let start = line_index
                    .line_col(diag.span.start)
                    .unwrap_or(LineCol::new(0, 0));
                let end = line_index
                    .line_col(diag.span.end)
                    .unwrap_or(LineCol::new(0, 0));

                FormattedDiagnostic {
                    diagnostic_type: match diag.severity {
                        Severity::Error => "Error".to_string(),
                        Severity::Warning => "Warning".to_string(),
                        Severity::Hint => "Hint".to_string(),
                    },
                    filename: file_path.to_string(),
                    start: Position {
                        line: start.line + 1,
                        column: start.col + 1,
                        offset: u32::from(diag.span.start),
                    },
                    end: Position {
                        line: end.line + 1,
                        column: end.col + 1,
                        offset: u32::from(diag.span.end),
                    },
                    message: diag.message.clone(),
                    code: diag.code.to_string(),
                    source: "svelte".to_string(),
                }
            })
            .collect()
    }

    /// Formats as machine-readable output.
    fn format_machine(
        &self,
        diagnostics: &[Diagnostic],
        file_path: &Utf8Path,
        source: &str,
    ) -> String {
        let line_index = LineIndex::new(source);
        let mut output = String::new();

        for diag in diagnostics {
            let start = line_index
                .line_col(diag.span.start)
                .unwrap_or(LineCol::new(0, 0));
            let end = line_index
                .line_col(diag.span.end)
                .unwrap_or(LineCol::new(0, 0));

            let severity = match diag.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARNING",
                Severity::Hint => "HINT",
            };

            output.push_str(&format!(
                "{} {}:{}:{}:{}:{} {} ({})\n",
                severity,
                file_path,
                start.line + 1,
                start.col + 1,
                end.line + 1,
                end.col + 1,
                diag.message,
                diag.code
            ));
        }

        output
    }
}

/// Summary of a check run.
#[derive(Debug, Default)]
pub struct CheckSummary {
    /// Number of files checked.
    pub file_count: usize,
    /// Number of errors.
    pub error_count: usize,
    /// Number of warnings.
    pub warning_count: usize,
    /// Whether to fail on warnings.
    pub fail_on_warnings: bool,
}

impl CheckSummary {
    /// Formats the summary line.
    pub fn format(&self) -> String {
        let error_word = if self.error_count == 1 {
            "error"
        } else {
            "errors"
        };
        let warning_word = if self.warning_count == 1 {
            "warning"
        } else {
            "warnings"
        };
        let file_word = if self.file_count == 1 {
            "file"
        } else {
            "files"
        };

        format!(
            "====================================\nsvelte-check found {} {} and {} {} in {} {}",
            self.error_count,
            error_word,
            self.warning_count,
            warning_word,
            self.file_count,
            file_word
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use source_map::Span;
    use svelte_diagnostics::DiagnosticCode;
    use text_size::TextSize;

    #[test]
    fn test_format_human() {
        let formatter = Formatter::new(OutputFormat::Human);
        let diag = Diagnostic::new(
            DiagnosticCode::A11yMissingAttribute,
            "Missing alt attribute",
            Span::new(TextSize::from(0), TextSize::from(5)),
        );

        let output = formatter.format(&[diag], Utf8Path::new("test.svelte"), "<img>");
        assert!(output.contains("test.svelte:1:1"));
        assert!(output.contains("Missing alt"));
    }

    #[test]
    fn test_format_json() {
        let formatter = Formatter::new(OutputFormat::Json);
        let diag = Diagnostic::new(
            DiagnosticCode::A11yMissingAttribute,
            "Missing alt attribute",
            Span::new(TextSize::from(0), TextSize::from(5)),
        );

        let output = formatter.format(&[diag], Utf8Path::new("test.svelte"), "<img>");
        assert!(output.contains("\"filename\""));
        assert!(output.contains("test.svelte"));
    }

    #[test]
    fn test_summary() {
        let summary = CheckSummary {
            file_count: 5,
            error_count: 2,
            warning_count: 3,
            fail_on_warnings: false,
        };

        let output = summary.format();
        assert!(output.contains("2 errors"));
        assert!(output.contains("3 warnings"));
        assert!(output.contains("5 files"));
    }
}
