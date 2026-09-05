//! Output formatting.

use crate::cli::{ColorMode, OutputFormat};
use camino::Utf8Path;
use serde::Serialize;
use source_map::{LineCol, LineIndex};
use std::io::IsTerminal;
use svelte_diagnostics::{Diagnostic, Severity};

/// Shared color policy for internal, compiler, and TypeScript diagnostics.
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorPolicy {
    enabled: bool,
    orange: bool,
}

impl ColorPolicy {
    /// Detect colors once for a check run. Structured formats always stay plain.
    pub fn detect(format: OutputFormat, mode: ColorMode) -> Self {
        Self::resolve(
            format,
            mode,
            std::io::stdout().is_terminal(),
            std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()),
            &std::env::var("TERM").unwrap_or_default(),
            &std::env::var("COLORTERM").unwrap_or_default(),
        )
    }

    fn resolve(
        format: OutputFormat,
        mode: ColorMode,
        terminal: bool,
        no_color: bool,
        term: &str,
        colorterm: &str,
    ) -> Self {
        let enabled = matches!(format, OutputFormat::Human | OutputFormat::HumanVerbose)
            && !no_color
            && match mode {
                ColorMode::Always => true,
                ColorMode::Never => false,
                ColorMode::Auto => terminal && term != "dumb",
            };
        Self {
            enabled,
            orange: term.contains("256color") || matches!(colorterm, "truecolor" | "24bit"),
        }
    }

    fn paint(self, text: &str, code: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    /// Gray file path in human output.
    pub fn path(self, path: &str) -> String {
        self.paint(path, "90")
    }

    /// Color a diagnostic severity consistently across all diagnostic sources.
    pub fn severity(self, severity: &str) -> String {
        self.paint(
            severity,
            match severity {
                "Error" => "31",
                "Warning" if self.orange => "38;5;208",
                "Warning" => "33",
                _ => "36",
            },
        )
    }
}

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
    colors: ColorPolicy,
}

impl Formatter {
    /// Creates a new formatter.
    pub fn new(format: OutputFormat, colors: ColorPolicy) -> Self {
        Self { format, colors }
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
                .utf16_line_col(diag.span.start)
                .unwrap_or(LineCol::new(0, 0));

            let severity = match diag.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Hint => "Hint",
            };

            output.push_str(&format!(
                "{}:{}:{}\n{}: {} ({})\n\n",
                self.colors.path(file_path.as_str()),
                start.line + 1,
                start.col + 1,
                self.colors.severity(severity),
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
                .utf16_line_col(diag.span.start)
                .unwrap_or(LineCol::new(0, 0));

            let severity = match diag.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Hint => "Hint",
            };

            output.push_str(&format!(
                "{}:{}:{}\n{}: {} ({})\n",
                self.colors.path(file_path.as_str()),
                start.line + 1,
                start.col + 1,
                self.colors.severity(severity),
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
                    .utf16_line_col(diag.span.start)
                    .unwrap_or(LineCol::new(0, 0));
                let end = line_index
                    .utf16_line_col(diag.span.end)
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
                .utf16_line_col(diag.span.start)
                .unwrap_or(LineCol::new(0, 0));
            let end = line_index
                .utf16_line_col(diag.span.end)
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
    /// Number of files with diagnostics.
    pub file_count: usize,
    /// Number of errors.
    pub error_count: usize,
    /// Number of warnings.
    pub warning_count: usize,
    /// Whether to fail on warnings.
    pub fail_on_warnings: bool,
}

impl CheckSummary {
    /// Formats the summary using the same severity palette as diagnostics.
    pub fn format_with_color(&self, colors: ColorPolicy) -> String {
        let code = if self.error_count > 0 {
            "31"
        } else if self.warning_count > 0 {
            if colors.orange {
                "38;5;208"
            } else {
                "33"
            }
        } else {
            "32"
        };
        colors.paint(&self.format(), code)
    }

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

        if self.file_count == 0 {
            format!(
                "svelte-check-rs found {} {} and {} {}",
                self.error_count, error_word, self.warning_count, warning_word
            )
        } else {
            let file_word = if self.file_count == 1 {
                "file"
            } else {
                "files"
            };
            format!(
                "====================================\nsvelte-check-rs found {} {} and {} {} in {} {}",
                self.error_count,
                error_word,
                self.warning_count,
                warning_word,
                self.file_count,
                file_word
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use source_map::Span;
    use svelte_diagnostics::DiagnosticCode;
    use text_size::TextSize;

    #[test]
    fn color_policy_respects_terminal_format_and_overrides() {
        let policy = |format, mode, tty, no_color, term| {
            ColorPolicy::resolve(format, mode, tty, no_color, term, "")
        };
        assert!(
            !policy(
                OutputFormat::Human,
                ColorMode::Auto,
                false,
                false,
                "xterm-256color"
            )
            .enabled
        );
        assert!(
            policy(
                OutputFormat::Human,
                ColorMode::Auto,
                true,
                false,
                "xterm-256color"
            )
            .enabled
        );
        assert!(!policy(OutputFormat::Human, ColorMode::Auto, true, false, "dumb").enabled);
        assert!(!policy(OutputFormat::Human, ColorMode::Never, true, false, "xterm").enabled);
        assert!(!policy(OutputFormat::Human, ColorMode::Always, true, true, "xterm").enabled);
        for format in [OutputFormat::Json, OutputFormat::Machine] {
            assert!(!policy(format, ColorMode::Always, true, false, "xterm").enabled);
        }
        let orange = policy(
            OutputFormat::HumanVerbose,
            ColorMode::Always,
            false,
            false,
            "xterm-256color",
        );
        assert_eq!(orange.severity("Warning"), "\x1b[38;5;208mWarning\x1b[0m");
        let basic = policy(
            OutputFormat::Human,
            ColorMode::Always,
            false,
            false,
            "xterm",
        );
        assert_eq!(basic.severity("Warning"), "\x1b[33mWarning\x1b[0m");
        assert_eq!(basic.severity("Error"), "\x1b[31mError\x1b[0m");
        assert_eq!(basic.path("App.svelte"), "\x1b[90mApp.svelte\x1b[0m");
    }

    #[test]
    fn test_format_human() {
        let formatter = Formatter::new(OutputFormat::Human, ColorPolicy::default());
        let diag = Diagnostic::new(
            DiagnosticCode::A11yStructure,
            "Skipped heading level",
            Span::new(TextSize::from(0), TextSize::from(5)),
        );

        let output = formatter.format(&[diag], Utf8Path::new("test.svelte"), "<img>");
        assert!(output.contains("test.svelte:1:1"));
        assert!(output.contains("Skipped heading level"));
    }

    #[test]
    fn test_format_json() {
        let formatter = Formatter::new(OutputFormat::Json, ColorPolicy::default());
        let diag = Diagnostic::new(
            DiagnosticCode::A11yStructure,
            "Skipped heading level",
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
        assert!(output.contains("===="));
        assert!(output.contains("2 errors"));
        assert!(output.contains("3 warnings"));
        assert!(output.contains("in 5 files"));
    }

    #[test]
    fn test_summary_clean_omits_divider_and_file_count() {
        // Matches language-tools svelte-check: when there are no files with
        // problems, drop both the divider line and the "in N files" suffix.
        let summary = CheckSummary {
            file_count: 0,
            error_count: 0,
            warning_count: 0,
            fail_on_warnings: false,
        };

        let output = summary.format();
        assert_eq!(output, "svelte-check-rs found 0 errors and 0 warnings");
    }

    #[test]
    fn test_summary_single_file() {
        let summary = CheckSummary {
            file_count: 1,
            error_count: 1,
            warning_count: 0,
            fail_on_warnings: false,
        };

        let output = summary.format();
        assert!(output.contains("===="));
        assert!(output.contains("1 error "));
        assert!(output.contains("in 1 file"));
    }
}
