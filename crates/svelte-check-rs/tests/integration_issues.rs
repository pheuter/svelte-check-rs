//! Integration tests for issues #19, #20, #21.
//!
//! These tests verify that:
//! - Issue #21: Imports with colons (like `virtual:pwa-register`) don't cause parsing errors
//! - Issue #20: `<!-- svelte-ignore -->` pragma comments suppress warnings
//! - Issue #19: tsconfig `exclude` patterns filter Svelte diagnostics
//!
//! All tests use JSON output for precise verification of:
//! - Exact error locations (file, line, column)
//! - Exact error codes and messages
//! - No unexpected errors in valid files
//!
//! Test fixtures are located in: test-fixtures/projects/sveltekit-bundler/
//!
//! Note: These tests are skipped on Windows due to tsgo/path handling differences.

#![cfg(not(target_os = "windows"))]

use serde::Deserialize;
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

// ============================================================================
// TEST INFRASTRUCTURE
// ============================================================================

/// Path to the test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("projects")
}

/// Path to the svelte-check-rs binary
fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("svelte-check-rs")
}

/// Path to the svelte-check-rs cache directory for a fixture
fn cache_root(fixture_path: &std::path::Path) -> PathBuf {
    fixture_path
        .join("node_modules")
        .join(".cache")
        .join("svelte-check-rs")
}

/// A diagnostic from the JSON output
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct JsonDiagnostic {
    #[serde(rename = "type")]
    diagnostic_type: String,
    filename: String,
    start: JsonPosition,
    message: String,
    code: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct JsonPosition {
    line: u32,
    column: u32,
    offset: u32,
}

/// Expected diagnostic definition for precise testing
#[derive(Debug, Clone)]
struct ExpectedDiagnostic {
    filename: &'static str,
    line: u32,
    code: &'static str,
    message_contains: &'static str,
}

/// Fixture state tracking
static BUNDLER_READY: OnceLock<()> = OnceLock::new();

/// Ensures dependencies are installed for a fixture (runs once per fixture)
fn ensure_fixture_ready(fixture_path: &PathBuf, ready: &'static OnceLock<()>) {
    ready.get_or_init(|| {
        // Clean cache to ensure fresh state
        let cache_path = cache_root(fixture_path);
        let _ = fs::remove_dir_all(&cache_path);

        // Check if node_modules exists
        let node_modules = fixture_path.join("node_modules");
        if !node_modules.exists() {
            eprintln!("Installing dependencies for sveltekit-bundler...");

            let output = Command::new("bun")
                .arg("install")
                .current_dir(fixture_path)
                .output()
                .expect("Failed to run bun install. Is bun installed?");

            if !output.status.success() {
                panic!(
                    "bun install failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        // Run svelte-kit sync to generate types
        let _ = Command::new("bunx")
            .args(["svelte-kit", "sync"])
            .current_dir(fixture_path)
            .output();
    });
}

/// Runs svelte-check-rs on a fixture with JSON output
fn run_check_json(fixture_path: &PathBuf, diagnostic_sources: &str) -> (i32, Vec<JsonDiagnostic>) {
    // Ensure fixture is ready
    ensure_fixture_ready(fixture_path, &BUNDLER_READY);

    // Build if necessary
    let _ = Command::new("cargo")
        .args(["build", "-p", "svelte-check-rs"])
        .output();

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_path)
        .arg("--diagnostic-sources")
        .arg(diagnostic_sources)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Parse JSON diagnostics
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON output: {}", e);
        eprintln!("Raw output:\n{}", stdout);
        vec![]
    });

    (exit_code, diagnostics)
}

/// Verifies that an expected diagnostic is present in the results
fn assert_diagnostic_present(diagnostics: &[JsonDiagnostic], expected: &ExpectedDiagnostic) {
    let found = diagnostics.iter().any(|d| {
        d.filename.ends_with(expected.filename)
            && d.start.line == expected.line
            && d.code == expected.code
            && d.message.contains(expected.message_contains)
    });

    assert!(
        found,
        "Expected diagnostic not found:\n  File: {}\n  Line: {}\n  Code: {}\n  Message contains: '{}'\n\nActual diagnostics:\n{:#?}",
        expected.filename, expected.line, expected.code, expected.message_contains, diagnostics
    );
}

/// Verifies that no diagnostics exist for a given file
fn assert_no_diagnostics_in_file(diagnostics: &[JsonDiagnostic], filename: &str) {
    let diagnostics_in_file: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.ends_with(filename))
        .collect();

    assert!(
        diagnostics_in_file.is_empty(),
        "Expected no diagnostics in {}, but found:\n{:#?}",
        filename,
        diagnostics_in_file
    );
}

/// Count diagnostics matching a predicate
fn count_diagnostics_matching<F>(diagnostics: &[JsonDiagnostic], predicate: F) -> usize
where
    F: Fn(&JsonDiagnostic) -> bool,
{
    diagnostics.iter().filter(|d| predicate(d)).count()
}

// ============================================================================
// ISSUE #21: COLON IN IMPORTS
// ============================================================================
// These tests verify that colons in import paths, string literals, and regex
// patterns don't cause parsing errors on Svelte special elements.
//
// The bug was that the parser incorrectly treated colons as pseudo-class
// selectors in CSS context, causing errors like:
//   - "Unknown pseudo-class :head" for <svelte:head>
//   - "Unknown pseudo-class :global" for :global()
//
// Test files: test-fixtures/projects/sveltekit-bundler/src/routes/issue-21-*/
/// Test that imports with colons (like `virtual:pwa-register`) don't cause parsing errors.
///
/// This reproduces issue #21 where imports like:
///   import { registerSW } from 'virtual:pwa-register';
/// caused spurious errors on `<svelte:head>` and `:global()` constructs.
///
/// Fixture: src/routes/issue-21-colon-import/+page.svelte
/// Line numbers for reference:
///   Line 10: <svelte:head> - this should NOT produce an error
///   Line 19: :global(body) - this should NOT produce an error
#[test]
#[serial]
fn test_colon_in_import_no_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "js,svelte");

    // Verify no parse errors or svelte-related errors exist for this file
    assert_no_diagnostics_in_file(&diagnostics, "issue-21-colon-import/+page.svelte");
}

/// Test that regex literals with colons don't cause parsing errors.
///
/// Fixture: src/routes/issue-21-regex-colon/+page.svelte
/// Line numbers for reference:
///   Line 13: <svelte:head> - this should NOT produce an error
#[test]
#[serial]
fn test_regex_with_colon_no_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "js,svelte");

    // Verify no diagnostics exist for this file
    assert_no_diagnostics_in_file(&diagnostics, "issue-21-regex-colon/+page.svelte");
}

// ============================================================================
// ISSUE #20: SVELTE-IGNORE PRAGMA
// ============================================================================
// These tests verify that `<!-- svelte-ignore code -->` comments suppress
// specific warnings for the immediately following element.
//
// The pragma should:
// - Suppress the specified warning code
// - Only affect the next element (not subsequent elements)
// - Support multiple warning codes separated by commas
//
// Test files: test-fixtures/projects/sveltekit-bundler/src/routes/issue-20-*/
/// Test that `<!-- svelte-ignore -->` pragma suppresses a11y warnings.
///
/// Fixture: src/routes/issue-20-svelte-ignore/+page.svelte
/// Line numbers for reference:
///   Line 6: <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
///   Line 7: <div tabindex="0"> - this warning should be suppressed
#[test]
#[serial]
fn test_svelte_ignore_suppresses_a11y_warning() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Verify no a11y-no-noninteractive-tabindex warning exists for this file
    assert_no_diagnostics_in_file(&diagnostics, "issue-20-svelte-ignore/+page.svelte");
}

/// Test that svelte-ignore only affects the next element, not subsequent ones.
///
/// Fixture: src/routes/issue-20-svelte-ignore-scope/+page.svelte
/// Line numbers for reference:
///   Line 5: <!-- svelte-ignore ... -->
///   Line 6: <div tabindex="0"> - suppressed
///   Line 9: <div tabindex="0"> - NOT suppressed, should warn on line 9
#[test]
#[serial]
fn test_svelte_ignore_only_affects_next_element() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Should have exactly ONE warning for the second div at line 9
    let expected = ExpectedDiagnostic {
        filename: "issue-20-svelte-ignore-scope/+page.svelte",
        line: 9,
        code: "a11y-no-noninteractive-tabindex",
        message_contains: "tabindex",
    };
    assert_diagnostic_present(&diagnostics, &expected);

    // Verify only one warning total for this file
    let warning_count = count_diagnostics_matching(&diagnostics, |d| {
        d.filename
            .ends_with("issue-20-svelte-ignore-scope/+page.svelte")
            && d.code == "a11y-no-noninteractive-tabindex"
    });
    assert_eq!(
        warning_count,
        1,
        "Expected exactly 1 warning (on line 9), found {}: {:?}",
        warning_count,
        diagnostics
            .iter()
            .filter(|d| d
                .filename
                .ends_with("issue-20-svelte-ignore-scope/+page.svelte"))
            .collect::<Vec<_>>()
    );
}

/// Test that without svelte-ignore pragma, warnings are produced at correct line.
///
/// Fixture: src/routes/issue-20-no-pragma/+page.svelte
/// Line numbers for reference:
///   Line 6: <div tabindex="0"> - should produce warning on line 6
#[test]
#[serial]
fn test_no_svelte_ignore_produces_warning() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Verify warning exists on line 6 with correct code
    let expected = ExpectedDiagnostic {
        filename: "issue-20-no-pragma/+page.svelte",
        line: 6,
        code: "a11y-no-noninteractive-tabindex",
        message_contains: "tabindex",
    };
    assert_diagnostic_present(&diagnostics, &expected);
}

// ============================================================================
// ISSUE #19: TSCONFIG EXCLUDE PATTERNS
// ============================================================================
// These tests verify that tsconfig.json `exclude` patterns correctly filter
// Svelte files from diagnostics, matching TypeScript's behavior.
//
// The exclude patterns should:
// - Support simple directory patterns (e.g., "src/routes/issue-19-excluded")
// - Support wildcard patterns (e.g., "**/__tests__/**")
// - Apply to both TypeScript and Svelte diagnostics
//
// Test files: test-fixtures/projects/sveltekit-bundler/src/routes/issue-19-*/
//
// NOTE: These tests modify tsconfig.json during execution and restore it
// afterward. This is necessary because tsconfig exclude patterns must be
// set before running the checker.

/// Test that tsconfig `exclude` patterns filter out Svelte diagnostics.
///
/// When a directory is in tsconfig's exclude array, files in that directory
/// should NOT be checked and should NOT produce any diagnostics.
///
/// Fixture: src/routes/issue-19-excluded/+page.svelte
/// If NOT excluded, would produce warnings on lines 6 and 7
#[test]
#[serial]
fn test_tsconfig_exclude_filters_svelte_diagnostics() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Update tsconfig.json to exclude the issue-19-excluded directory
    let tsconfig_path = fixture_path.join("tsconfig.json");
    let original_tsconfig = fs::read_to_string(&tsconfig_path).expect("Failed to read tsconfig");

    // Parse original and add exclude pattern
    let updated_tsconfig = r#"{
	"extends": "./.svelte-kit/tsconfig.json",
	"compilerOptions": {
		"allowJs": true,
		"checkJs": true,
		"esModuleInterop": true,
		"forceConsistentCasingInFileNames": true,
		"resolveJsonModule": true,
		"skipLibCheck": true,
		"sourceMap": true,
		"strict": true,
		"moduleResolution": "bundler"
	},
	"exclude": ["node_modules", "src/routes/issue-19-excluded"]
}
"#;
    fs::write(&tsconfig_path, updated_tsconfig).expect("Failed to write updated tsconfig");

    // Clean cache to ensure tsconfig changes are picked up
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Restore original tsconfig before asserting (ensures cleanup even on failure)
    fs::write(&tsconfig_path, &original_tsconfig).expect("Failed to restore tsconfig");

    // Verify no diagnostics for excluded file
    assert_no_diagnostics_in_file(&diagnostics, "issue-19-excluded/+page.svelte");
}

/// Test that files NOT in exclude patterns still produce diagnostics at correct lines.
///
/// Fixture: src/routes/issue-19-not-excluded/+page.svelte
/// Line numbers for reference:
///   Line 6: <div tabindex="0"> - should produce warning on line 6
#[test]
#[serial]
fn test_non_excluded_files_still_checked() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Verify warning exists on line 6 with correct code
    let expected = ExpectedDiagnostic {
        filename: "issue-19-not-excluded/+page.svelte",
        line: 6,
        code: "a11y-no-noninteractive-tabindex",
        message_contains: "tabindex",
    };
    assert_diagnostic_present(&diagnostics, &expected);
}

/// Test that wildcard exclude patterns work correctly.
///
/// Tests patterns like:
///   - "**/__tests__/**" - matches any __tests__ directory
///   - "**/spec/**" - matches any spec directory
///
/// Fixtures:
///   - src/__tests__/TestComponent.svelte (line 2 has a11y issue)
///   - src/spec/SpecComponent.svelte (line 2 has a11y issue)
#[test]
#[serial]
fn test_tsconfig_exclude_wildcard_patterns() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Update tsconfig.json with wildcard exclude patterns
    let tsconfig_path = fixture_path.join("tsconfig.json");
    let original_tsconfig = fs::read_to_string(&tsconfig_path).expect("Failed to read tsconfig");

    let updated_tsconfig = r#"{
	"extends": "./.svelte-kit/tsconfig.json",
	"compilerOptions": {
		"allowJs": true,
		"checkJs": true,
		"esModuleInterop": true,
		"forceConsistentCasingInFileNames": true,
		"resolveJsonModule": true,
		"skipLibCheck": true,
		"sourceMap": true,
		"strict": true,
		"moduleResolution": "bundler"
	},
	"exclude": ["node_modules", "**/__tests__/**", "**/spec/**"]
}
"#;
    fs::write(&tsconfig_path, updated_tsconfig).expect("Failed to write updated tsconfig");

    // Clean cache to ensure tsconfig changes are picked up
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Restore original tsconfig before asserting
    fs::write(&tsconfig_path, &original_tsconfig).expect("Failed to restore tsconfig");

    // Verify no diagnostics for files in excluded test directories
    assert_no_diagnostics_in_file(&diagnostics, "__tests__/TestComponent.svelte");
    assert_no_diagnostics_in_file(&diagnostics, "spec/SpecComponent.svelte");
}
