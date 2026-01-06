//! Integration tests for issues #19, #20, #21.
//!
//! These tests verify that:
//! - Issue #21: Imports with colons (like `virtual:pwa-register`) don't cause parsing errors
//! - Issue #20: `<!-- svelte-ignore -->` pragma comments suppress warnings
//! - Issue #19: tsconfig `exclude` patterns filter Svelte diagnostics
//!
//! Note: These tests are skipped on Windows due to tsgo/path handling differences.

#![cfg(not(target_os = "windows"))]

use serde::Deserialize;
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

/// Runs svelte-check-rs on a fixture with JSON output
fn run_check_json(fixture_path: &PathBuf, diagnostic_sources: &str) -> (i32, Vec<JsonDiagnostic>) {
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

/// Test that imports with colons (like `virtual:pwa-register`) don't cause parsing errors.
///
/// This reproduces issue #21 where imports like:
///   import { registerSW } from 'virtual:pwa-register';
/// caused spurious errors on `<svelte:head>` and `:global()` constructs.
#[test]
#[serial]
fn test_colon_in_import_no_errors() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache to start fresh
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    // Create a Svelte file with a colon-containing import
    let test_svelte = fixture_path.join("src/ColonImport.svelte");
    let svelte_content = r#"<script lang="ts">
    // Simulating imports with colons in the path (like virtual:pwa-register)
    // This should NOT cause any parsing errors
    const virtualImport = 'virtual:pwa-register';
    const anotherColon = "some:value:here";

    let count = $state(0);
</script>

<svelte:head>
    <title>Test Page</title>
</svelte:head>

<button onclick={() => count++}>
    Count: {count}
</button>

<style>
    :global(body) {
        margin: 0;
    }
</style>
"#;
    fs::write(&test_svelte, svelte_content).expect("Failed to write Svelte file");

    // Run svelte-check-rs
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "js,svelte");

    // There should be no errors related to <svelte:head> or :global()
    let svelte_head_errors = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("ColonImport.svelte")
            && (d.message.contains("svelte:head") || d.message.contains(":global"))
    });

    assert_eq!(
        svelte_head_errors,
        0,
        "ISSUE #21 REGRESSION: Colon in import string should not cause errors on <svelte:head> or :global(). \
         Found errors: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("ColonImport"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&test_svelte);
}

/// Test that regex literals with colons don't cause parsing errors.
#[test]
#[serial]
fn test_regex_with_colon_no_errors() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    let test_svelte = fixture_path.join("src/RegexColon.svelte");
    let svelte_content = r#"<script lang="ts">
    // Regex literals with colons should not confuse the parser
    const timeRegex = /\d{2}:\d{2}:\d{2}/;
    const urlRegex = /https?:\/\/[^\s]+/;

    let value = $state('12:30:45');

    function isValidTime(str: string): boolean {
        return timeRegex.test(str);
    }
</script>

<svelte:head>
    <title>Regex Test</title>
</svelte:head>

<p>{isValidTime(value) ? 'Valid' : 'Invalid'}</p>
"#;
    fs::write(&test_svelte, svelte_content).expect("Failed to write Svelte file");

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "js,svelte");

    let regex_errors = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("RegexColon.svelte")
            && (d.message.contains("svelte:head") || d.message.contains(":global"))
    });

    assert_eq!(
        regex_errors,
        0,
        "ISSUE #21 REGRESSION: Regex literals with colons should not cause parsing errors. \
         Found errors: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("RegexColon"))
            .collect::<Vec<_>>()
    );

    let _ = fs::remove_file(&test_svelte);
}

// ============================================================================
// ISSUE #20: SVELTE-IGNORE PRAGMA
// ============================================================================

/// Test that `<!-- svelte-ignore -->` pragma suppresses a11y warnings.
#[test]
#[serial]
fn test_svelte_ignore_suppresses_a11y_warning() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    let test_svelte = fixture_path.join("src/SvelteIgnore.svelte");
    let svelte_content = r#"<script lang="ts">
    let value = $state('test');
</script>

<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
<div tabindex="0">
    This div has tabindex but the warning should be suppressed
</div>

<p>{value}</p>
"#;
    fs::write(&test_svelte, svelte_content).expect("Failed to write Svelte file");

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // The a11y warning should be suppressed
    let tabindex_warnings = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("SvelteIgnore.svelte") && d.code == "a11y-no-noninteractive-tabindex"
    });

    assert_eq!(
        tabindex_warnings,
        0,
        "ISSUE #20 REGRESSION: svelte-ignore pragma should suppress a11y-no-noninteractive-tabindex warning. \
         Found warnings: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("SvelteIgnore"))
            .collect::<Vec<_>>()
    );

    let _ = fs::remove_file(&test_svelte);
}

/// Test that svelte-ignore only affects the next element, not subsequent ones.
#[test]
#[serial]
fn test_svelte_ignore_only_affects_next_element() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    let test_svelte = fixture_path.join("src/SvelteIgnoreScope.svelte");
    let svelte_content = r#"<script lang="ts">
</script>

<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
<div tabindex="0">First div - warning suppressed</div>

<!-- This second div should still produce a warning -->
<div tabindex="0">Second div - warning NOT suppressed</div>
"#;
    fs::write(&test_svelte, svelte_content).expect("Failed to write Svelte file");

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Should have exactly ONE warning (for the second div)
    let tabindex_warnings = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("SvelteIgnoreScope.svelte")
            && d.code == "a11y-no-noninteractive-tabindex"
    });

    assert_eq!(
        tabindex_warnings,
        1,
        "ISSUE #20: svelte-ignore should only affect the next element. \
         Expected 1 warning (for second div), found: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("SvelteIgnoreScope"))
            .collect::<Vec<_>>()
    );

    let _ = fs::remove_file(&test_svelte);
}

/// Test that svelte-ignore works without pragma and produces warnings.
#[test]
#[serial]
fn test_no_svelte_ignore_produces_warning() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    let test_svelte = fixture_path.join("src/NoPragma.svelte");
    let svelte_content = r#"<script lang="ts">
</script>

<!-- No svelte-ignore pragma here -->
<div tabindex="0">This should produce a warning</div>
"#;
    fs::write(&test_svelte, svelte_content).expect("Failed to write Svelte file");

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    let tabindex_warnings = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("NoPragma.svelte") && d.code == "a11y-no-noninteractive-tabindex"
    });

    assert!(
        tabindex_warnings > 0,
        "Without svelte-ignore pragma, a11y warning should be produced. \
         Found diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("NoPragma"))
            .collect::<Vec<_>>()
    );

    let _ = fs::remove_file(&test_svelte);
}

// ============================================================================
// ISSUE #19: TSCONFIG EXCLUDE PATTERNS
// ============================================================================

/// Test that tsconfig `exclude` patterns filter out Svelte diagnostics.
#[test]
#[serial]
fn test_tsconfig_exclude_filters_svelte_diagnostics() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    // Create an excluded directory
    let excluded_dir = fixture_path.join("src/excluded");
    fs::create_dir_all(&excluded_dir).expect("Failed to create excluded dir");

    // Create a Svelte file in the excluded directory with a11y issues
    let excluded_svelte = excluded_dir.join("ExcludedComponent.svelte");
    let excluded_content = r#"<script lang="ts">
</script>

<!-- This has a11y issues but is in excluded directory -->
<div tabindex="0">Should not produce warnings if excluded</div>
<img src="test.png">
"#;
    fs::write(&excluded_svelte, excluded_content).expect("Failed to write excluded file");

    // Update tsconfig.json to exclude this directory
    let tsconfig_path = fixture_path.join("tsconfig.json");
    let original_tsconfig = fs::read_to_string(&tsconfig_path).expect("Failed to read tsconfig");

    let updated_tsconfig = r#"{
	"compilerOptions": {
		"target": "ES2022",
		"module": "ESNext",
		"moduleResolution": "bundler",
		"strict": true,
		"noEmit": true,
		"skipLibCheck": true,
		"esModuleInterop": true,
		"isolatedModules": true,
		"resolveJsonModule": true
	},
	"include": ["src/**/*.ts", "src/**/*.svelte"],
	"exclude": ["node_modules", "src/excluded"]
}
"#;
    fs::write(&tsconfig_path, updated_tsconfig).expect("Failed to write updated tsconfig");

    // Run svelte-check-rs
    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // There should be no diagnostics for files in excluded directory
    let excluded_diagnostics = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("ExcludedComponent.svelte") || d.filename.contains("excluded")
    });

    // Restore original tsconfig
    fs::write(&tsconfig_path, original_tsconfig).expect("Failed to restore tsconfig");

    assert_eq!(
        excluded_diagnostics,
        0,
        "ISSUE #19 REGRESSION: Files in tsconfig exclude patterns should not produce diagnostics. \
         Found diagnostics for excluded files: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("excluded"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&excluded_svelte);
    let _ = fs::remove_dir(&excluded_dir);
}

/// Test that files NOT in exclude patterns still produce diagnostics.
#[test]
#[serial]
fn test_non_excluded_files_still_checked() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    // Create a file in src (not excluded) with a11y issues
    let test_svelte = fixture_path.join("src/NotExcluded.svelte");
    let content = r#"<script lang="ts">
</script>

<!-- This should produce a warning since it's not excluded -->
<div tabindex="0">Should produce warning</div>
"#;
    fs::write(&test_svelte, content).expect("Failed to write test file");

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    let warnings = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("NotExcluded.svelte") && d.code == "a11y-no-noninteractive-tabindex"
    });

    assert!(
        warnings > 0,
        "Files not in exclude patterns should still be checked and produce diagnostics. \
         Found: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("NotExcluded"))
            .collect::<Vec<_>>()
    );

    let _ = fs::remove_file(&test_svelte);
}

/// Test that wildcard exclude patterns work correctly.
#[test]
#[serial]
fn test_tsconfig_exclude_wildcard_patterns() {
    let fixture_path = fixtures_dir().join("simple-app");

    // Clean cache
    let cache_path = fixture_path.join(".svelte-check-rs");
    let _ = fs::remove_dir_all(&cache_path);

    // Create test directories
    let test_dir = fixture_path.join("src/__tests__");
    fs::create_dir_all(&test_dir).expect("Failed to create test dir");

    let spec_dir = fixture_path.join("src/spec");
    fs::create_dir_all(&spec_dir).expect("Failed to create spec dir");

    // Create Svelte files with a11y issues in test directories
    let test_svelte = test_dir.join("TestComponent.svelte");
    fs::write(&test_svelte, r#"<div tabindex="0">Test file</div>"#)
        .expect("Failed to write test file");

    let spec_svelte = spec_dir.join("SpecComponent.svelte");
    fs::write(&spec_svelte, r#"<div tabindex="0">Spec file</div>"#)
        .expect("Failed to write spec file");

    // Update tsconfig.json with wildcard exclude patterns
    let tsconfig_path = fixture_path.join("tsconfig.json");
    let original_tsconfig = fs::read_to_string(&tsconfig_path).expect("Failed to read tsconfig");

    let updated_tsconfig = r#"{
	"compilerOptions": {
		"target": "ES2022",
		"module": "ESNext",
		"moduleResolution": "bundler",
		"strict": true,
		"noEmit": true,
		"skipLibCheck": true,
		"esModuleInterop": true,
		"isolatedModules": true,
		"resolveJsonModule": true
	},
	"include": ["src/**/*.ts", "src/**/*.svelte"],
	"exclude": ["node_modules", "**/__tests__/**", "**/spec/**"]
}
"#;
    fs::write(&tsconfig_path, updated_tsconfig).expect("Failed to write updated tsconfig");

    let (_exit_code, diagnostics) = run_check_json(&fixture_path, "svelte");

    // Restore original tsconfig
    fs::write(&tsconfig_path, original_tsconfig).expect("Failed to restore tsconfig");

    // There should be no diagnostics for files in __tests__ or spec directories
    let test_diagnostics = count_diagnostics_matching(&diagnostics, |d| {
        d.filename.contains("__tests__") || d.filename.contains("/spec/")
    });

    assert_eq!(
        test_diagnostics,
        0,
        "ISSUE #19: Wildcard exclude patterns should filter out test directories. \
         Found diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.contains("__tests__") || d.filename.contains("spec"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&test_svelte);
    let _ = fs::remove_file(&spec_svelte);
    let _ = fs::remove_dir(&test_dir);
    let _ = fs::remove_dir(&spec_dir);
}
