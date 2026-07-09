//! Integration tests for package-manager workspace Svelte imports.

use bun_runner::BunRunner;
use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture_root() -> PathBuf {
    workspace_root()
        .join("test-fixtures")
        .join("projects")
        .join("workspace-stubs")
}

fn app_root() -> PathBuf {
    fixture_root().join("app")
}

fn binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_svelte-check-rs") {
        return PathBuf::from(path);
    }
    workspace_root()
        .join("target")
        .join("debug")
        .join("svelte-check-rs")
}

static BIN_READY: OnceLock<()> = OnceLock::new();
static FIXTURE_READY: OnceLock<()> = OnceLock::new();
static BUN_PATH: OnceLock<Utf8PathBuf> = OnceLock::new();

fn ensure_binary_built() {
    BIN_READY.get_or_init(|| {
        let output = Command::new("cargo")
            .args(["build", "-p", "svelte-check-rs"])
            .output()
            .expect("cargo build");
        assert!(output.status.success(), "cargo build failed");
    });
}

fn bun_path_for(workspace: &Utf8Path) -> Utf8PathBuf {
    BUN_PATH
        .get_or_init(|| {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            runtime
                .block_on(BunRunner::ensure_bun(Some(workspace)))
                .expect("ensure bun")
        })
        .clone()
}

fn ensure_fixture_ready() {
    FIXTURE_READY.get_or_init(|| {
        let root = fixture_root();
        let tsgo = root.join("node_modules").join(".bin").join("tsgo");
        if !tsgo.exists() {
            let root_utf8 = Utf8PathBuf::from_path_buf(root.clone()).expect("utf-8 fixture root");
            let bun = bun_path_for(&root_utf8);
            let output = Command::new(bun.as_std_path())
                .arg("install")
                .current_dir(&root)
                .output()
                .expect("bun install");
            assert!(
                output.status.success(),
                "bun install failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    });
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonDiagnostic {
    filename: String,
    code: String,
    message: String,
}

#[test]
fn workspace_package_svelte_imports_resolve_via_declaration_stubs() {
    ensure_binary_built();
    ensure_fixture_ready();

    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(app_root())
        .arg("--tsconfig")
        .arg(app_root().join("tsconfig.json"))
        .arg("--output")
        .arg("json")
        .output()
        .expect("run svelte-check-rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("failed to parse JSON output: {err}\nstdout:\n{stdout}"));

    assert!(
        output.status.success(),
        "expected clean check, got diagnostics:\n{diagnostics:#?}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:#?}"
    );
}
