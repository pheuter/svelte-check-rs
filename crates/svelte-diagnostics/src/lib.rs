//! Svelte-specific diagnostics for svelte-check-rs.
//!
//! This crate provides diagnostics for:
//! - Accessibility (a11y) checks
//! - CSS validation (unused selectors, invalid :global())
//! - Component validation (invalid rune usage, naming conventions)
//!
//! # Example
//!
//! ```
//! use svelte_parser::parse;
//! use svelte_diagnostics::{check, DiagnosticOptions};
//!
//! let source = r#"<img src="photo.jpg">"#;
//! let doc = parse(source);
//! let diagnostics = check(&doc.document, DiagnosticOptions::default());
//!
//! for diagnostic in diagnostics {
//!     println!("{}: {}", diagnostic.code, diagnostic.message);
//! }
//! ```

pub mod a11y;
pub mod component;
pub mod css;
mod diagnostic;

pub use component::ComponentCheckOptions;
pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};

use svelte_parser::SvelteDocument;

/// Options for diagnostic checking.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticOptions {
    /// Whether to run a11y checks.
    pub a11y: bool,
    /// Whether to run CSS checks.
    pub css: bool,
    /// Whether to run component checks.
    pub component: bool,
    /// The filename of the component (for naming checks).
    pub filename: Option<String>,
}

impl DiagnosticOptions {
    /// Returns options with all checks enabled.
    pub fn all() -> Self {
        Self {
            a11y: true,
            css: true,
            component: true,
            filename: None,
        }
    }

    /// Sets the filename for component checks.
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
}

/// Runs all enabled diagnostic checks on a Svelte document.
pub fn check(doc: &SvelteDocument, options: DiagnosticOptions) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if options.a11y {
        diagnostics.extend(a11y::check(doc));
    }

    if options.css {
        diagnostics.extend(css::check(doc));
    }

    if options.component {
        let component_options = ComponentCheckOptions {
            filename: options.filename.clone(),
        };
        diagnostics.extend(component::check(doc, &component_options));
    }

    // Sort by position
    diagnostics.sort_by_key(|d| d.span.start);

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_check_empty_document() {
        let doc = parse("").document;
        let diagnostics = check(&doc, DiagnosticOptions::all());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_with_a11y_issue() {
        let doc = parse(r#"<img src="photo.jpg">"#).document;
        let diagnostics = check(
            &doc,
            DiagnosticOptions {
                a11y: true,
                ..Default::default()
            },
        );

        assert!(!diagnostics.is_empty());
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yMissingAttribute)));
    }
}
