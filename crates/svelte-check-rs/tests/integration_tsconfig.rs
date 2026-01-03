//! Integration tests for different TypeScript configurations.
//!
//! These tests verify that svelte-check-rs correctly detects type errors
//! across different tsconfig module resolution strategies (bundler, NodeNext, Node16).
//!
//! Each test fixture contains:
//! - SvelteKit-style route files (+page.svelte)
//! - Server load functions (+page.server.ts)
//! - Shared components with typed props
//! - Intentional type errors to verify detection
//!
//! Note: Tests are serialized using #[serial] to avoid race conditions during
//! fixture setup (bun install creates bun.lock before node_modules is complete).

use serial_test::serial;
use std::process::Command;
use std::sync::OnceLock;

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

// Track which fixtures have been set up
static BUNDLER_READY: OnceLock<()> = OnceLock::new();
static NODENEXT_READY: OnceLock<()> = OnceLock::new();
static NODE16_READY: OnceLock<()> = OnceLock::new();

/// Ensures dependencies are installed for a fixture (runs once per fixture)
fn ensure_deps_installed(fixture_name: &str, ready: &'static OnceLock<()>) {
    // Use get_or_init to ensure we only install once per fixture
    ready.get_or_init(|| {
        let fixture_path = fixtures_dir().join(fixture_name);

        // Check if node_modules exists (more reliable than bun.lock which is created early)
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
    });
}

/// Runs svelte-check-rs on a fixture directory and returns (exit_code, stdout, stderr)
fn run_check(fixture_name: &str) -> (i32, String, String) {
    // Map fixture name to its ready flag
    let ready = match fixture_name {
        "sveltekit-bundler" => &BUNDLER_READY,
        "sveltekit-nodenext" => &NODENEXT_READY,
        "sveltekit-node16" => &NODE16_READY,
        _ => panic!("Unknown fixture: {}", fixture_name),
    };

    // Ensure dependencies are installed
    ensure_deps_installed(fixture_name, ready);

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
        .arg("js") // Only check TypeScript errors for these tests
        .output()
        .expect("Failed to execute svelte-check-rs");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Combine stdout and stderr for error checking (output goes to stdout)
    let combined = format!("{}\n{}", stdout, stderr);

    (exit_code, combined.clone(), combined)
}

/// Verifies that a specific error pattern is found in the output
fn assert_error_detected(output: &str, pattern: &str, context: &str) {
    assert!(
        output.contains(pattern),
        "Expected to find error pattern '{}' in output for {}.\nActual output:\n{}",
        pattern,
        context,
        output
    );
}

// ============================================================================
// BUNDLER MODULE RESOLUTION TESTS
// ============================================================================

#[test]
#[serial]
fn test_sveltekit_bundler_detects_invalid_props() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-bundler");

    // Should exit with error code (found errors)
    assert_ne!(exit_code, 0, "Expected errors to be found");

    // Should detect invalid prop 'wrong' on Button component
    assert_error_detected(&stderr, "wrong", "bundler: invalid prop on Button");
}

#[test]
#[serial]
fn test_sveltekit_bundler_detects_invalid_variant() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-bundler");

    assert_ne!(exit_code, 0, "Expected errors to be found");

    // Should detect invalid variant value
    assert_error_detected(&stderr, "invalid", "bundler: invalid variant value");
}

#[test]
#[serial]
fn test_sveltekit_bundler_detects_page_data_errors() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-bundler");

    assert_ne!(exit_code, 0, "Expected errors to be found");

    // Should detect accessing 'comments' when server returns 'posts'
    assert_error_detected(
        &stderr,
        "comments",
        "bundler: accessing non-existent PageData property",
    );

    // Should detect accessing 'author' on post when server returns {id, title, content}
    assert_error_detected(
        &stderr,
        "author",
        "bundler: accessing non-existent post property",
    );
}

// ============================================================================
// NODENEXT MODULE RESOLUTION TESTS
// ============================================================================

#[test]
#[serial]
fn test_sveltekit_nodenext_detects_invalid_props() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-nodenext");

    // Should exit with error code (found errors)
    assert_ne!(exit_code, 0, "Expected errors to be found with NodeNext");

    // Should detect invalid prop 'wrong' on Button component
    // This is the key test - NodeNext was previously missing these errors
    assert_error_detected(&stderr, "wrong", "nodenext: invalid prop on Button");
}

#[test]
#[serial]
fn test_sveltekit_nodenext_detects_invalid_variant() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-nodenext");

    assert_ne!(exit_code, 0, "Expected errors to be found with NodeNext");

    // Should detect invalid variant value
    assert_error_detected(&stderr, "invalid", "nodenext: invalid variant value");
}

// NOTE: PageData type tests are skipped for NodeNext because $types imports
// require explicit file extensions which SvelteKit's generated types don't provide.
// The key issue (#4) - component prop detection - is tested above.

// ============================================================================
// NODE16 MODULE RESOLUTION TESTS
// ============================================================================

#[test]
#[serial]
fn test_sveltekit_node16_detects_invalid_props() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-node16");

    // Should exit with error code (found errors)
    assert_ne!(exit_code, 0, "Expected errors to be found with Node16");

    // Should detect invalid prop 'wrong' on Button component
    assert_error_detected(&stderr, "wrong", "node16: invalid prop on Button");
}

#[test]
#[serial]
fn test_sveltekit_node16_detects_invalid_variant() {
    let (exit_code, _stdout, stderr) = run_check("sveltekit-node16");

    assert_ne!(exit_code, 0, "Expected errors to be found with Node16");

    // Should detect invalid variant value
    assert_error_detected(&stderr, "invalid", "node16: invalid variant value");
}

// NOTE: PageData type tests are skipped for Node16 because $types imports
// require explicit file extensions which SvelteKit's generated types don't provide.
// The key issue (#4) - component prop detection - is tested above.

// ============================================================================
// CROSS-CONFIG PARITY TESTS
// ============================================================================

#[test]
#[serial]
fn test_all_configs_detect_same_errors() {
    // All three configurations should detect the same errors
    let (bundler_exit, _, bundler_stderr) = run_check("sveltekit-bundler");
    let (nodenext_exit, _, nodenext_stderr) = run_check("sveltekit-nodenext");
    let (node16_exit, _, node16_stderr) = run_check("sveltekit-node16");

    // All should find errors
    assert_ne!(bundler_exit, 0, "bundler should find errors");
    assert_ne!(nodenext_exit, 0, "nodenext should find errors");
    assert_ne!(node16_exit, 0, "node16 should find errors");

    // All should detect the 'wrong' prop error
    assert!(
        bundler_stderr.contains("wrong"),
        "bundler should detect 'wrong' prop"
    );
    assert!(
        nodenext_stderr.contains("wrong"),
        "nodenext should detect 'wrong' prop"
    );
    assert!(
        node16_stderr.contains("wrong"),
        "node16 should detect 'wrong' prop"
    );

    // All should detect the 'invalid' variant error
    assert!(
        bundler_stderr.contains("invalid"),
        "bundler should detect 'invalid' variant"
    );
    assert!(
        nodenext_stderr.contains("invalid"),
        "nodenext should detect 'invalid' variant"
    );
    assert!(
        node16_stderr.contains("invalid"),
        "node16 should detect 'invalid' variant"
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
    let (exit_code, _stdout, stderr) = run_check("sveltekit-nodenext");

    // The key assertion: NodeNext should detect prop errors just like bundler does
    assert_ne!(
        exit_code, 0,
        "REGRESSION: NodeNext should detect type errors (issue #4)"
    );

    // Should specifically detect the 'wrong' prop which was being missed
    assert!(
        stderr.contains("wrong") || stderr.contains("TS2353"),
        "REGRESSION: NodeNext should detect invalid props on imported components (issue #4)"
    );
}
