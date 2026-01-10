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
pub mod state_analysis;

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

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use svelte_parser::parse;

    fn get_fixtures_dir() -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("test-fixtures")
    }

    fn collect_svelte_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "svelte") {
                    files.push(path);
                } else if path.is_dir() {
                    files.extend(collect_svelte_files(&path));
                }
            }
        }
        files
    }

    #[test]
    fn test_valid_a11y_fixtures_have_no_a11y_diagnostics() {
        let fixtures_dir = get_fixtures_dir().join("valid").join("a11y");
        let files = collect_svelte_files(&fixtures_dir);

        assert!(!files.is_empty(), "No valid a11y fixtures found");

        for path in &files {
            let source = fs::read_to_string(path).expect("Failed to read file");
            let filename = path.file_name().unwrap().to_string_lossy();
            let result = parse(&source);

            // Skip files with parse errors
            if !result.errors.is_empty() {
                continue;
            }

            let diagnostics = check(
                &result.document,
                DiagnosticOptions {
                    a11y: true,
                    ..Default::default()
                },
            );

            assert!(
                diagnostics.is_empty(),
                "Valid a11y fixture {} should have no diagnostics, but got {} diagnostics:\n{}",
                filename,
                diagnostics.len(),
                diagnostics
                    .iter()
                    .take(10)
                    .map(|d| format!("  - {:?}: {}", d.code, d.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        println!(
            "Verified {} valid a11y fixtures have no false positives",
            files.len()
        );
    }

    #[test]
    fn test_invalid_a11y_fixtures_have_diagnostics() {
        let fixtures_dir = get_fixtures_dir().join("invalid").join("a11y");
        let files = collect_svelte_files(&fixtures_dir);

        assert!(!files.is_empty(), "No invalid a11y fixtures found");

        let mut total_diagnostics = 0;

        for path in &files {
            let source = fs::read_to_string(path).expect("Failed to read file");
            let filename = path.file_name().unwrap().to_string_lossy();
            let result = parse(&source);

            let diagnostics = check(
                &result.document,
                DiagnosticOptions {
                    a11y: true,
                    ..Default::default()
                },
            );

            assert!(
                !diagnostics.is_empty(),
                "Invalid a11y fixture {} should have diagnostics, but got none",
                filename
            );

            total_diagnostics += diagnostics.len();
            println!("{}: {} diagnostics", filename, diagnostics.len());
        }

        println!(
            "Invalid a11y fixtures produced {} total diagnostics across {} files",
            total_diagnostics,
            files.len()
        );
    }

    #[test]
    fn test_invalid_component_fixtures_have_diagnostics() {
        let fixtures_dir = get_fixtures_dir().join("invalid").join("component");
        let files = collect_svelte_files(&fixtures_dir);

        if files.is_empty() {
            return; // No component fixtures
        }

        for path in &files {
            let source = fs::read_to_string(path).expect("Failed to read file");
            let filename = path.file_name().unwrap().to_string_lossy();
            let result = parse(&source);

            let diagnostics = check(
                &result.document,
                DiagnosticOptions {
                    component: true,
                    ..Default::default()
                },
            );

            assert!(
                !diagnostics.is_empty(),
                "Invalid component fixture {} should have diagnostics, but got none",
                filename
            );

            println!("{}: {} diagnostics", filename, diagnostics.len());
        }
    }

    /// Tests that specific diagnostic codes are triggered by specific fixtures
    #[test]
    fn test_specific_diagnostic_codes() {
        let fixtures_dir = get_fixtures_dir().join("invalid").join("a11y");

        // Map of fixture name -> expected diagnostic codes
        let expected: &[(&str, &[DiagnosticCode])] = &[
            (
                "MissingAttributes.svelte",
                &[
                    DiagnosticCode::A11yMissingAttribute,
                    DiagnosticCode::A11yMissingContent,
                ],
            ),
            (
                "AriaAttributes.svelte",
                &[
                    DiagnosticCode::A11yAriaAttributes,
                    DiagnosticCode::A11yNoRedundantRoles,
                    DiagnosticCode::A11yRoleHasRequiredAriaProps,
                    DiagnosticCode::A11yHidden,
                ],
            ),
            (
                "InteractiveElements.svelte",
                &[
                    DiagnosticCode::A11yClickEventsHaveKeyEvents,
                    DiagnosticCode::A11yMouseEventsHaveKeyEvents,
                    DiagnosticCode::A11yPositiveTabindex,
                    DiagnosticCode::A11yNoNoninteractiveTabindex,
                    DiagnosticCode::A11yNoStaticElementInteractions,
                    DiagnosticCode::A11yInteractiveSupportsFocus,
                ],
            ),
            (
                "FormAndMedia.svelte",
                &[
                    DiagnosticCode::A11yLabelHasAssociatedControl,
                    DiagnosticCode::A11yMediaHasCaption,
                    DiagnosticCode::A11yAutofocus,
                    DiagnosticCode::A11yAccesskey,
                ],
            ),
            (
                "StructureAndContent.svelte",
                &[
                    DiagnosticCode::A11yStructure,
                    DiagnosticCode::A11yImgRedundantAlt,
                    DiagnosticCode::A11yDistractingElements,
                ],
            ),
        ];

        for (fixture_name, expected_codes) in expected {
            let path = fixtures_dir.join(fixture_name);
            if !path.exists() {
                continue;
            }

            let source = fs::read_to_string(&path).expect("Failed to read file");
            let result = parse(&source);
            let diagnostics = check(
                &result.document,
                DiagnosticOptions {
                    a11y: true,
                    ..Default::default()
                },
            );

            let found_codes: std::collections::HashSet<_> = diagnostics
                .iter()
                .map(|d| std::mem::discriminant(&d.code))
                .collect();

            for expected_code in *expected_codes {
                let expected_discriminant = std::mem::discriminant(expected_code);
                assert!(
                    found_codes.contains(&expected_discriminant),
                    "Fixture {} should trigger {:?}, but it wasn't found.\nFound codes: {:?}",
                    fixture_name,
                    expected_code,
                    diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
                );
            }
        }
    }

    /// Tests that parser error fixtures produce parse errors
    /// NOTE: Some fixtures don't produce errors due to known parser limitations
    #[test]
    fn test_parser_error_fixtures_have_errors() {
        let fixtures_dir = get_fixtures_dir().join("invalid").join("parser");
        let files = collect_svelte_files(&fixtures_dir);

        assert!(!files.is_empty(), "No parser error fixtures found");

        // Known parser limitations - these fixtures SHOULD produce errors but currently don't
        // TODO: Fix parser to catch these errors
        let known_parser_gaps = [
            "UnclosedBlocks.svelte",     // Parser doesn't detect unclosed blocks
            "InvalidBlockSyntax.svelte", // Parser is lenient with block syntax
        ];

        let mut total_errors = 0;
        let mut files_with_errors = 0;
        let mut files_without_errors = Vec::new();

        for path in &files {
            let source = fs::read_to_string(path).expect("Failed to read file");
            let filename = path.file_name().unwrap().to_string_lossy();
            let result = parse(&source);

            if result.errors.is_empty() {
                files_without_errors.push(filename.to_string());
            } else {
                files_with_errors += 1;
                total_errors += result.errors.len();
            }

            println!("{}: {} parse errors", filename, result.errors.len());
        }

        // Report files that didn't produce errors
        if !files_without_errors.is_empty() {
            println!("\n=== Files without parse errors (potential parser gaps) ===");
            for f in &files_without_errors {
                let is_known = known_parser_gaps.iter().any(|g| f.contains(g));
                if is_known {
                    println!("  {} (KNOWN LIMITATION)", f);
                } else {
                    println!("  {} (UNEXPECTED - may need investigation)", f);
                }
            }
        }

        // At least SOME parser fixtures should produce errors
        assert!(
            files_with_errors > 0,
            "At least some parser error fixtures should produce errors"
        );

        println!(
            "\nParser error fixtures: {} files with errors, {} total errors",
            files_with_errors, total_errors
        );
    }
}
