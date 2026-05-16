//! Integration tests for `svelte.config.js#extensions` handling (issue #126).
//!
//! Each test builds a self-contained project on disk under `target/test-tmp/`,
//! runs the CLI against it with `--list-files --skip-tsgo`, and asserts on
//! stdout/stderr. `--list-files` exits before tsgo or bun are invoked, so
//! these tests don't require `bun install` or `tsgo` to be present.

#![cfg(not(target_os = "windows"))]

use std::fs;
use std::path::{Path, PathBuf};
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

fn binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_svelte-check-rs") {
        return PathBuf::from(path);
    }
    if let Some(path) = option_env!("CARGO_BIN_EXE_svelte-check-rs") {
        return PathBuf::from(path);
    }
    workspace_root()
        .join("target")
        .join("debug")
        .join("svelte-check-rs")
}

static BIN_READY: OnceLock<()> = OnceLock::new();

fn ensure_binary_built() {
    BIN_READY.get_or_init(|| {
        let _ = Command::new("cargo")
            .args(["build", "-p", "svelte-check-rs"])
            .output();
    });
}

fn make_project(name: &str) -> PathBuf {
    let dir = workspace_root()
        .join("target")
        .join("test-tmp")
        .join("integration_extensions")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("clear previous test dir");
    }
    fs::create_dir_all(dir.join("src")).expect("create project dir");
    dir
}

fn write(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap_or_else(|e| panic!("write {}: {}", path.display(), e));
}

struct RunOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn run_list_files(project: &Path) -> RunOutput {
    ensure_binary_built();
    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(project)
        .arg("--skip-tsgo")
        .arg("--list-files")
        .output()
        .expect("Failed to execute svelte-check-rs");
    RunOutput {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn write_minimal_tsconfig(project: &Path) {
    write(
        &project.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*"]
}
"#,
    );
}

/// Issue #126: when svelte.config.js registers a non-default extension like
/// `.svx` (mdsvex), the checker must warn-and-skip those files instead of
/// silently breaking the rest of the pipeline.
#[test]
fn test_unregistered_extension_warns_and_skips() {
    let project = make_project("warn_and_skip_svx");
    write(
        &project.join("svelte.config.js"),
        "export default { extensions: ['.svelte', '.svx'] };\n",
    );
    write_minimal_tsconfig(&project);
    write(
        &project.join("src/Hello.svelte"),
        "<script lang=\"ts\">let name: string = 'world';</script>\n<p>Hello {name}</p>\n",
    );
    write(&project.join("src/post.svx"), "# A post\n");
    write(&project.join("src/another.svx"), "# Another post\n");

    let RunOutput {
        exit_code,
        stdout,
        stderr,
    } = run_list_files(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {}", stderr);
    assert!(
        stderr.contains("warning: 2 files with unregistered extension (.svx) skipped"),
        "expected warning about skipped .svx files, got stderr:\n{}",
        stderr
    );
    assert!(
        stdout.contains("src/Hello.svelte"),
        "expected .svelte file to be listed, got stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(".svx"),
        ".svx files should NOT appear in the file list, got stdout:\n{}",
        stdout
    );
    assert!(
        stderr.contains("Files to check (1)"),
        "expected exactly 1 file to be listed, got stderr:\n{}",
        stderr
    );
}

/// Defaults like `.svelte.ts`/`.svelte.js` must still be discovered even when
/// the user declares custom extensions (`extensions` should merge with the
/// natives, not replace them).
#[test]
fn test_user_extensions_merge_with_native_defaults() {
    let project = make_project("merge_defaults");
    write(
        &project.join("svelte.config.js"),
        // User declares only .svelte + .svx, omitting .svelte.ts/.svelte.js.
        // We should still pick up the .svelte.ts module file.
        "export default { extensions: ['.svelte', '.svx'] };\n",
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");
    write(
        &project.join("src/state.svelte.ts"),
        "export const count = $state(0);\n",
    );

    let RunOutput {
        exit_code,
        stdout,
        stderr,
    } = run_list_files(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {}", stderr);
    assert!(
        stdout.contains("src/App.svelte"),
        "expected .svelte file to be listed, got stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("src/state.svelte.ts"),
        ".svelte.ts file should still be discovered when extensions are customized, got stdout:\n{}",
        stdout
    );
}

/// Sanity check: a project with no custom extensions should not emit any
/// "unregistered extension" warning.
#[test]
fn test_no_warning_when_only_native_extensions_present() {
    let project = make_project("no_custom_extensions");
    write(
        &project.join("svelte.config.js"),
        "export default { extensions: ['.svelte'] };\n",
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_list_files(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {}", stderr);
    assert!(
        !stderr.contains("unregistered extension"),
        "no warning expected for native-only project, got stderr:\n{}",
        stderr
    );
}
