//! Integration tests for cache invalidation.
//!
//! These tests verify that svelte-check-rs correctly detects when source files
//! have been modified and invalidates stale cache entries.
//!
//! The tests simulate real-world scenarios where:
//! - TypeScript types are added or modified after initial cache population
//! - Source files are updated with new code
//! - The cache should be refreshed to reflect the current source state
//!
//! Note: These tests are skipped on Windows due to tsgo/path handling differences.

#![cfg(not(target_os = "windows"))]

use serde::Deserialize;
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

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

/// Runs svelte-check-rs on a fixture with JSON output
fn run_check_json(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    // Build if necessary
    let _ = Command::new("cargo")
        .args(["build", "-p", "svelte-check-rs"])
        .output();

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(fixture_path)
        .arg("--diagnostic-sources")
        .arg("js")
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

/// Count errors matching a predicate
fn count_errors_matching<F>(diagnostics: &[JsonDiagnostic], predicate: F) -> usize
where
    F: Fn(&JsonDiagnostic) -> bool,
{
    diagnostics
        .iter()
        .filter(|d| d.diagnostic_type == "Error" && predicate(d))
        .count()
}

/// Sleep briefly to ensure filesystem timestamps differ
fn sleep_for_timestamp_resolution() {
    // Most filesystems have at least 1-second resolution
    // Some (like HFS+) have only 1-second resolution
    thread::sleep(Duration::from_millis(1100));
}

// ============================================================================
// CACHE INVALIDATION TESTS
// ============================================================================

/// Test that legacy .svelte-check-rs cache is migrated to node_modules/.cache.
#[test]
#[serial]
fn test_cache_migration_from_legacy_path() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    let legacy_cache = fixture_path.join(".svelte-check-rs");
    let legacy_marker = legacy_cache.join("cache/legacy.txt");
    let _ = fs::create_dir_all(legacy_marker.parent().unwrap());
    fs::write(&legacy_marker, "legacy").expect("Failed to write legacy marker");

    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    let (_exit_code, _diagnostics) = run_check_json(&fixture_path);

    assert!(
        !legacy_cache.exists(),
        "Legacy cache directory should be removed during migration"
    );
    assert!(
        cache_path.exists(),
        "New cache directory should be created during migration"
    );
}

/// Test that modifying a TypeScript file invalidates the cache and new types are detected.
///
/// This test reproduces the exact bug found in careswitch-web where:
/// 1. A type definition was modified (adding 'tags' field to a Pick type)
/// 2. The cache had the old type definition
/// 3. svelte-check-rs reported false positive errors because it used stale types
#[test]
#[serial]
fn test_modified_typescript_types_are_detected() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    // Create a test TypeScript file with an initial type
    let test_file = fixture_path.join("src/lib/cache-test-types.ts");
    let initial_content = r#"// Test file for cache invalidation
export type TestUser = {
    id: string;
    name: string;
};

export function getUser(): TestUser {
    return { id: "1", name: "Test" };
}
"#;
    fs::write(&test_file, initial_content).expect("Failed to write test file");

    // Create a Svelte file that uses this type
    let test_svelte = fixture_path.join("src/lib/CacheTest.svelte");
    let svelte_content = r#"<script lang="ts">
    import type { TestUser } from './cache-test-types';

    let user: TestUser = { id: "1", name: "Test" };

    // This should NOT error initially, but SHOULD error after we add 'email' field
    console.log(user.id, user.name);
</script>

<p>{user.name}</p>
"#;
    fs::write(&test_svelte, svelte_content).expect("Failed to write Svelte file");

    // Run svelte-check-rs to populate cache
    let (_exit_code1, diagnostics1) = run_check_json(&fixture_path);

    // Verify no errors for our test files initially
    let cache_test_errors1 = count_errors_matching(&diagnostics1, |d| {
        d.filename.contains("cache-test-types") || d.filename.contains("CacheTest.svelte")
    });
    assert_eq!(
        cache_test_errors1,
        0,
        "Expected no errors in cache test files initially, but found: {:?}",
        diagnostics1
            .iter()
            .filter(|d| d.filename.contains("cache-test") || d.filename.contains("CacheTest"))
            .collect::<Vec<_>>()
    );

    // Wait to ensure timestamp differs
    sleep_for_timestamp_resolution();

    // Modify the TypeScript file to add a required field
    let modified_content = r#"// Test file for cache invalidation - MODIFIED
export type TestUser = {
    id: string;
    name: string;
    email: string;  // NEW REQUIRED FIELD
};

export function getUser(): TestUser {
    return { id: "1", name: "Test", email: "test@example.com" };
}
"#;
    fs::write(&test_file, modified_content).expect("Failed to write modified test file");

    // Run svelte-check-rs again - it should detect the stale cache and use new types
    let (_exit_code2, diagnostics2) = run_check_json(&fixture_path);

    // Now there SHOULD be an error because user object is missing 'email'
    let cache_test_errors2 = count_errors_matching(&diagnostics2, |d| {
        d.filename.contains("CacheTest.svelte") && d.message.contains("email")
    });

    assert!(
        cache_test_errors2 > 0,
        "CACHE INVALIDATION BUG: After modifying TestUser to require 'email' field, \
         svelte-check-rs should detect that the user object in CacheTest.svelte is missing 'email'. \
         This indicates the cache was not properly invalidated.\n\
         Diagnostics: {:?}",
        diagnostics2
            .iter()
            .filter(|d| d.filename.contains("CacheTest"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_file(&test_svelte);
}

/// Test that adding a new TypeScript file is detected.
#[test]
#[serial]
fn test_new_typescript_file_is_detected() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    // Run svelte-check-rs to populate cache (without the new file)
    let (_exit_code1, _diagnostics1) = run_check_json(&fixture_path);

    // Wait to ensure timestamp differs
    sleep_for_timestamp_resolution();

    // Create a new TypeScript file with an intentional type error
    let new_file = fixture_path.join("src/lib/new-file-with-error.ts");
    let content_with_error = r#"// New file with intentional type error
export function brokenFunction(): number {
    return "not a number";  // TYPE ERROR: string not assignable to number
}
"#;
    fs::write(&new_file, content_with_error).expect("Failed to write new file");

    // Run svelte-check-rs again
    let (_exit_code2, diagnostics2) = run_check_json(&fixture_path);

    // The new file's error should be detected
    let new_file_errors = count_errors_matching(&diagnostics2, |d| {
        d.filename.contains("new-file-with-error") && d.code == "TS2322"
    });

    assert!(
        new_file_errors > 0,
        "New TypeScript file with type error should be detected. \
         This verifies the cache doesn't prevent new files from being checked.\n\
         Diagnostics: {:?}",
        diagnostics2
    );

    // Cleanup
    let _ = fs::remove_file(&new_file);
}

/// Test that fixing a type error is detected (cache doesn't persist old errors).
#[test]
#[serial]
fn test_fixed_type_error_is_detected() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    // Create a TypeScript file with a type error
    let test_file = fixture_path.join("src/lib/fixable-error.ts");
    let broken_content = r#"// File with fixable type error
export function getValue(): number {
    return "wrong type";  // TYPE ERROR
}
"#;
    fs::write(&test_file, broken_content).expect("Failed to write broken file");

    // Run svelte-check-rs to populate cache with the error
    let (_exit_code1, diagnostics1) = run_check_json(&fixture_path);

    // Verify the error is detected
    let error_count1 = count_errors_matching(&diagnostics1, |d| {
        d.filename.contains("fixable-error") && d.code == "TS2322"
    });
    assert!(
        error_count1 > 0,
        "Initial type error should be detected: {:?}",
        diagnostics1
    );

    // Wait to ensure timestamp differs
    sleep_for_timestamp_resolution();

    // Fix the type error
    let fixed_content = r#"// File with FIXED type error
export function getValue(): number {
    return 42;  // FIXED: now returns number
}
"#;
    fs::write(&test_file, fixed_content).expect("Failed to write fixed file");

    // Run svelte-check-rs again
    let (_exit_code2, diagnostics2) = run_check_json(&fixture_path);

    // The error should no longer be present
    let error_count2 = count_errors_matching(&diagnostics2, |d| {
        d.filename.contains("fixable-error") && d.code == "TS2322"
    });

    assert_eq!(
        error_count2,
        0,
        "CACHE INVALIDATION BUG: After fixing the type error, it should no longer be reported. \
         This indicates the cache was not properly invalidated.\n\
         Still seeing errors: {:?}",
        diagnostics2
            .iter()
            .filter(|d| d.filename.contains("fixable-error"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&test_file);
}

/// Test that modifying an imported module propagates type changes.
///
/// This is the exact scenario from careswitch-web:
/// - A shared types file (like billing/util.ts) is modified
/// - TypeScript files importing from that file should see the updated types
#[test]
#[serial]
fn test_imported_module_changes_propagate() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    // Create a shared types module
    let types_file = fixture_path.join("src/lib/shared-types.ts");
    let initial_types = r#"// Shared types module
export type SharedData = {
    id: string;
    name: string;
};
"#;
    fs::write(&types_file, initial_types).expect("Failed to write types file");

    // Create a TypeScript file that imports and uses the shared types
    let consumer_file = fixture_path.join("src/lib/consumer.ts");
    let consumer_content = r#"// Consumer that imports shared types
import type { SharedData } from './shared-types';

export function createData(): SharedData {
    // Initially correct - has id and name
    return { id: "1", name: "Test" };
}
"#;
    fs::write(&consumer_file, consumer_content).expect("Failed to write consumer file");

    // Run svelte-check-rs to populate cache
    let (_exit_code1, diagnostics1) = run_check_json(&fixture_path);

    // Verify no errors for our test files initially
    let test_errors1 = count_errors_matching(&diagnostics1, |d| {
        d.filename.contains("shared-types") || d.filename.contains("consumer.ts")
    });
    assert_eq!(
        test_errors1,
        0,
        "Expected no errors initially: {:?}",
        diagnostics1
            .iter()
            .filter(|d| d.filename.contains("shared") || d.filename.contains("consumer"))
            .collect::<Vec<_>>()
    );

    // Wait to ensure timestamp differs
    sleep_for_timestamp_resolution();

    // Modify the shared types to add a required field
    let modified_types = r#"// Shared types module - MODIFIED
export type SharedData = {
    id: string;
    name: string;
    requiredField: boolean;  // NEW REQUIRED FIELD
};
"#;
    fs::write(&types_file, modified_types).expect("Failed to write modified types");

    // Run svelte-check-rs again
    let (_exit_code2, diagnostics2) = run_check_json(&fixture_path);

    // Now there should be an error because consumer.ts returns object missing 'requiredField'
    let missing_field_errors = count_errors_matching(&diagnostics2, |d| {
        d.filename.contains("consumer.ts")
            && (d.message.contains("requiredField") || d.code == "TS2741")
    });

    assert!(
        missing_field_errors > 0,
        "CACHE INVALIDATION BUG: After adding 'requiredField' to SharedData, \
         the consumer.ts should report error about missing 'requiredField'. \
         This is the exact bug that caused false positives in careswitch-web.\n\
         Diagnostics: {:?}",
        diagnostics2
            .iter()
            .filter(|d| d.filename.contains("consumer") || d.filename.contains("shared"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&types_file);
    let _ = fs::remove_file(&consumer_file);
}

/// Test that deleting a file removes it from cache.
#[test]
#[serial]
fn test_deleted_file_removed_from_cache() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    // Create a TypeScript file that triggers a tsgo patch
    let temp_file = fixture_path.join("src/lib/temp-file.ts");
    let content = r#"// Temporary file with Promise.all empty array branch
export async function loadValue(cond: boolean) {
    const [value] = await Promise.all([cond ? fetchValue() : []]);
    return value;
}

async function fetchValue() {
    return 42;
}
"#;
    fs::write(&temp_file, content).expect("Failed to write temp file");

    // Run svelte-check-rs to populate cache
    let (_exit_code1, _diagnostics1) = run_check_json(&fixture_path);

    // Verify the file is in the cache
    let cached_file = cache_path.join("src/lib/temp-file.ts");
    assert!(
        cached_file.exists(),
        "File should be in cache after first run"
    );

    // Delete the source file
    fs::remove_file(&temp_file).expect("Failed to delete temp file");

    // Run svelte-check-rs again
    let (_exit_code2, _diagnostics2) = run_check_json(&fixture_path);

    // Verify the file is removed from cache
    assert!(
        !cached_file.exists(),
        "CACHE CLEANUP BUG: Deleted source file should be removed from cache"
    );
}

/// Test cache behavior with rapidly modified files.
#[test]
#[serial]
fn test_rapid_modifications_detected() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    let test_file = fixture_path.join("src/lib/rapid-changes.ts");

    // Initial version - no error
    let v1 = r#"export function getValue(): number { return 1; }"#;
    fs::write(&test_file, v1).expect("Failed to write v1");
    let (_exit1, diag1) = run_check_json(&fixture_path);
    let errors1 = count_errors_matching(&diag1, |d| d.filename.contains("rapid-changes"));
    assert_eq!(errors1, 0, "v1 should have no errors");

    sleep_for_timestamp_resolution();

    // Version 2 - introduces error
    let v2 = r#"export function getValue(): number { return "string"; }"#;
    fs::write(&test_file, v2).expect("Failed to write v2");
    let (_exit2, diag2) = run_check_json(&fixture_path);
    let errors2 = count_errors_matching(&diag2, |d| d.filename.contains("rapid-changes"));
    assert!(errors2 > 0, "v2 should have type error");

    sleep_for_timestamp_resolution();

    // Version 3 - fixes error again
    let v3 = r#"export function getValue(): number { return 3; }"#;
    fs::write(&test_file, v3).expect("Failed to write v3");
    let (_exit3, diag3) = run_check_json(&fixture_path);
    let errors3 = count_errors_matching(&diag3, |d| d.filename.contains("rapid-changes"));
    assert_eq!(
        errors3,
        0,
        "v3 should have no errors after fix: {:?}",
        diag3
            .iter()
            .filter(|d| d.filename.contains("rapid-changes"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&test_file);
}

/// Test that cache correctly handles files that change size but not content hash.
/// (Edge case: whitespace-only changes)
#[test]
#[serial]
fn test_whitespace_changes_detected() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");

    // Clean cache to start fresh
    let cache_path = cache_root(&fixture_path);
    let _ = fs::remove_dir_all(&cache_path);

    // Ensure dependencies are installed
    let node_modules = fixture_path.join("node_modules");
    if !node_modules.exists() {
        let output = Command::new("bun")
            .arg("install")
            .current_dir(&fixture_path)
            .output()
            .expect("Failed to run bun install");
        assert!(output.status.success(), "bun install failed");
    }

    // Run svelte-kit sync
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(&fixture_path)
        .output();

    let test_file = fixture_path.join("src/lib/whitespace-test.ts");

    // Initial version with error (compact)
    let v1 = r#"export function f():number{return"s";}"#;
    fs::write(&test_file, v1).expect("Failed to write v1");
    let (_exit1, diag1) = run_check_json(&fixture_path);
    let errors1 = count_errors_matching(&diag1, |d| d.filename.contains("whitespace-test"));
    assert!(errors1 > 0, "v1 should have type error");

    sleep_for_timestamp_resolution();

    // Version 2 - same content with whitespace, still has error
    let v2 = r#"export function f(): number {
    return "s";
}"#;
    fs::write(&test_file, v2).expect("Failed to write v2");
    let (_exit2, diag2) = run_check_json(&fixture_path);
    let errors2 = count_errors_matching(&diag2, |d| d.filename.contains("whitespace-test"));
    assert!(errors2 > 0, "v2 should still have type error");

    sleep_for_timestamp_resolution();

    // Version 3 - fixed with different formatting
    let v3 = r#"export function f(): number {
    return 42;
}"#;
    fs::write(&test_file, v3).expect("Failed to write v3");
    let (_exit3, diag3) = run_check_json(&fixture_path);
    let errors3 = count_errors_matching(&diag3, |d| d.filename.contains("whitespace-test"));
    assert_eq!(
        errors3,
        0,
        "v3 should have no errors after fix: {:?}",
        diag3
            .iter()
            .filter(|d| d.filename.contains("whitespace-test"))
            .collect::<Vec<_>>()
    );

    // Cleanup
    let _ = fs::remove_file(&test_file);
}
