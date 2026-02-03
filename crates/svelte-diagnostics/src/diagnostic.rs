//! Diagnostic types.

use source_map::Span;

/// A diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The diagnostic code.
    pub code: DiagnosticCode,
    /// The severity level.
    pub severity: Severity,
    /// The diagnostic message.
    pub message: String,
    /// The source location.
    pub span: Span,
    /// Optional suggestions for fixing the issue.
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    /// Creates a new diagnostic.
    pub fn new(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: code.default_severity(),
            code,
            message: message.into(),
            span,
            suggestions: Vec::new(),
        }
    }

    /// Adds a suggestion to this diagnostic.
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }
}

/// The severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// A hint or suggestion.
    Hint,
    /// A warning that doesn't prevent compilation.
    Warning,
    /// An error that should be fixed.
    Error,
}

/// A suggestion for fixing a diagnostic.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// A description of the fix.
    pub message: String,
    /// The text to replace with.
    pub replacement: String,
    /// The span to replace.
    pub span: Span,
}

/// Diagnostic codes for all checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    // === A11y Codes ===
    /// `a11y-structure`: Heading structure
    A11yStructure,

    // === CSS Codes ===
    /// `css-invalid-global`
    CssInvalidGlobal,

    // === Component Codes ===
    /// `missing-declaration`
    MissingDeclaration,
    /// `invalid-rune-usage`
    InvalidRuneUsage,

    // === Parse Codes ===
    /// `parse-error`: Syntax error during parsing
    ParseError,
}

impl DiagnosticCode {
    /// Returns the default severity for this diagnostic code.
    pub fn default_severity(&self) -> Severity {
        match self {
            DiagnosticCode::InvalidRuneUsage => Severity::Error,
            DiagnosticCode::MissingDeclaration => Severity::Error,
            DiagnosticCode::ParseError => Severity::Error,

            DiagnosticCode::A11yStructure | DiagnosticCode::CssInvalidGlobal => Severity::Warning,
        }
    }

    /// Returns the diagnostic code as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticCode::A11yStructure => "a11y-structure",
            DiagnosticCode::CssInvalidGlobal => "css-invalid-global",
            DiagnosticCode::MissingDeclaration => "missing-declaration",
            DiagnosticCode::InvalidRuneUsage => "invalid-rune-usage",
            DiagnosticCode::ParseError => "parse-error",
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
