//! Integration tests for configured Svelte preprocessors.
//!
//! The fixture's markup preprocessor changes both script and template source.
//! The processed source must feed compiler diagnostics and TypeScript checking.

#![cfg(not(target_os = "windows"))]

use bun_runner::BunRunner;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("projects")
        .join("configured-preprocessor")
}

fn binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_svelte-check-rs") {
        return PathBuf::from(path);
    }
    if let Some(path) = option_env!("CARGO_BIN_EXE_svelte-check-rs") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("svelte-check-rs")
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonDiagnostic {
    #[serde(rename = "type")]
    diagnostic_type: String,
    filename: String,
    message: String,
    code: String,
    source: String,
    start: JsonPosition,
}

#[derive(Debug, Deserialize)]
struct JsonPosition {
    line: u32,
    column: u32,
}

static FIXTURE_READY: OnceLock<()> = OnceLock::new();
static BUN_PATH: OnceLock<Utf8PathBuf> = OnceLock::new();

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

fn ensure_fixture_ready(project: &PathBuf) {
    FIXTURE_READY.get_or_init(|| {
        let cache_path = project.join("node_modules/.cache/svelte-check-rs");
        let _ = fs::remove_dir_all(cache_path);

        if !project.join("node_modules").exists() {
            let output = Command::new(bun_path_for(project).as_std_path())
                .arg("install")
                .current_dir(project)
                .output()
                .expect("Failed to run bun install. Is bun installed?");

            if !output.status.success() {
                panic!(
                    "bun install failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    });
}

#[test]
#[serial]
fn configured_preprocessor_feeds_compiler_and_typescript_checks() {
    let project = fixture_path();
    ensure_fixture_ready(&project);

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(&project)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let diagnostics: Vec<JsonDiagnostic> =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "Invalid JSON output: {e}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });

    assert!(
        output.status.success(),
        "processed fixture should pass, diagnostics: {diagnostics:#?}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics.is_empty(),
        "processed fixture should have no diagnostics: {diagnostics:#?}"
    );
}

#[test]
#[serial]
fn configured_preprocessor_source_map_restores_original_diagnostic_position() {
    let project = fixture_path();
    ensure_fixture_ready(&project);

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(&project)
        .arg("--output")
        .arg("json")
        .env("SVELTE_CHECK_RS_TEST_MAPPED_DIAGNOSTIC", "1")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let diagnostics: Vec<JsonDiagnostic> =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "Invalid JSON output: {error}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });

    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TS2322")
        .unwrap_or_else(|| panic!("expected mapped TS2322 diagnostic: {diagnostics:#?}"));
    assert_eq!(
        diagnostic.start.line, 4,
        "diagnostic must map past prepended lines"
    );
    assert_eq!(diagnostic.start.column, 17);

    let internal_diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "a11y-structure")
        .unwrap_or_else(|| panic!("expected mapped internal diagnostic: {diagnostics:#?}"));
    assert_eq!(internal_diagnostic.start.line, 9);
    assert_eq!(internal_diagnostic.start.column, 16);

    let compiler_diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.contains("missing_attribute"))
        .unwrap_or_else(|| panic!("expected mapped compiler diagnostic: {diagnostics:#?}"));
    assert_eq!(compiler_diagnostic.start.line, 11);
    assert_eq!(compiler_diagnostic.start.column, 16);
}

#[test]
#[serial]
fn missing_config_export_is_reported_as_an_error() {
    let project = fixture_path();
    ensure_fixture_ready(&project);

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(&project)
        .arg("--output")
        .arg("json")
        .env("SVELTE_CHECK_RS_TEST_MISSING_CONFIG_EXPORT", "1")
        .output()
        .expect("Failed to execute svelte-check-rs");

    assert!(!output.status.success(), "missing config export must fail");
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("Invalid JSON output: {error}"));
    let preprocess_errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "preprocess-error")
        .collect();
    assert!(
        preprocess_errors.iter().all(|diagnostic| {
            diagnostic.code == "preprocess-error"
                && diagnostic.message.contains("Missing exports in the config")
        }),
        "unexpected diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        preprocess_errors.len(),
        2,
        "one error per component expected"
    );
    assert!(
        preprocess_errors
            .iter()
            .any(|diagnostic| diagnostic.filename.ends_with("App.svelte"))
            && preprocess_errors
                .iter()
                .any(|diagnostic| diagnostic.filename.ends_with("Child.svelte")),
        "each affected component should receive an error: {diagnostics:#?}"
    );
}

#[test]
#[serial]
fn one_preprocessor_failure_does_not_hide_other_file_diagnostics() {
    let project = fixture_path();
    ensure_fixture_ready(&project);

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(&project)
        .arg("--output")
        .arg("json")
        .env("SVELTE_CHECK_RS_TEST_CONTINUE_AFTER_PREPROCESS_ERROR", "1")
        .output()
        .expect("Failed to execute svelte-check-rs");

    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("Invalid JSON output: {error}"));
    assert!(
        !output.status.success(),
        "fixture intentionally contains errors"
    );
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.filename.ends_with("App.svelte")
                && diagnostic.code == "preprocess-error"
                && diagnostic.message.contains("fixture preprocessor failure")
        }),
        "expected the per-file preprocess error: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.filename.ends_with("Child.svelte") && diagnostic.code == "TS2322"
        }),
        "the other component must still be type-checked: {diagnostics:#?}"
    );
}
