//! CSS diagnostics.
//!
//! This module provides CSS-related checks:
//! - Invalid :global() usage

use crate::Diagnostic;
use svelte_parser::SvelteDocument;

/// Runs CSS checks on a document.
pub fn check(_doc: &SvelteDocument) -> Vec<Diagnostic> {
    // CSS checking would require parsing the CSS and validating :global() usage.
    // This is a placeholder for future implementation.

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_no_style() {
        let doc = parse("<div>hello</div>").document;
        let diagnostics = check(&doc);
        assert!(diagnostics.is_empty());
    }
}
