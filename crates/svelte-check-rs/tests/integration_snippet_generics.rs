//! Integration tests for issue #32 (snippet generic headers).
//!
//! These tests verify:
//! - Generic snippet headers type-check cleanly (no diagnostics)
//! - Generic constraints produce TypeScript errors at the correct line/column
//!
//! Test fixtures are located in: test-fixtures/projects/sveltekit-bundler/

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
    column: u32,
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

/// Runs svelte-check-rs on a fixture with JSON output, restricted to a single file
fn run_check_json_single(
    fixture_path: &PathBuf,
    single_file: &PathBuf,
) -> (i32, Vec<JsonDiagnostic>) {
    // Ensure fixture is ready
    ensure_fixture_ready(fixture_path, &BUNDLER_READY);

    // Build if necessary
    let _ = Command::new("cargo")
        .args(["build", "-p", "svelte-check-rs"])
        .output();

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_path)
        .arg("--single-file")
        .arg(single_file)
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

/// Filters diagnostics by source (ts, svelte, etc.)
fn filter_diagnostics_by_source(
    diagnostics: &[JsonDiagnostic],
    source: &str,
) -> Vec<JsonDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.source == source)
        .cloned()
        .collect()
}

/// Verifies that an expected diagnostic is present in the results
fn assert_diagnostic_present(diagnostics: &[JsonDiagnostic], expected: &ExpectedDiagnostic) {
    let found = diagnostics.iter().any(|d| {
        d.filename.ends_with(expected.filename)
            && d.start.line == expected.line
            && d.start.column == expected.column
            && d.code == expected.code
            && d.message.contains(expected.message_contains)
    });

    assert!(
        found,
        "Expected diagnostic not found:\n  File: {}\n  Line: {}\n  Column: {}\n  Code: {}\n  Message contains: '{}'\n\nActual diagnostics:\n{:#?}",
        expected.filename,
        expected.line,
        expected.column,
        expected.code,
        expected.message_contains,
        diagnostics
    );
}

// ============================================================================
// ISSUE #32: SNIPPET GENERIC HEADERS
// ============================================================================

/// Test that a generic snippet header type-checks without diagnostics.
#[test]
#[serial]
fn test_snippet_generic_header_no_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let file_path = fixture_path.join("src/routes/issue-32-snippet-generic-ok/+page.svelte");

    let (_exit_code, diagnostics) = run_check_json_single(&fixture_path, &file_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for generic snippet header, found:\n{:#?}",
        diagnostics
    );
}

/// Test that generic constraint errors map to the correct line/column.
#[test]
#[serial]
fn test_snippet_generic_header_error_location() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let file_path = fixture_path.join("src/routes/issue-32-snippet-generic-error/+page.svelte");

    let (_exit_code, diagnostics) = run_check_json_single(&fixture_path, &file_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    let expected = ExpectedDiagnostic {
        filename: "issue-32-snippet-generic-error/+page.svelte",
        line: 7,
        column: 35,
        code: "TS2322",
        message_contains: "number",
    };

    assert_diagnostic_present(&diagnostics, &expected);
}

/// Test that optional property type errors map to the correct line/column.
#[test]
#[serial]
fn test_snippet_generic_header_optional_property_error_location() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let file_path =
        fixture_path.join("src/routes/issue-32-snippet-generic-error-shortname/+page.svelte");

    let (_exit_code, diagnostics) = run_check_json_single(&fixture_path, &file_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    let expected = ExpectedDiagnostic {
        filename: "issue-32-snippet-generic-error-shortname/+page.svelte",
        line: 7,
        column: 63,
        code: "TS2322",
        message_contains: "number",
    };

    assert_diagnostic_present(&diagnostics, &expected);
}
