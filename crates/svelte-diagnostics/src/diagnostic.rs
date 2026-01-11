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
    /// `a11y-accesskey`: No accesskey attribute
    A11yAccesskey,
    /// `a11y-aria-activedescendant-has-tabindex`
    A11yAriaActivedescendantHasTabindex,
    /// `a11y-aria-attributes`: Valid aria-* attributes
    A11yAriaAttributes,
    /// `a11y-autofocus`: No autofocus
    A11yAutofocus,
    /// `a11y-click-events-have-key-events`
    A11yClickEventsHaveKeyEvents,
    /// `a11y-distracting-elements`: No marquee or blink
    A11yDistractingElements,
    /// `a11y-hidden`: No aria-hidden on focusable elements
    A11yHidden,
    /// `a11y-img-redundant-alt`
    A11yImgRedundantAlt,
    /// `a11y-incorrect-aria-attribute-type`
    A11yIncorrectAriaAttributeType,
    /// `a11y-interactive-supports-focus`
    A11yInteractiveSupportsFocus,
    /// `a11y-invalid-attribute`
    A11yInvalidAttribute,
    /// `a11y-label-has-associated-control`
    A11yLabelHasAssociatedControl,
    /// `a11y-media-has-caption`
    A11yMediaHasCaption,
    /// `a11y-missing-attribute`: Required attributes missing
    A11yMissingAttribute,
    /// `a11y-missing-content`: Anchors/headings need content
    A11yMissingContent,
    /// `a11y-mouse-events-have-key-events`
    A11yMouseEventsHaveKeyEvents,
    /// `a11y-no-noninteractive-element-interactions`
    A11yNoNoninteractiveElementInteractions,
    /// `a11y-no-noninteractive-element-to-interactive-role`
    A11yNoNoninteractiveElementToInteractiveRole,
    /// `a11y-no-noninteractive-tabindex`
    A11yNoNoninteractiveTabindex,
    /// `a11y-no-redundant-roles`
    A11yNoRedundantRoles,
    /// `a11y-no-static-element-interactions`
    A11yNoStaticElementInteractions,
    /// `a11y-positive-tabindex`
    A11yPositiveTabindex,
    /// `a11y-role-has-required-aria-props`
    A11yRoleHasRequiredAriaProps,
    /// `a11y-role-supports-aria-props`
    A11yRoleSupportsAriaProps,
    /// `a11y-structure`: Heading structure
    A11yStructure,

    // === CSS Codes ===
    /// `css-unused-selector`
    CssUnusedSelector,
    /// `css-invalid-global`
    CssInvalidGlobal,

    // === Component Codes ===
    /// `unused-export-let`
    UnusedExportLet,
    /// `missing-declaration`
    MissingDeclaration,
    /// `invalid-rune-usage`
    InvalidRuneUsage,
    /// `state-referenced-locally`
    StateReferencedLocally,

    // === Parse Codes ===
    /// `parse-error`: Syntax error during parsing
    ParseError,
}

impl DiagnosticCode {
    /// Returns the default severity for this diagnostic code.
    pub fn default_severity(&self) -> Severity {
        match self {
            // Errors
            DiagnosticCode::A11yMissingAttribute => Severity::Warning,
            DiagnosticCode::InvalidRuneUsage => Severity::Error,
            DiagnosticCode::MissingDeclaration => Severity::Error,
            DiagnosticCode::ParseError => Severity::Error,

            // Warnings (most a11y)
            DiagnosticCode::A11yAccesskey
            | DiagnosticCode::A11yAriaActivedescendantHasTabindex
            | DiagnosticCode::A11yAriaAttributes
            | DiagnosticCode::A11yAutofocus
            | DiagnosticCode::A11yClickEventsHaveKeyEvents
            | DiagnosticCode::A11yDistractingElements
            | DiagnosticCode::A11yHidden
            | DiagnosticCode::A11yImgRedundantAlt
            | DiagnosticCode::A11yIncorrectAriaAttributeType
            | DiagnosticCode::A11yInteractiveSupportsFocus
            | DiagnosticCode::A11yInvalidAttribute
            | DiagnosticCode::A11yLabelHasAssociatedControl
            | DiagnosticCode::A11yMediaHasCaption
            | DiagnosticCode::A11yMissingContent
            | DiagnosticCode::A11yMouseEventsHaveKeyEvents
            | DiagnosticCode::A11yNoNoninteractiveElementInteractions
            | DiagnosticCode::A11yNoNoninteractiveElementToInteractiveRole
            | DiagnosticCode::A11yNoNoninteractiveTabindex
            | DiagnosticCode::A11yNoRedundantRoles
            | DiagnosticCode::A11yNoStaticElementInteractions
            | DiagnosticCode::A11yPositiveTabindex
            | DiagnosticCode::A11yRoleHasRequiredAriaProps
            | DiagnosticCode::A11yRoleSupportsAriaProps
            | DiagnosticCode::A11yStructure => Severity::Warning,

            // CSS warnings
            DiagnosticCode::CssUnusedSelector => Severity::Warning,
            DiagnosticCode::CssInvalidGlobal => Severity::Warning,
            DiagnosticCode::StateReferencedLocally => Severity::Warning,

            // Component hints
            DiagnosticCode::UnusedExportLet => Severity::Hint,
        }
    }

    /// Returns the diagnostic code as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticCode::A11yAccesskey => "a11y-accesskey",
            DiagnosticCode::A11yAriaActivedescendantHasTabindex => {
                "a11y-aria-activedescendant-has-tabindex"
            }
            DiagnosticCode::A11yAriaAttributes => "a11y-aria-attributes",
            DiagnosticCode::A11yAutofocus => "a11y-autofocus",
            DiagnosticCode::A11yClickEventsHaveKeyEvents => "a11y-click-events-have-key-events",
            DiagnosticCode::A11yDistractingElements => "a11y-distracting-elements",
            DiagnosticCode::A11yHidden => "a11y-hidden",
            DiagnosticCode::A11yImgRedundantAlt => "a11y-img-redundant-alt",
            DiagnosticCode::A11yIncorrectAriaAttributeType => "a11y-incorrect-aria-attribute-type",
            DiagnosticCode::A11yInteractiveSupportsFocus => "a11y-interactive-supports-focus",
            DiagnosticCode::A11yInvalidAttribute => "a11y-invalid-attribute",
            DiagnosticCode::A11yLabelHasAssociatedControl => "a11y-label-has-associated-control",
            DiagnosticCode::A11yMediaHasCaption => "a11y-media-has-caption",
            DiagnosticCode::A11yMissingAttribute => "a11y-missing-attribute",
            DiagnosticCode::A11yMissingContent => "a11y-missing-content",
            DiagnosticCode::A11yMouseEventsHaveKeyEvents => "a11y-mouse-events-have-key-events",
            DiagnosticCode::A11yNoNoninteractiveElementInteractions => {
                "a11y-no-noninteractive-element-interactions"
            }
            DiagnosticCode::A11yNoNoninteractiveElementToInteractiveRole => {
                "a11y-no-noninteractive-element-to-interactive-role"
            }
            DiagnosticCode::A11yNoNoninteractiveTabindex => "a11y-no-noninteractive-tabindex",
            DiagnosticCode::A11yNoRedundantRoles => "a11y-no-redundant-roles",
            DiagnosticCode::A11yNoStaticElementInteractions => {
                "a11y-no-static-element-interactions"
            }
            DiagnosticCode::A11yPositiveTabindex => "a11y-positive-tabindex",
            DiagnosticCode::A11yRoleHasRequiredAriaProps => "a11y-role-has-required-aria-props",
            DiagnosticCode::A11yRoleSupportsAriaProps => "a11y-role-supports-aria-props",
            DiagnosticCode::A11yStructure => "a11y-structure",
            DiagnosticCode::CssUnusedSelector => "css-unused-selector",
            DiagnosticCode::CssInvalidGlobal => "css-invalid-global",
            DiagnosticCode::UnusedExportLet => "unused-export-let",
            DiagnosticCode::MissingDeclaration => "missing-declaration",
            DiagnosticCode::InvalidRuneUsage => "invalid-rune-usage",
            DiagnosticCode::StateReferencedLocally => "state-referenced-locally",
            DiagnosticCode::ParseError => "parse-error",
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
