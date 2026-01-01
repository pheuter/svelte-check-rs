//! Component diagnostics.
//!
//! This module provides component-level checks:
//! - Invalid rune usage
//! - Component naming conventions
//! - Missing declarations

use crate::Diagnostic;
use svelte_parser::SvelteDocument;

/// Runs component checks on a document.
pub fn check(_doc: &SvelteDocument) -> Vec<Diagnostic> {
    // Component checking would require analyzing the script content
    // for rune usage and prop declarations. This is a placeholder for
    // future implementation.

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_empty_component() {
        let doc = parse("").document;
        let diagnostics = check(&doc);
        assert!(diagnostics.is_empty());
    }
}
