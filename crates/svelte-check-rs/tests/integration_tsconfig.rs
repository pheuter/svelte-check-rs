//! Integration tests for different TypeScript configurations.
//!
//! These tests verify that svelte-check-rs correctly detects type errors
//! across different tsconfig module resolution strategies (bundler, NodeNext, Node16).
//!
//! All tests use JSON output for precise verification of:
//! - Exact error locations (file, line, column)
//! - Exact error codes and messages
//! - No unexpected errors in valid files
//!
//! Note: Tests are serialized using #[serial] to avoid race conditions during
//! fixture setup (bun install creates bun.lock before node_modules is complete).
//!
//! Note: These tests are skipped on Windows due to tsgo/path handling differences.

// Skip all tests on Windows - tsgo and path handling differs
#![cfg(not(target_os = "windows"))]

use serde::Deserialize;
use serial_test::serial;
use std::process::Command;
use std::sync::OnceLock;

// ============================================================================
// SHARED TEST INFRASTRUCTURE
// ============================================================================

/// Path to the test fixtures directory
fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("projects")
}

/// Path to the svelte-check-rs binary
fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
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

/// Expected error definition for precise testing
#[derive(Debug, Clone)]
struct ExpectedError {
    filename: &'static str,
    line: u32,
    code: &'static str,
    message_contains: &'static str,
}

/// Fixture state tracking
static BUNDLER_READY: OnceLock<()> = OnceLock::new();
static NODENEXT_READY: OnceLock<()> = OnceLock::new();
static NODE16_READY: OnceLock<()> = OnceLock::new();
static MODULES_READY: OnceLock<()> = OnceLock::new();

/// Ensures dependencies are installed for a fixture (runs once per fixture)
fn ensure_fixture_ready(fixture_name: &str, ready: &'static OnceLock<()>) {
    ready.get_or_init(|| {
        let fixture_path = fixtures_dir().join(fixture_name);

        // Clean cache to ensure fresh state
        let cache_path = fixture_path.join(".svelte-check-rs");
        let _ = std::fs::remove_dir_all(&cache_path);

        // Check if node_modules exists
        let node_modules = fixture_path.join("node_modules");
        if !node_modules.exists() {
            eprintln!("Installing dependencies for {}...", fixture_name);

            let output = Command::new("bun")
                .arg("install")
                .current_dir(&fixture_path)
                .output()
                .expect("Failed to run bun install. Is bun installed?");

            if !output.status.success() {
                panic!(
                    "bun install failed for {}:\n{}",
                    fixture_name,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        // Run svelte-kit sync to generate types (for SvelteKit projects)
        let _ = Command::new("bunx")
            .args(["svelte-kit", "sync"])
            .current_dir(&fixture_path)
            .output();
    });
}

/// Runs svelte-check-rs on a fixture with JSON output
fn run_check_json(fixture_name: &str) -> (i32, Vec<JsonDiagnostic>) {
    let (exit_code, diagnostics, _stderr) = run_check_json_internal(fixture_name, false);
    (exit_code, diagnostics)
}

/// Runs svelte-check-rs on a fixture with JSON output and optional TS emission.
fn run_check_json_internal(
    fixture_name: &str,
    emit_ts: bool,
) -> (i32, Vec<JsonDiagnostic>, String) {
    // Map fixture name to its ready flag
    let ready = match fixture_name {
        "sveltekit-bundler" => &BUNDLER_READY,
        "sveltekit-nodenext" => &NODENEXT_READY,
        "sveltekit-node16" => &NODE16_READY,
        "svelte-modules" => &MODULES_READY,
        _ => panic!("Unknown fixture: {}", fixture_name),
    };

    // Ensure dependencies are installed
    ensure_fixture_ready(fixture_name, ready);

    let fixture_path = fixtures_dir().join(fixture_name);
    let binary = binary_path();

    // Build if necessary
    let _ = Command::new("cargo")
        .args(["build", "-p", "svelte-check-rs"])
        .output();

    let output = Command::new(&binary)
        .arg("--workspace")
        .arg(&fixture_path)
        .arg("--diagnostic-sources")
        .arg("js")
        .args(if emit_ts { vec!["--emit-ts"] } else { vec![] })
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Parse JSON diagnostics
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON output: {}", e);
        eprintln!("Raw output:\n{}", stdout);
        vec![]
    });

    (exit_code, diagnostics, stderr)
}

/// Runs svelte-check-rs on a fixture with JSON output and emitted TS output.
fn run_check_json_with_emit_ts(fixture_name: &str) -> (i32, Vec<JsonDiagnostic>, String) {
    run_check_json_internal(fixture_name, true)
}

/// Extract emitted TS block for a relative path from stderr output.
fn extract_emitted_ts(stderr: &str, relative_path: &str) -> Option<String> {
    let marker = format!("=== TypeScript for {} ===\n", relative_path);
    let start = stderr.find(&marker)? + marker.len();
    let rest = &stderr[start..];
    let end = rest.find("=== TypeScript for ").unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Verifies that an expected error is present in the diagnostics
fn assert_error_present(diagnostics: &[JsonDiagnostic], expected: &ExpectedError) {
    let found = diagnostics.iter().any(|d| {
        d.filename == expected.filename
            && d.start.line == expected.line
            && d.code == expected.code
            && d.message.contains(expected.message_contains)
    });

    assert!(
        found,
        "Expected error not found:\n  File: {}\n  Line: {}\n  Code: {}\n  Message contains: '{}'\n\nActual diagnostics:\n{:#?}",
        expected.filename, expected.line, expected.code, expected.message_contains, diagnostics
    );
}

/// Verifies that no errors exist for a given file
fn assert_no_errors_in_file(diagnostics: &[JsonDiagnostic], filename: &str) {
    let errors_in_file: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename == filename && d.diagnostic_type == "Error")
        .collect();

    assert!(
        errors_in_file.is_empty(),
        "Expected no errors in {}, but found:\n{:#?}",
        filename,
        errors_in_file
    );
}

/// Verifies the exact set of expected errors (no more, no less)
fn assert_exact_errors(diagnostics: &[JsonDiagnostic], expected: &[ExpectedError]) {
    // Check all expected errors are present
    for exp in expected {
        assert_error_present(diagnostics, exp);
    }

    // Check no unexpected errors
    let error_count = diagnostics
        .iter()
        .filter(|d| d.diagnostic_type == "Error")
        .count();

    assert_eq!(
        error_count,
        expected.len(),
        "Expected exactly {} errors, but found {}.\n\nExpected:\n{:#?}\n\nActual:\n{:#?}",
        expected.len(),
        error_count,
        expected,
        diagnostics
    );
}

/// Verifies no module resolution errors (indicates broken imports)
fn assert_no_resolution_errors(diagnostics: &[JsonDiagnostic]) {
    let resolution_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            // These codes indicate broken module resolution
            d.code == "TS2614"  // has no exported member
                || d.code == "TS1149"  // file name casing
                || d.code == "TS2307" // cannot find module
        })
        .collect();

    assert!(
        resolution_errors.is_empty(),
        "Found module resolution errors (indicates broken imports):\n{:#?}",
        resolution_errors
    );
}

/// Counts errors by code
fn count_errors_by_code(diagnostics: &[JsonDiagnostic], code: &str) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.code == code && d.diagnostic_type == "Error")
        .count()
}

// ============================================================================
// BUNDLER MODULE RESOLUTION TESTS
// ============================================================================

/// Expected errors for the sveltekit-bundler fixture
fn bundler_expected_errors() -> Vec<ExpectedError> {
    vec![
        // PageData type errors - accessing non-existent properties
        // Line 12 in original source: `const comments = data.comments;`
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 12,
            code: "TS2339",
            message_contains: "comments",
        },
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 22,
            code: "TS2339",
            message_contains: "author",
        },
        // Component prop errors
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 32,
            code: "TS2353",
            message_contains: "wrong",
        },
        // Variant type error
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 37,
            code: "TS2322",
            message_contains: "invalid",
        },
    ]
}

#[test]
#[serial]
fn test_bundler_exact_errors() {
    let (exit_code, diagnostics) = run_check_json("sveltekit-bundler");

    // Verify exact errors
    assert_exact_errors(&diagnostics, &bundler_expected_errors());

    // Should exit with error code
    assert_ne!(exit_code, 0, "Expected non-zero exit code due to errors");
}

#[test]
#[serial]
fn test_bundler_no_resolution_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-bundler");

    // Should not have any module resolution errors
    assert_no_resolution_errors(&diagnostics);
}

#[test]
#[serial]
fn test_bundler_pagedata_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-bundler");

    // Verify PageData type errors are correctly detected
    let pagedata_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename == "src/routes/+page.svelte"
                && d.code == "TS2339"
                && (d.message.contains("comments") || d.message.contains("author"))
        })
        .collect();

    assert_eq!(
        pagedata_errors.len(),
        2,
        "Expected 2 PageData errors (comments, author), found {}:\n{:#?}",
        pagedata_errors.len(),
        pagedata_errors
    );

    // Verify line numbers
    let lines: Vec<u32> = pagedata_errors.iter().map(|e| e.start.line).collect();
    assert!(lines.contains(&12), "Expected 'comments' error on line 12");
    assert!(lines.contains(&22), "Expected 'author' error on line 22");
}

#[test]
#[serial]
fn test_bundler_component_prop_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-bundler");

    // Verify component prop errors
    let prop_error = diagnostics.iter().find(|d| {
        d.filename == "src/routes/+page.svelte" && d.code == "TS2353" && d.message.contains("wrong")
    });

    assert!(
        prop_error.is_some(),
        "Expected 'wrong' prop error on Button component"
    );

    let error = prop_error.unwrap();
    assert_eq!(
        error.start.line, 32,
        "Expected 'wrong' prop error on line 32, found on line {}",
        error.start.line
    );
}

#[test]
#[serial]
fn test_bundler_variant_type_error() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-bundler");

    // Verify variant type error
    let variant_error = diagnostics.iter().find(|d| {
        d.filename == "src/routes/+page.svelte"
            && d.code == "TS2322"
            && d.message.contains("invalid")
    });

    assert!(
        variant_error.is_some(),
        "Expected 'invalid' variant type error"
    );

    let error = variant_error.unwrap();
    assert_eq!(
        error.start.line, 37,
        "Expected variant error on line 37, found on line {}",
        error.start.line
    );
}

#[test]
#[serial]
fn test_bundler_no_errors_in_valid_files() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-bundler");

    // These files should have NO errors
    assert_no_errors_in_file(&diagnostics, "src/lib/components/Button.svelte");
    assert_no_errors_in_file(&diagnostics, "src/routes/+page.server.ts");
    assert_no_errors_in_file(&diagnostics, "src/routes/+layout.svelte");
}

// ============================================================================
// NODENEXT MODULE RESOLUTION TESTS
// ============================================================================

/// Expected errors for the sveltekit-nodenext fixture
fn nodenext_expected_errors() -> Vec<ExpectedError> {
    vec![
        // TS2834 errors - relative imports need explicit extensions
        // Only user code imports trigger these (generated imports now use .js extension)
        // Line 1: `import type { PageServerLoad } from './$types';`
        ExpectedError {
            filename: "src/routes/+page.server.ts",
            line: 1,
            code: "TS2834",
            message_contains: "explicit file extensions",
        },
        // Line 2: `import type { PageData } from './$types';`
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 2,
            code: "TS2834",
            message_contains: "explicit file extensions",
        },
        // Component prop errors - THE KEY TEST for issue #4
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 32,
            code: "TS2353",
            message_contains: "wrong",
        },
        // Variant type error
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 37,
            code: "TS2322",
            message_contains: "invalid",
        },
    ]
}

#[test]
#[serial]
fn test_nodenext_exact_errors() {
    let (exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    // Verify exact errors
    assert_exact_errors(&diagnostics, &nodenext_expected_errors());

    // Should exit with error code
    assert_ne!(exit_code, 0, "Expected non-zero exit code due to errors");
}

#[test]
#[serial]
fn test_nodenext_no_resolution_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    // Should not have module resolution errors (TS2614, TS1149, TS2307)
    // Note: TS2834 is expected for NodeNext (explicit extensions needed)
    assert_no_resolution_errors(&diagnostics);
}

#[test]
#[serial]
fn test_nodenext_component_prop_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    // This is the KEY test for GitHub issue #4
    // NodeNext was previously missing component prop errors
    let prop_error = diagnostics.iter().find(|d| {
        d.filename == "src/routes/+page.svelte" && d.code == "TS2353" && d.message.contains("wrong")
    });

    assert!(
        prop_error.is_some(),
        "REGRESSION (issue #4): NodeNext should detect 'wrong' prop error on Button component"
    );

    let error = prop_error.unwrap();
    assert_eq!(
        error.start.line, 32,
        "Expected 'wrong' prop error on line 32, found on line {}",
        error.start.line
    );
}

#[test]
#[serial]
fn test_nodenext_variant_type_error() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    let variant_error = diagnostics.iter().find(|d| {
        d.filename == "src/routes/+page.svelte"
            && d.code == "TS2322"
            && d.message.contains("invalid")
    });

    assert!(
        variant_error.is_some(),
        "Expected 'invalid' variant type error in NodeNext"
    );

    let error = variant_error.unwrap();
    assert_eq!(
        error.start.line, 37,
        "Expected variant error on line 37, found on line {}",
        error.start.line
    );
}

#[test]
#[serial]
fn test_nodenext_extension_errors_expected() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    // TS2834 errors are expected for user code imports without extensions
    // Generated imports (like $types) now use .js extension
    let extension_errors = count_errors_by_code(&diagnostics, "TS2834");

    assert_eq!(
        extension_errors, 2,
        "Expected 2 TS2834 errors (extension required for user imports), found {}",
        extension_errors
    );
}

#[test]
#[serial]
fn test_nodenext_no_errors_in_button() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    // The Button component itself should have no errors
    assert_no_errors_in_file(&diagnostics, "src/lib/components/Button.svelte");
}

// ============================================================================
// NODE16 MODULE RESOLUTION TESTS
// ============================================================================

/// Expected errors for the sveltekit-node16 fixture (same as nodenext)
fn node16_expected_errors() -> Vec<ExpectedError> {
    vec![
        // TS2834 errors - relative imports need explicit extensions
        // Only user code imports trigger these (generated imports now use .js extension)
        ExpectedError {
            filename: "src/routes/+page.server.ts",
            line: 1,
            code: "TS2834",
            message_contains: "explicit file extensions",
        },
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 2,
            code: "TS2834",
            message_contains: "explicit file extensions",
        },
        // Component prop errors
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 32,
            code: "TS2353",
            message_contains: "wrong",
        },
        // Variant type error
        ExpectedError {
            filename: "src/routes/+page.svelte",
            line: 37,
            code: "TS2322",
            message_contains: "invalid",
        },
    ]
}

#[test]
#[serial]
fn test_node16_exact_errors() {
    let (exit_code, diagnostics) = run_check_json("sveltekit-node16");

    // Verify exact errors
    assert_exact_errors(&diagnostics, &node16_expected_errors());

    // Should exit with error code
    assert_ne!(exit_code, 0, "Expected non-zero exit code due to errors");
}

#[test]
#[serial]
fn test_node16_no_resolution_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-node16");

    // Should not have module resolution errors
    assert_no_resolution_errors(&diagnostics);
}

#[test]
#[serial]
fn test_node16_component_prop_errors() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-node16");

    let prop_error = diagnostics.iter().find(|d| {
        d.filename == "src/routes/+page.svelte" && d.code == "TS2353" && d.message.contains("wrong")
    });

    assert!(
        prop_error.is_some(),
        "Node16 should detect 'wrong' prop error on Button component"
    );

    let error = prop_error.unwrap();
    assert_eq!(
        error.start.line, 32,
        "Expected 'wrong' prop error on line 32"
    );
}

#[test]
#[serial]
fn test_node16_variant_type_error() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-node16");

    let variant_error = diagnostics.iter().find(|d| {
        d.filename == "src/routes/+page.svelte"
            && d.code == "TS2322"
            && d.message.contains("invalid")
    });

    assert!(
        variant_error.is_some(),
        "Expected 'invalid' variant type error in Node16"
    );

    let error = variant_error.unwrap();
    assert_eq!(error.start.line, 37, "Expected variant error on line 37");
}

#[test]
#[serial]
fn test_node16_no_errors_in_button() {
    let (_exit_code, diagnostics) = run_check_json("sveltekit-node16");

    assert_no_errors_in_file(&diagnostics, "src/lib/components/Button.svelte");
}

#[test]
#[serial]
fn test_node16_rewrites_svelte_imports_in_emitted_ts() {
    let (_exit_code, _diagnostics, stderr) = run_check_json_with_emit_ts("sveltekit-node16");
    let ts = extract_emitted_ts(&stderr, "src/routes/+page.svelte")
        .expect("Expected emitted TS for src/routes/+page.svelte");

    assert!(
        ts.contains("Button.svelte.js"),
        "Expected Node16 emitted TS to rewrite .svelte imports to .svelte.js, but got:\n{}",
        ts
    );
}

#[test]
#[serial]
fn test_bundler_does_not_rewrite_svelte_imports_in_emitted_ts() {
    let (_exit_code, _diagnostics, stderr) = run_check_json_with_emit_ts("sveltekit-bundler");
    let ts = extract_emitted_ts(&stderr, "src/routes/+page.svelte")
        .expect("Expected emitted TS for src/routes/+page.svelte");

    assert!(
        ts.contains("Button.svelte"),
        "Expected bundler emitted TS to include .svelte import, but got:\n{}",
        ts
    );
    assert!(
        !ts.contains("Button.svelte.js"),
        "Did not expect bundler emitted TS to rewrite .svelte imports to .svelte.js, but got:\n{}",
        ts
    );
}

// ============================================================================
// CROSS-CONFIG PARITY TESTS
// ============================================================================

#[test]
#[serial]
fn test_all_configs_detect_component_prop_errors() {
    let (_, bundler_diags) = run_check_json("sveltekit-bundler");
    let (_, nodenext_diags) = run_check_json("sveltekit-nodenext");
    let (_, node16_diags) = run_check_json("sveltekit-node16");

    // All configs should detect the 'wrong' prop error (TS2353)
    let bundler_has_prop_error = bundler_diags
        .iter()
        .any(|d| d.code == "TS2353" && d.message.contains("wrong"));
    let nodenext_has_prop_error = nodenext_diags
        .iter()
        .any(|d| d.code == "TS2353" && d.message.contains("wrong"));
    let node16_has_prop_error = node16_diags
        .iter()
        .any(|d| d.code == "TS2353" && d.message.contains("wrong"));

    assert!(bundler_has_prop_error, "Bundler should detect 'wrong' prop");
    assert!(
        nodenext_has_prop_error,
        "NodeNext should detect 'wrong' prop (issue #4 regression)"
    );
    assert!(node16_has_prop_error, "Node16 should detect 'wrong' prop");
}

#[test]
#[serial]
fn test_all_configs_detect_variant_errors() {
    let (_, bundler_diags) = run_check_json("sveltekit-bundler");
    let (_, nodenext_diags) = run_check_json("sveltekit-nodenext");
    let (_, node16_diags) = run_check_json("sveltekit-node16");

    // All configs should detect the 'invalid' variant error (TS2322)
    let bundler_has_variant_error = bundler_diags
        .iter()
        .any(|d| d.code == "TS2322" && d.message.contains("invalid"));
    let nodenext_has_variant_error = nodenext_diags
        .iter()
        .any(|d| d.code == "TS2322" && d.message.contains("invalid"));
    let node16_has_variant_error = node16_diags
        .iter()
        .any(|d| d.code == "TS2322" && d.message.contains("invalid"));

    assert!(
        bundler_has_variant_error,
        "Bundler should detect 'invalid' variant"
    );
    assert!(
        nodenext_has_variant_error,
        "NodeNext should detect 'invalid' variant"
    );
    assert!(
        node16_has_variant_error,
        "Node16 should detect 'invalid' variant"
    );
}

#[test]
#[serial]
fn test_all_configs_have_expected_error_counts() {
    let (_, bundler_diags) = run_check_json("sveltekit-bundler");
    let (_, nodenext_diags) = run_check_json("sveltekit-nodenext");
    let (_, node16_diags) = run_check_json("sveltekit-node16");

    let bundler_errors = bundler_diags
        .iter()
        .filter(|d| d.diagnostic_type == "Error")
        .count();
    let nodenext_errors = nodenext_diags
        .iter()
        .filter(|d| d.diagnostic_type == "Error")
        .count();
    let node16_errors = node16_diags
        .iter()
        .filter(|d| d.diagnostic_type == "Error")
        .count();

    assert_eq!(bundler_errors, 4, "Bundler should have exactly 4 errors");
    assert_eq!(
        nodenext_errors, 4,
        "NodeNext should have exactly 4 errors (2 TS2834 + 2 type errors)"
    );
    assert_eq!(
        node16_errors, 4,
        "Node16 should have exactly 4 errors (2 TS2834 + 2 type errors)"
    );
}

// ============================================================================
// REGRESSION TESTS FOR GITHUB ISSUE #4
// ============================================================================

/// This is the specific regression test for GitHub issue #4.
/// Previously, NodeNext/Node16 module resolution would cause .svelte imports
/// to silently fail to resolve, making component types become `any` and
/// missing prop type errors.
#[test]
#[serial]
fn test_issue_4_nodenext_prop_detection() {
    let (exit_code, diagnostics) = run_check_json("sveltekit-nodenext");

    // The key assertion: NodeNext should detect prop errors just like bundler does
    assert_ne!(
        exit_code, 0,
        "REGRESSION: NodeNext should detect type errors (issue #4)"
    );

    // Should specifically detect the 'wrong' prop error
    let has_wrong_prop_error = diagnostics
        .iter()
        .any(|d| d.code == "TS2353" && d.message.contains("wrong"));

    assert!(
        has_wrong_prop_error,
        "REGRESSION: NodeNext should detect 'wrong' prop on imported components (issue #4).\n\
         This indicates .svelte imports may be silently failing.\n\
         Diagnostics:\n{:#?}",
        diagnostics
    );

    // Verify the error is on the expected line
    let prop_error = diagnostics
        .iter()
        .find(|d| d.code == "TS2353" && d.message.contains("wrong"))
        .unwrap();

    assert_eq!(
        prop_error.start.line, 32,
        "REGRESSION: 'wrong' prop error should be on line 32"
    );
}

// ============================================================================
// SVELTE MODULE (.svelte.ts/.svelte.js) TESTS
// ============================================================================

/// Expected errors for the svelte-modules fixture
fn modules_expected_errors() -> Vec<ExpectedError> {
    vec![
        // $props is not valid in module files
        // Line 7: `let { name } = $props<{ name: string }>();`
        ExpectedError {
            filename: "src/lib/invalid-props.svelte.ts",
            line: 7,
            code: "parse-error",
            message_contains: "$props is only valid inside .svelte component files",
        },
        // Type errors in type-errors.svelte.ts
        // Line 8: `count = "not a number";`
        ExpectedError {
            filename: "src/lib/type-errors.svelte.ts",
            line: 8,
            code: "TS2322",
            message_contains: "Type 'string' is not assignable to type 'number'",
        },
        // Line 23: `return "wrong type";`
        ExpectedError {
            filename: "src/lib/type-errors.svelte.ts",
            line: 23,
            code: "TS2322",
            message_contains: "Type 'string' is not assignable to type 'number'",
        },
    ]
}

#[test]
#[serial]
fn test_modules_exact_errors() {
    let (exit_code, diagnostics) = run_check_json("svelte-modules");

    // Verify exact errors
    assert_exact_errors(&diagnostics, &modules_expected_errors());

    // Should exit with error code since there are errors
    assert_ne!(exit_code, 0, "Expected non-zero exit code due to errors");
}

#[test]
#[serial]
fn test_modules_no_errors_in_valid_files() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // These files should have NO errors
    assert_no_errors_in_file(&diagnostics, "src/lib/counter-state.svelte.ts");
    assert_no_errors_in_file(&diagnostics, "src/lib/valid-runes.svelte.ts");
    assert_no_errors_in_file(&diagnostics, "src/lib/Counter.svelte");
    assert_no_errors_in_file(&diagnostics, "src/routes/+page.svelte");
}

#[test]
#[serial]
fn test_modules_no_import_resolution_errors() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // Should not have any module resolution errors
    assert_no_resolution_errors(&diagnostics);
}

#[test]
#[serial]
fn test_modules_props_error_location() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // Verify the $props error is on the correct line
    let props_error = diagnostics
        .iter()
        .find(|d| d.filename == "src/lib/invalid-props.svelte.ts" && d.code == "parse-error");

    assert!(
        props_error.is_some(),
        "Expected $props error in invalid-props.svelte.ts"
    );

    let error = props_error.unwrap();
    assert_eq!(
        error.start.line, 7,
        "Expected $props error on line 7, found on line {}",
        error.start.line
    );
    assert!(
        error.message.contains("$props"),
        "Error message should mention $props"
    );
}

#[test]
#[serial]
fn test_modules_type_error_locations() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // Verify type errors are on the correct lines
    let type_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename == "src/lib/type-errors.svelte.ts" && d.code == "TS2322")
        .collect();

    assert_eq!(
        type_errors.len(),
        2,
        "Expected exactly 2 type errors in type-errors.svelte.ts, found {}",
        type_errors.len()
    );

    // Check line numbers
    let lines: Vec<u32> = type_errors.iter().map(|e| e.start.line).collect();
    assert!(
        lines.contains(&8),
        "Expected type error on line 8 (count = \"not a number\")"
    );
    assert!(
        lines.contains(&23),
        "Expected type error on line 23 (return \"wrong type\")"
    );
}

#[test]
#[serial]
fn test_modules_rune_transformation_works() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // counter-state.svelte.ts uses $state, $derived, $effect
    // If rune transformation is broken, we'd see errors like:
    // - "Cannot find name '$state'"
    // - "Cannot find name '$derived'"
    let rune_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.message.contains("Cannot find name '$state'")
                || d.message.contains("Cannot find name '$derived'")
                || d.message.contains("Cannot find name '$effect'")
        })
        .collect();

    assert!(
        rune_errors.is_empty(),
        "Rune transformation is broken - runes not being transformed:\n{:#?}",
        rune_errors
    );
}

#[test]
#[serial]
fn test_modules_valid_runes_file_has_no_errors() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // valid-runes.svelte.ts contains comprehensive usage of all valid runes
    // It should have absolutely no errors
    let valid_runes_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename == "src/lib/valid-runes.svelte.ts")
        .collect();

    assert!(
        valid_runes_errors.is_empty(),
        "valid-runes.svelte.ts should have no errors, but found:\n{:#?}",
        valid_runes_errors
    );
}

#[test]
#[serial]
fn test_modules_counter_state_has_no_errors() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // counter-state.svelte.ts is a valid module file
    let counter_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename == "src/lib/counter-state.svelte.ts")
        .collect();

    assert!(
        counter_errors.is_empty(),
        "counter-state.svelte.ts should have no errors, but found:\n{:#?}",
        counter_errors
    );
}

#[test]
#[serial]
fn test_modules_page_imports_work() {
    let (_exit_code, diagnostics) = run_check_json("svelte-modules");

    // +page.svelte imports from .svelte.ts modules
    // If imports are broken, we'd see TS2307 or similar
    let page_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename == "src/routes/+page.svelte")
        .collect();

    assert!(
        page_errors.is_empty(),
        "+page.svelte should have no errors (imports from .svelte.ts modules should work), but found:\n{:#?}",
        page_errors
    );
}
