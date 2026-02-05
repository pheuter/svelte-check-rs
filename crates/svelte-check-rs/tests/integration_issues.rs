//! Integration tests for issues #19, #20, #21 (and others as they are added).
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

use bun_runner::BunRunner;
use camino::Utf8PathBuf;
use fs2::FileExt;
use serde::Deserialize;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

// ============================================================================
// TEST INFRASTRUCTURE
// ============================================================================

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Path to the test fixtures directory
fn fixtures_dir() -> PathBuf {
    workspace_root().join("test-fixtures").join("projects")
}

/// Path to the svelte-check-rs binary
fn binary_path() -> PathBuf {
    workspace_root()
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
static BIN_READY: OnceLock<()> = OnceLock::new();
static BUNDLER_READY: OnceLock<()> = OnceLock::new();
static BUNDLER_CACHE: OnceLock<(i32, Vec<JsonDiagnostic>)> = OnceLock::new();
static BUNDLER_LOCK: Mutex<()> = Mutex::new(());
static BUN_PATH: OnceLock<Utf8PathBuf> = OnceLock::new();

/// Ensures dependencies are installed for a fixture (runs once per fixture)
fn ensure_fixture_ready(fixture_path: &PathBuf, ready: &'static OnceLock<()>) {
    ready.get_or_init(|| {
        // Clean cache to ensure fresh state
        let cache_path = cache_root(fixture_path);
        let _ = fs::remove_dir_all(&cache_path);

        // Check if node_modules and tsgo exist
        let node_modules = fixture_path.join("node_modules");
        let tsgo_bin = node_modules.join(".bin/tsgo");
        if !node_modules.exists() || !tsgo_bin.exists() {
            eprintln!("Installing dependencies for sveltekit-bundler...");

            let bun_path = bun_path_for(fixture_path);
            let output = Command::new(bun_path.as_std_path())
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
        run_sveltekit_sync(fixture_path);
    });
}

fn ensure_binary_built() {
    BIN_READY.get_or_init(|| {
        let _ = Command::new("cargo")
            .args(["build", "-p", "svelte-check-rs"])
            .output();
    });
}

fn bun_path_for(workspace: &Path) -> Utf8PathBuf {
    BUN_PATH
        .get_or_init(|| {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            let workspace = Utf8PathBuf::from_path_buf(workspace.to_path_buf())
                .expect("workspace path must be utf-8");
            runtime
                .block_on(BunRunner::ensure_bun(Some(&workspace)))
                .expect("ensure bun")
        })
        .clone()
}

fn run_sveltekit_sync(fixture_path: &PathBuf) {
    let bun_path = bun_path_for(fixture_path);
    let _ = Command::new(bun_path.as_std_path())
        .args(["x", "svelte-kit", "sync"])
        .current_dir(fixture_path)
        .output();
}

fn lock_fixture(name: &str) -> std::fs::File {
    let lock_dir = workspace_root().join("target").join("test-locks");
    fs::create_dir_all(&lock_dir).expect("create lock dir");
    let lock_path = lock_dir.join(format!("{name}.lock"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .expect("open lock file");
    file.lock_exclusive().expect("lock fixture");
    file
}

fn with_bundler_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = BUNDLER_LOCK.lock().expect("lock sveltekit-bundler mutex");
    let _file_lock = lock_fixture("sveltekit-bundler");
    f()
}

fn with_modules_lock<T>(f: impl FnOnce() -> T) -> T {
    let _file_lock = lock_fixture("svelte-modules");
    f()
}

/// Runs svelte-check-rs on a fixture with JSON output (no cache, no locking)
fn run_check_json_uncached(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    // Ensure fixture is ready
    ensure_fixture_ready(fixture_path, &BUNDLER_READY);

    // Build if necessary
    ensure_binary_built();

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_path)
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

/// Runs svelte-check-rs on sveltekit-bundler with JSON output (cached)
fn run_check_json(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    BUNDLER_CACHE
        .get_or_init(|| with_bundler_lock(|| run_check_json_uncached(fixture_path)))
        .clone()
}

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
fn test_colon_in_import_no_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    // Verify no parse errors or svelte-related errors exist for this file
    assert_no_diagnostics_in_file(&diagnostics, "issue-21-colon-import/+page.svelte");
}

/// Test that regex literals with colons don't cause parsing errors.
///
/// Fixture: src/routes/issue-21-regex-colon/+page.svelte
/// Line numbers for reference:
///   Line 13: <svelte:head> - this should NOT produce an error
#[test]
fn test_regex_with_colon_no_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

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
///   Line 6: <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
///   Line 7: <div tabindex="0"> - this warning should be suppressed
#[test]
fn test_svelte_ignore_suppresses_a11y_warning() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    // Verify no a11y_no_noninteractive_tabindex warning exists for this file
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
fn test_svelte_ignore_only_affects_next_element() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    // Should have exactly ONE warning for the second div at line 9
    let expected = ExpectedDiagnostic {
        filename: "issue-20-svelte-ignore-scope/+page.svelte",
        line: 9,
        code: "a11y_no_noninteractive_tabindex",
        message_contains: "tabindex",
    };
    assert_diagnostic_present(&diagnostics, &expected);

    // Verify only one warning total for this file
    let warning_count = count_diagnostics_matching(&diagnostics, |d| {
        d.filename
            .ends_with("issue-20-svelte-ignore-scope/+page.svelte")
            && d.code == "a11y_no_noninteractive_tabindex"
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
fn test_no_svelte_ignore_produces_warning() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    // Verify warning exists on line 6 with correct code
    let expected = ExpectedDiagnostic {
        filename: "issue-20-no-pragma/+page.svelte",
        line: 6,
        code: "a11y_no_noninteractive_tabindex",
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
fn test_tsconfig_exclude_filters_svelte_diagnostics() {
    with_bundler_lock(|| {
        let fixture_path = fixtures_dir().join("sveltekit-bundler");

        // Update tsconfig.json to exclude the issue-19-excluded directory
        let tsconfig_path = fixture_path.join("tsconfig.json");
        let original_tsconfig =
            fs::read_to_string(&tsconfig_path).expect("Failed to read tsconfig");

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

        let (_exit_code, diagnostics) = run_check_json_uncached(&fixture_path);

        // Restore original tsconfig before asserting (ensures cleanup even on failure)
        fs::write(&tsconfig_path, &original_tsconfig).expect("Failed to restore tsconfig");

        // Verify no diagnostics for excluded file
        assert_no_diagnostics_in_file(&diagnostics, "issue-19-excluded/+page.svelte");
    });
}

/// Test that files NOT in exclude patterns still produce diagnostics at correct lines.
///
/// Fixture: src/routes/issue-19-not-excluded/+page.svelte
/// Line numbers for reference:
///   Line 6: <div tabindex="0"> - should produce warning on line 6
#[test]
fn test_non_excluded_files_still_checked() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    // Verify warning exists on line 6 with correct code
    let expected = ExpectedDiagnostic {
        filename: "issue-19-not-excluded/+page.svelte",
        line: 6,
        code: "a11y_no_noninteractive_tabindex",
        message_contains: "tabindex",
    };
    assert_diagnostic_present(&diagnostics, &expected);
}

// ============================================================================
// ISSUE #68: REST SPREAD PROPS TYPE ANNOTATIONS
// ============================================================================
// These tests verify that props destructuring with rest spread and an
// intersection type annotation is accepted, and that genuine type errors
// in the same pattern are still reported.
//
// Test files:
// - test-fixtures/projects/sveltekit-bundler/src/routes/issue-68-rest-props/+page.svelte
// - test-fixtures/projects/sveltekit-bundler/src/routes/issue-68-rest-props-invalid/+page.svelte
#[test]
fn test_issue_68_rest_props_no_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Verify no TS diagnostics exist for the valid rest props fixture
    assert_no_diagnostics_in_file(&diagnostics, "issue-68-rest-props/+page.svelte");
}

#[test]
fn test_issue_68_rest_props_reports_type_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Verify a deliberate type error is still reported for the invalid fixture
    let expected = ExpectedDiagnostic {
        filename: "issue-68-rest-props-invalid/+page.svelte",
        line: 8,
        code: "TS2322",
        message_contains: "not assignable",
    };
    assert_diagnostic_present(&diagnostics, &expected);
}

// ============================================================================
// ISSUE #74: COMPUTED PROPERTY NAMES IN MOUNT PROPS
// ============================================================================
// This test verifies that computed property names in mount() props do not
// produce type errors for valid component props.
//
// Test file:
// - test-fixtures/projects/sveltekit-bundler/src/lib/issue-74-mount.ts
#[test]
fn test_issue_74_computed_props_in_mount_no_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "lib/issue-74-mount.ts");
}

#[test]
fn test_issue_74_computed_props_missing_required_prop_reports_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    let expected = ExpectedDiagnostic {
        filename: "lib/issue-74-mount-invalid.ts",
        line: 9,
        code: "TS2769",
        message_contains: "No overload matches this call",
    };
    assert_diagnostic_present(&diagnostics, &expected);
}

// ============================================================================
// ISSUE #77: MULTI-LINE QUOTED STYLE DIRECTIVE VALUES
// ============================================================================
// This test verifies that multi-line quoted style directive values do not
// produce TypeScript parse errors in the generated output.
//
// Test file:
// - test-fixtures/projects/sveltekit-bundler/src/routes/issue-77-multiline-style/+page.svelte
#[test]
fn test_issue_77_multiline_style_directive_no_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "issue-77-multiline-style/+page.svelte");
}

// ============================================================================
// ISSUE #77: MULTI-LINE QUOTED NORMAL ATTRIBUTE VALUES
// ============================================================================
// This test verifies that multi-line quoted normal attributes do not
// produce TypeScript parse errors in the generated output.
//
// Test file:
// - test-fixtures/projects/sveltekit-bundler/src/routes/issue-77-multiline-attr/+page.svelte
#[test]
fn test_issue_77_multiline_normal_attribute_no_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "issue-77-multiline-attr/+page.svelte");
}

// ============================================================================
// ISSUE #79: MOUNT() RETURN TYPE INCLUDES COMPONENT EXPORTS
// ============================================================================
// This test verifies that component exports declared via `export { ... }`
// are reflected in the type of the object returned from `mount()`.
//
// Test file:
// - test-fixtures/projects/sveltekit-bundler/src/lib/issue-79-mount.ts
#[test]
fn test_issue_79_mount_exports_no_error() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "lib/issue-79-mount.ts");
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
fn test_tsconfig_exclude_wildcard_patterns() {
    with_bundler_lock(|| {
        let fixture_path = fixtures_dir().join("sveltekit-bundler");

        // Update tsconfig.json with wildcard exclude patterns
        let tsconfig_path = fixture_path.join("tsconfig.json");
        let original_tsconfig =
            fs::read_to_string(&tsconfig_path).expect("Failed to read tsconfig");

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

        let (_exit_code, diagnostics) = run_check_json_uncached(&fixture_path);
        let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

        // Restore original tsconfig before asserting
        fs::write(&tsconfig_path, &original_tsconfig).expect("Failed to restore tsconfig");

        // Verify no diagnostics for files in excluded test directories
        assert_no_diagnostics_in_file(&diagnostics, "__tests__/TestComponent.svelte");
        assert_no_diagnostics_in_file(&diagnostics, "spec/SpecComponent.svelte");
    });
}

// ============================================================================
// ISSUE #35: SVELTE.TS FILES WITH RUNES
// ============================================================================
// These tests verify that .svelte.ts files with Svelte 5 runes ($state, $derived,
// etc.) are properly transformed before being passed to tsgo, and that no
// TypeScript parse errors occur.
//
// The bug was that when workspace paths contain "./" (e.g., --workspace ./project),
// the exclude patterns for .svelte.ts files might not match due to path
// normalization inconsistencies.
//
// Test files: test-fixtures/projects/svelte-modules/src/lib/*.svelte.ts

/// Fixture state tracking for svelte-modules
static MODULES_READY: OnceLock<()> = OnceLock::new();
static MODULES_CACHE: OnceLock<(i32, Vec<JsonDiagnostic>)> = OnceLock::new();

/// Ensures dependencies are installed for svelte-modules fixture
fn ensure_modules_fixture_ready(fixture_path: &PathBuf) {
    MODULES_READY.get_or_init(|| {
        // Clean cache to ensure fresh state
        let cache_path = cache_root(fixture_path);
        let _ = fs::remove_dir_all(&cache_path);

        // Check if node_modules and tsgo exist
        let node_modules = fixture_path.join("node_modules");
        let tsgo_bin = node_modules.join(".bin/tsgo");
        if !node_modules.exists() || !tsgo_bin.exists() {
            eprintln!("Installing dependencies for svelte-modules...");

            let bun_path = bun_path_for(fixture_path);
            let output = Command::new(bun_path.as_std_path())
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
        run_sveltekit_sync(fixture_path);
    });
}

/// Runs svelte-check-rs on svelte-modules fixture with JSON output
fn run_modules_check_json_uncached(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    // Ensure fixture is ready
    ensure_modules_fixture_ready(fixture_path);

    // Build if necessary
    ensure_binary_built();

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_path)
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

/// Runs svelte-check-rs on svelte-modules with JSON output (cached)
fn run_modules_check_json(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    MODULES_CACHE
        .get_or_init(|| with_modules_lock(|| run_modules_check_json_uncached(fixture_path)))
        .clone()
}

/// Test that .svelte.ts files with runes don't produce TypeScript parse errors.
///
/// This reproduces issue #35 where .svelte.ts files using Svelte 5 runes like:
///   let count = $state(0);
///   let doubled = $derived(count * 2);
/// caused TypeScript parse errors like "')' expected" (TS1005).
///
/// The bug occurs when the workspace path contains "./" which causes path
/// normalization mismatches between include and exclude patterns.
///
/// Fixture: test-fixtures/projects/svelte-modules/src/lib/*.svelte.ts
#[test]
fn test_svelte_ts_files_no_parse_errors() {
    let fixture_path = fixtures_dir().join("svelte-modules");
    let (_exit_code, diagnostics) = run_modules_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Check that no TS1005 (parse error) diagnostics exist for .svelte.ts files
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.ends_with(".svelte.ts")
                && (d.code.contains("TS1005")
                    || d.code.contains("TS1003")
                    || d.code.contains("TS1002")
                    || d.message.contains("expected"))
        })
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Found TypeScript parse errors in .svelte.ts files (indicates untransformed runes):\n{:#?}",
        parse_errors
    );
}

/// Test that .svelte.ts files with runes work correctly when using relative path with ./
///
/// This specifically tests the path normalization issue where --workspace ./path
/// causes the exclude patterns to have "./" but include patterns don't.
#[test]
fn test_svelte_ts_files_relative_path_with_dot() {
    let fixture_path = fixtures_dir().join("svelte-modules");

    // Ensure fixture is ready
    ensure_modules_fixture_ready(&fixture_path);

    // Build if necessary
    ensure_binary_built();

    // Use a relative path with ./ prefix (the problematic case)
    // We need to run from the fixtures parent directory
    let fixtures_parent = fixtures_dir();
    let relative_path = format!("./{}", "svelte-modules");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(&relative_path)
        .arg("--output")
        .arg("json")
        .current_dir(&fixtures_parent)
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON output: {}", e);
        eprintln!("Raw output:\n{}", stdout);
        vec![]
    });
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Check that no TS1005 (parse error) diagnostics exist for .svelte.ts files
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.ends_with(".svelte.ts")
                && (d.code.contains("TS1005")
                    || d.code.contains("TS1003")
                    || d.code.contains("TS1002")
                    || d.message.contains("expected"))
        })
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Found TypeScript parse errors in .svelte.ts files when using relative path with ./:\n{:#?}",
        parse_errors
    );
}

/// Test that multiline $state<T>(value) with trailing commas is correctly transformed.
///
/// This is the specific bug from issue #35 where patterns like:
///   $state<'a' | 'b'>(
///       'a',
///   )
/// were being transformed to invalid TypeScript:
///   ('a', as 'a' | 'b')
/// instead of:
///   ('a' as 'a' | 'b')
///
/// Fixture: test-fixtures/projects/svelte-modules/src/lib/issue-35-multiline-runes.svelte.ts
#[test]
fn test_issue_35_multiline_state_with_trailing_comma() {
    let fixture_path = fixtures_dir().join("svelte-modules");
    let (_exit_code, diagnostics) = run_modules_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Check that no parse errors exist for the issue-35 test file specifically
    let issue_35_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.contains("issue-35-multiline-runes")
                && (d.code.contains("TS1005")
                    || d.code.contains("TS1003")
                    || d.code.contains("TS1002")
                    || d.code.contains("TS1109")
                    || d.message.contains("expected")
                    || d.message.contains("Expression expected"))
        })
        .collect();

    assert!(
        issue_35_errors.is_empty(),
        "Issue #35: Multiline $state<T>(value) with trailing comma produced parse errors:\n{:#?}",
        issue_35_errors
    );
}

/// Test that multiline runes in .svelte component scripts also work correctly.
///
/// This ensures the fix applies to component scripts, not just .svelte.ts module files.
///
/// Fixture: test-fixtures/projects/svelte-modules/src/lib/Issue35Component.svelte
#[test]
fn test_issue_35_svelte_component_multiline_runes() {
    let fixture_path = fixtures_dir().join("svelte-modules");
    let (_exit_code, diagnostics) = run_modules_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Check that no parse errors exist for the .svelte component
    let component_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.contains("Issue35Component.svelte")
                && (d.code.contains("TS1005")
                    || d.code.contains("TS1003")
                    || d.code.contains("TS1002")
                    || d.code.contains("TS1109")
                    || d.message.contains("expected")
                    || d.message.contains("Expression expected"))
        })
        .collect();

    assert!(
        component_errors.is_empty(),
        "Issue #35: .svelte component with multiline runes produced parse errors:\n{:#?}",
        component_errors
    );
}

/// Test that the transformed output for multiline runes is valid by checking the cache.
///
/// This test verifies that the transformation produces valid TypeScript by
/// examining the cached output file.
#[test]
fn test_issue_35_transformed_output_is_valid() {
    let fixture_path = fixtures_dir().join("svelte-modules");

    // Run the check to generate cache
    let _ = run_modules_check_json(&fixture_path);

    // Check the cached transformed file
    let cache_file = cache_root(&fixture_path)
        .join("src")
        .join("lib")
        .join("issue-35-multiline-runes.svelte.ts");

    if cache_file.exists() {
        let content = fs::read_to_string(&cache_file).expect("Failed to read cache file");

        // Skip comments when checking for ", as" pattern
        // The fixture file has comments explaining the bug which contain ", as"
        // We need to check the actual code, not the comments
        let code_lines: Vec<&str> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("//")
                    && !trimmed.starts_with("*")
                    && !trimmed.starts_with("/*")
            })
            .collect();
        let code_only = code_lines.join("\n");

        // The transformed code should NOT contain ", as" pattern (comma before as)
        assert!(
            !code_only.contains(", as"),
            "Transformed code contains invalid ', as' pattern:\n{}",
            code_only
        );

        // The transformed content should contain valid 'as' casts
        // (checking for pattern like "value as Type" without leading comma)
        assert!(
            content.contains("'status-start-title' as 'status-start-title'"),
            "Transformed file should contain valid 'as' cast:\n{}",
            content
        );
    }
}

// ============================================================================
// ISSUE #36, #37, #38: PARSER FIXES
// ============================================================================
// These tests verify that the parser correctly handles:
// - Issue #36: Dot notation in component names and attribute names (T.Mesh, rotation.x)
// - Issue #37: CSS custom property attributes (--primary-color="value")
// - Issue #38: HTML void elements without closing tags (<br>, <hr>, <img>)
//
// Test fixtures:
// - test-fixtures/valid/parser/issue-36-dot-notation.svelte
// - test-fixtures/valid/parser/issue-37-css-custom-props.svelte
// - test-fixtures/valid/parser/issue-38-void-elements.svelte
// - test-fixtures/projects/sveltekit-bundler/src/routes/test-issues/+page.svelte

/// Test that HTML void elements don't cause parsing errors.
///
/// Issue #38: Void elements like <br>, <hr>, <img> should not require closing tags.
///
/// Fixture: src/routes/test-issues/+page.svelte
/// Contains void elements without closing tags
#[test]
fn test_issue_38_void_elements_no_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    // Verify no parse errors for test-issues file
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.ends_with("test-issues/+page.svelte")
                && (d.code.contains("parse") || d.message.contains("Expected closing tag"))
        })
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #38: Void elements caused parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that void elements fixture parses without errors using --skip-tsgo.
///
/// This tests the parser directly without involving TypeScript checking.
#[test]
fn test_issue_38_void_elements_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("parser")
        .join("issue-38-void-elements.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("Expected closing tag") || d.code.contains("parse"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #38: Void elements fixture has parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that dot notation in attribute names parses correctly.
///
/// Issue #36: Attribute names like rotation.x should be valid.
///
/// This test verifies the parser can handle dot notation in attributes
/// as used by libraries like threlte.
#[test]
fn test_issue_36_dot_notation_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("parser")
        .join("issue-36-dot-notation.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("Unexpected token"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #36: Dot notation in attributes has parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that CSS custom property attributes parse correctly.
///
/// Issue #37: Attributes like --primary-color="red" should be valid on components.
///
/// This test verifies the parser can handle CSS custom property syntax in attributes.
#[test]
fn test_issue_37_css_custom_properties_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("parser")
        .join("issue-37-css-custom-props.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("Unexpected token"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #37: CSS custom property attributes have parse errors:\n{:#?}",
        parse_errors
    );
}

// ============================================================================
// ISSUE #46: REGEX LITERALS IN EXPRESSIONS
// ============================================================================
// These tests verify that regex literals in expressions are parsed correctly.
//
// The bug was that the expression parser (read_expression_until) did not handle
// regex literals, causing the depth counter to be affected by ()/[]/{}
// characters inside regex patterns.
//
// Test fixtures:
// - test-fixtures/valid/issues/issue-46-regex-simple.svelte
// - test-fixtures/valid/issues/issue-46-regex-const.svelte
// - test-fixtures/valid/issues/issue-46-regex-snippet.svelte
// - test-fixtures/valid/issues/issue-46-regex-arrow-iife.svelte
// - test-fixtures/valid/issues/issue-46-regex-edge-cases.svelte

/// Test that simple regex literals parse correctly.
///
/// Issue #46: Basic regex patterns like /test/ should not cause parsing issues.
#[test]
fn test_issue_46_regex_simple_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("issues")
        .join("issue-46-regex-simple.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("unclosed"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #46: Simple regex caused parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that @const with regex match and complex patterns parses correctly.
///
/// Issue #46: Regex patterns like /^(.+?)\s*\(([^)]+)\)$/ in @const tags
/// should not cause the parser to lose track of expression boundaries.
#[test]
fn test_issue_46_regex_const_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("issues")
        .join("issue-46-regex-const.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("unclosed"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #46: @const with regex caused parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that snippets with @const and regex parse correctly.
///
/// Issue #46: The main reproduction case - snippets containing @const with
/// complex regex patterns that have parentheses, brackets, and braces.
#[test]
fn test_issue_46_regex_snippet_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("issues")
        .join("issue-46-regex-snippet.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors - the key indicator of issue #46 is "unclosed tag"
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("unclosed"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #46: Snippet with @const and regex caused parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that @const with typed arrow functions and IIFEs containing regex parses correctly.
///
/// Issue #46: Arrow functions with type annotations and IIFEs containing regex
/// should be parsed correctly without "Expression expected" errors.
#[test]
fn test_issue_46_regex_arrow_iife_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("issues")
        .join("issue-46-regex-arrow-iife.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("unclosed"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #46: Arrow function/IIFE with regex caused parse errors:\n{:#?}",
        parse_errors
    );
}

/// Test that regex edge cases (quantifiers, escapes, char classes) parse correctly.
///
/// Issue #46: Edge cases like:
/// - Regex with {n,m} quantifiers: /\d{2,4}/
/// - Regex with } in character class: /[{}]+/
/// - Regex with ) in character class: /[^)]+/
/// - Division operators that should NOT be treated as regex
#[test]
fn test_issue_46_regex_edge_cases_parser_only() {
    // Build if necessary
    ensure_binary_built();

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("valid")
        .join("issues")
        .join("issue-46-regex-edge-cases.svelte");

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_file.parent().unwrap().parent().unwrap())
        .arg("--single-file")
        .arg(&fixture_file)
        .arg("--skip-tsgo")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // Should have no parse errors
    let parse_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.contains("parse") || d.message.contains("unclosed"))
        .collect();

    assert!(
        parse_errors.is_empty(),
        "Issue #46: Regex edge cases caused parse errors:\n{:#?}",
        parse_errors
    );
}

// ============================================================================
// ISSUES #87-#90: ATTACHMENTS, DYNAMIC COMPONENTS, SNAPSHOT EXPORTS, SNIPPETS
// ============================================================================

/// Verify that issues #87-#90 do not produce TypeScript diagnostics.
#[test]
fn test_issue_87_90_no_ts_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "issue-87-attach-details/+page.svelte");
    assert_no_diagnostics_in_file(&diagnostics, "issue-88-dynamic-components/+page.svelte");
    assert_no_diagnostics_in_file(&diagnostics, "snapshot-export/snapshot-test.svelte.ts");
    assert_no_diagnostics_in_file(&diagnostics, "snippet-passing/+page.svelte");
}

/// Verify complex snippet props remain contextually typed (regression guard).
#[test]
fn test_complex_snippet_contextual_typing_no_ts_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(
        &diagnostics,
        "complex-snippet-contextual-typing/+page.svelte",
    );
}

/// Verify namespace components + generic props + snippets remain contextually typed.
#[test]
fn test_namespace_component_generic_snippets_no_ts_errors() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(
        &diagnostics,
        "namespace-component-generic-snippets/+page.svelte",
    );
}

// ============================================================================
// ISSUE #93: TYPE NARROWING IN TEMPLATES
// ============================================================================
// These tests verify that type narrowing from script blocks is correctly
// propagated to template expressions. TypeScript's control flow analysis
// should recognize narrowing patterns like:
//   - `if (!x) throw ...` - x is narrowed after the throw
//   - `if (!x) return` - x is narrowed after the return
//   - Type guard functions - x is narrowed after the guard
//
// The bug was that svelte-check-rs generated the template check as a separate
// function, breaking TypeScript's control flow analysis which only works
// within a single scope.
//
// Test files: test-fixtures/projects/sveltekit-bundler/src/routes/issue-93-*/
/// Test that type narrowing after throw is recognized in templates.
///
/// Issue #93: After `if (!x) { throw ... }`, x should be narrowed to exclude
/// undefined/null in the template.
///
/// Fixture: src/routes/issue-93-type-narrowing-throw/+page.svelte
/// Line numbers for reference:
///   Line 17-19: if (!mp_trj_data) { throw ... }
///   Line 25: <a href={mp_trj_data.figshare}> - should NOT produce "possibly undefined" error
#[test]
fn test_issue_93_type_narrowing_after_throw() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Verify no "possibly undefined" errors for this file
    let undefined_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename
                .ends_with("issue-93-type-narrowing-throw/+page.svelte")
                && (d.code.contains("TS18048") || d.message.contains("possibly 'undefined'"))
        })
        .collect();

    assert!(
        undefined_errors.is_empty(),
        "Issue #93: Type narrowing after throw not recognized in template:\n{:#?}",
        undefined_errors
    );
}

/// Test that type narrowing with type guards works in templates.
///
/// This tests that type guard functions (`x is T`) correctly narrow types
/// in the template after a guard check.
///
/// Fixture: src/routes/issue-93-type-narrowing-guard/+page.svelte
#[test]
fn test_issue_93_type_narrowing_with_guards() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Verify no type errors for this file
    let type_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename
                .ends_with("issue-93-type-narrowing-guard/+page.svelte")
                && d.code.starts_with("TS")
        })
        .collect();

    assert!(
        type_errors.is_empty(),
        "Issue #93: Type guard narrowing not recognized in template:\n{:#?}",
        type_errors
    );
}

/// Test that {#if} blocks in templates work correctly with nullable types.
///
/// This tests that `{#if data.user}` correctly narrows data.user inside the block.
///
/// Fixture: src/routes/issue-93-type-narrowing-return/+page.svelte
#[test]
fn test_issue_93_if_block_narrowing() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    // Verify no null/undefined errors for this file
    let null_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename
                .ends_with("issue-93-type-narrowing-return/+page.svelte")
                && (d.code.contains("TS18048")
                    || d.code.contains("TS18047")
                    || d.message.contains("possibly 'null'")
                    || d.message.contains("possibly 'undefined'"))
        })
        .collect();

    assert!(
        null_errors.is_empty(),
        "Issue #93: If-block narrowing not working correctly:\n{:#?}",
        null_errors
    );
}

/// Test that store alias declarations inside the render function do not
/// conflict with template store usage.
///
/// Regression: template store declarations were emitted alongside render
/// scope store aliases, causing duplicate `$store` declarations and
/// "Modifiers cannot appear here" errors in SvelteKit apps.
///
/// Fixture: src/routes/issue-93-store-alias/+page.svelte
#[test]
fn test_issue_93_store_alias_no_redeclare() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "issue-93-store-alias/+page.svelte");
}

/// Test that module script exports can reference top-level snippets without
/// triggering "Cannot find name" errors.
///
/// Fixture: src/routes/issue-93-snippet-export/+page.svelte
#[test]
fn test_issue_93_snippet_module_export() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(&diagnostics, "issue-93-snippet-export/+page.svelte");
}

/// Test that snippets referencing instance-only types are not hoisted into
/// module scope, avoiding false "Cannot find name" errors.
///
/// Fixture: src/routes/issue-93-snippet-instance-typeof/+page.svelte
#[test]
fn test_issue_93_snippet_instance_typeof() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "ts");

    assert_no_diagnostics_in_file(
        &diagnostics,
        "issue-93-snippet-instance-typeof/+page.svelte",
    );
}

/// Issue #96: Label wrapping a component should not trigger a11y-label-has-associated-control.
///
/// Fixture: src/routes/issue-96-label-component/+page.svelte
#[test]
fn test_issue_96_label_component_no_a11y() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    assert_no_diagnostics_in_file(&diagnostics, "issue-96-label-component/+page.svelte");
}

/// Issue #96: Click handlers on td inside role="grid" should not require key handlers.
///
/// Fixture: src/routes/issue-96-click-on-td/+page.svelte
#[test]
fn test_issue_96_click_on_td_in_grid_no_a11y() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    assert_no_diagnostics_in_file(&diagnostics, "issue-96-click-on-td/+page.svelte");
}

/// Issue #96: state-referenced-locally warnings should not be duplicated.
///
/// Fixture: src/routes/issue-96-duplicate-warnings/+page.svelte
#[test]
fn test_issue_96_state_referenced_locally_not_duplicated() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);
    let diagnostics = filter_diagnostics_by_source(&diagnostics, "svelte");

    let count = count_diagnostics_matching(&diagnostics, |d| {
        d.filename
            .ends_with("issue-96-duplicate-warnings/+page.svelte")
            && d.code == "state_referenced_locally"
    });

    assert_eq!(
        count, 1,
        "Expected a single state-referenced-locally warning, got {}:\n{:#?}",
        count, diagnostics
    );
}
