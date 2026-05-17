//! Integration test for issue #128: shorthand directive syntax flagged as
//! unused (TS6133).
//!
//! Regression for <https://github.com/pheuter/svelte-check-rs/issues/128>.
//!
//! The bug: when a component or element uses a shorthand directive — `bind:foo`,
//! `class:foo`, or `style:foo` (instead of the explicit `<dir>:foo={foo}`) —
//! the generated TypeScript never references `foo`, so TypeScript reports
//!   'foo' is declared but its value is never read. (ts(TS6133))
//! under `noUnusedLocals: true`.
//!
//! The fixture sets `noUnusedLocals: true` so the bug surfaces if regressed.
//!
//! Skipped on Windows in line with the other tsgo-backed integration tests.

#![cfg(not(target_os = "windows"))]

use bun_runner::BunRunner;
use camino::Utf8PathBuf;
use serde::Deserialize;
use std::path::{Path, PathBuf};
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

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct JsonDiagnostic {
    #[serde(rename = "type")]
    diagnostic_type: String,
    filename: String,
    message: String,
    code: String,
    source: String,
}

static BUN_PATH: OnceLock<Utf8PathBuf> = OnceLock::new();
static BIN_READY: OnceLock<()> = OnceLock::new();
static FIXTURE_READY: OnceLock<()> = OnceLock::new();
static DIAGNOSTICS_CACHE: OnceLock<Vec<JsonDiagnostic>> = OnceLock::new();

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

fn ensure_fixture_ready(fixture_path: &PathBuf) {
    FIXTURE_READY.get_or_init(|| {
        let node_modules = fixture_path.join("node_modules");
        if !node_modules.exists() {
            eprintln!("Installing dependencies for bind-shorthand fixture...");
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
    });
}

fn ensure_binary_built() {
    BIN_READY.get_or_init(|| {
        let _ = Command::new("cargo")
            .args(["build", "-p", "svelte-check-rs"])
            .output();
    });
}

fn diagnostics() -> Vec<JsonDiagnostic> {
    DIAGNOSTICS_CACHE
        .get_or_init(|| {
            let fixture_path = fixtures_dir().join("bind-shorthand");
            ensure_fixture_ready(&fixture_path);
            ensure_binary_built();

            let output = Command::new(binary_path())
                .arg("--workspace")
                .arg(&fixture_path)
                .arg("--output")
                .arg("json")
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

fn assert_no_ts6133_for_variable(diagnostics: &[JsonDiagnostic], filename: &str, variable: &str) {
    let needle = format!("'{}' is declared but its value is never read", variable);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.ends_with(filename) && d.code == "TS6133" && d.message.contains(&needle)
        })
        .collect();

    assert!(
        matching.is_empty(),
        "Did not expect TS6133 for `{}` in {}, but found:\n{:#?}",
        variable,
        filename,
        matching
    );
}

/// Component shorthand: `<ChildTable bind:selectedIds />` must not flag
/// `selectedIds` as unused.
#[test]
fn test_component_bind_shorthand_does_not_flag_unused() {
    let diagnostics = diagnostics();
    assert_no_ts6133_for_variable(&diagnostics, "src/Parent.svelte", "selectedIds");
}

/// Element shorthand: `<input bind:value />` must not flag `value` as unused.
#[test]
fn test_element_bind_shorthand_value_does_not_flag_unused() {
    let diagnostics = diagnostics();
    assert_no_ts6133_for_variable(&diagnostics, "src/InputForm.svelte", "value");
}

/// Element shorthand: `<input type="checkbox" bind:checked />` must not flag
/// `checked` as unused.
#[test]
fn test_element_bind_shorthand_checked_does_not_flag_unused() {
    let diagnostics = diagnostics();
    assert_no_ts6133_for_variable(&diagnostics, "src/InputForm.svelte", "checked");
}

/// Class shorthand: `<div class:active>` must not flag `active` as unused.
#[test]
fn test_class_shorthand_does_not_flag_unused() {
    let diagnostics = diagnostics();
    assert_no_ts6133_for_variable(&diagnostics, "src/ClassStyle.svelte", "active");
}

/// Style shorthand: `<div style:color>` must not flag `color` as unused.
#[test]
fn test_style_shorthand_does_not_flag_unused() {
    let diagnostics = diagnostics();
    assert_no_ts6133_for_variable(&diagnostics, "src/ClassStyle.svelte", "color");
}

/// Sanity check: a truly unused local in the fixture is still flagged, i.e.
/// `noUnusedLocals` is actually in effect and the other tests are not passing
/// vacuously.
#[test]
fn test_no_unused_locals_actually_enabled() {
    let diagnostics = diagnostics();
    let found = diagnostics.iter().any(|d| {
        d.filename.ends_with("src/UnusedProbe.svelte")
            && d.code == "TS6133"
            && d.message
                .contains("'unusedProbe' is declared but its value is never read")
    });

    assert!(
        found,
        "Expected TS6133 for `unusedProbe` in src/UnusedProbe.svelte to confirm noUnusedLocals is active, but no such diagnostic was emitted.\nAll diagnostics: {:#?}",
        diagnostics
    );
}
