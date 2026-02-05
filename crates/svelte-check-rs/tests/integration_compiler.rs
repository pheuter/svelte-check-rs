//! Integration tests for Svelte compiler diagnostics via bun.
//!
//! These tests verify that:
//! - Compiler errors are reported with correct codes and locations
//!
//! Note: Tests are skipped on Windows due to bun/path handling differences.

#![cfg(not(target_os = "windows"))]

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("projects")
}

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

fn cache_root(fixture_path: &std::path::Path) -> PathBuf {
    fixture_path
        .join("node_modules")
        .join(".cache")
        .join("svelte-check-rs")
}

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
struct JsonPosition {
    line: u32,
    column: u32,
}

static COMPILER_READY: OnceLock<()> = OnceLock::new();
static BIN_READY: OnceLock<()> = OnceLock::new();
static COMPILER_CACHE: OnceLock<Vec<JsonDiagnostic>> = OnceLock::new();

fn ensure_fixture_ready(fixture_path: &PathBuf) {
    COMPILER_READY.get_or_init(|| {
        let cache_path = cache_root(fixture_path);
        let _ = fs::remove_dir_all(&cache_path);

        let node_modules = fixture_path.join("node_modules");
        if !node_modules.exists() {
            eprintln!("Installing dependencies for compiler-errors...");

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
    });
}

fn ensure_binary_built() {
    BIN_READY.get_or_init(|| {
        let _ = Command::new("cargo")
            .args(["build", "-p", "svelte-check-rs"])
            .output();
    });
}

fn compiler_diagnostics(fixture_path: &PathBuf) -> Vec<JsonDiagnostic> {
    COMPILER_CACHE
        .get_or_init(|| {
            ensure_fixture_ready(fixture_path);
            ensure_binary_built();

            let output = Command::new(binary_path())
                .arg("--workspace")
                .arg(fixture_path)
                .arg("--output")
                .arg("json")
                .arg("--skip-tsgo")
                .output()
                .expect("Failed to run svelte-check-rs");

            assert!(
                !output.stdout.is_empty(),
                "Expected JSON output. Stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );

            serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
                panic!(
                    "Invalid JSON output: {e}\n{}",
                    String::from_utf8_lossy(&output.stdout)
                )
            })
        })
        .clone()
}

#[test]
fn test_component_invalid_directive_reported() {
    let fixture_path = fixtures_dir().join("compiler-errors");
    let diagnostics = compiler_diagnostics(&fixture_path);

    let matching = diagnostics.iter().filter(|d| {
        d.filename == "src/App.svelte"
            && d.code == "component_invalid_directive"
            && d.start.line == 6
            && d.start.column == 7
            && d.source == "svelte"
    });

    assert!(
        matching.count() >= 1,
        "Expected component_invalid_directive diagnostic in src/App.svelte. Got: {:#?}",
        diagnostics
    );
}

#[test]
fn test_const_tag_invalid_placement_reported() {
    let fixture_path = fixtures_dir().join("compiler-errors");
    let diagnostics = compiler_diagnostics(&fixture_path);

    let matching = diagnostics.iter().filter(|d| {
        d.filename == "src/ConstInvalid.svelte"
            && d.code == "const_tag_invalid_placement"
            && d.start.line == 3
            && d.start.column == 5
            && d.source == "svelte"
    });

    assert!(
        matching.count() >= 1,
        "Expected const_tag_invalid_placement diagnostic in src/ConstInvalid.svelte. Got: {:#?}",
        diagnostics
    );
}
