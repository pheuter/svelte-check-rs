//! Integration tests for configured Svelte preprocessors.
//!
//! The fixture's markup preprocessor changes both script and template source.
//! The processed source must feed compiler diagnostics and TypeScript checking.

use bun_runner::{BunCompileOptions, BunInput, BunPreprocessPhase, BunRunner};
use camino::Utf8PathBuf;
use serde::Deserialize;
use serial_test::serial;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("svelte-check-rs");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
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
    assert_eq!(internal_diagnostic.start.line, 8);
    assert_eq!(internal_diagnostic.start.column, 16);

    let compiler_diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.contains("missing_attribute"))
        .unwrap_or_else(|| panic!("expected mapped compiler diagnostic: {diagnostics:#?}"));
    assert_eq!(compiler_diagnostic.start.line, 10);
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
        5,
        "one error per component expected"
    );
    assert!(
        preprocess_errors.iter().all(|diagnostic| diagnostic
            .filename
            .replace('\\', "/")
            .ends_with("svelte.config.js")),
        "initial config errors should retain the config filename: {diagnostics:#?}"
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

fn run_single_fixture(file: &str, envs: &[(&str, &str)]) -> std::process::Output {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg(file)
        .arg("--output")
        .arg("json");
    for (name, value) in envs {
        command.env(name, value);
    }
    command.output().expect("execute svelte-check-rs")
}

fn json_diagnostics(output: &std::process::Output) -> Vec<JsonDiagnostic> {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "Invalid JSON output: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

struct WatchProcess {
    child: std::process::Child,
    lines: mpsc::Receiver<String>,
}

impl WatchProcess {
    fn spawn(command: &mut Command) -> Self {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start watch mode");
        let stream = child.stdout.take().expect("watch stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        Self { child, lines }
    }

    fn wait_for(&self, needle: &str) -> Result<Vec<String>, Vec<String>> {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match self.lines.recv_timeout(Duration::from_millis(250)) {
                Ok(line) => {
                    let matched = line.contains(needle);
                    seen.push(line);
                    if matched {
                        return Ok(seen);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        Err(seen)
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct RestoreFile {
    path: PathBuf,
    contents: Vec<u8>,
}

impl Drop for RestoreFile {
    fn drop(&mut self) {
        let _ = fs::write(&self.path, &self.contents);
    }
}

#[test]
#[serial]
fn vite_inline_preprocessor_is_effective_and_stdout_is_isolated() {
    let output = run_single_fixture(
        "src/ViteOnly.svelte",
        &[("SVELTE_CHECK_RS_TEST_VITE_INLINE", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    assert!(output.status.success(), "{diagnostics:#?}");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("vite config stdout"), "{stderr}");
    assert!(stderr.contains("vite preprocessor stdout"), "{stderr}");
    assert_eq!(
        stderr.matches("vite config stdout").count(),
        1,
        "effective config should execute once per run: {stderr}"
    );
}

#[test]
#[serial]
fn protocol_ignores_partial_multiline_and_json_looking_stdout() {
    let output = run_single_fixture(
        "src/App.svelte",
        &[("SVELTE_CHECK_RS_TEST_PROTOCOL_OUTPUT", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    assert!(output.status.success(), "{diagnostics:#?}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in [
        r#"{"id":1,"config":{"found":false}}"#,
        "vite config multiline first",
        "vite config multiline second",
        "vite config partial write",
        r#"{"id":1,"code":"not a protocol frame"}"#,
        "preprocessor multiline first",
        "preprocessor multiline second",
        "preprocessor partial write",
    ] {
        assert!(stderr.contains(expected), "missing {expected:?}: {stderr}");
    }
}

#[test]
#[serial]
fn captured_protocol_writer_survives_stdout_monkey_patch() {
    let output = run_single_fixture(
        "src/App.svelte",
        &[("SVELTE_CHECK_RS_TEST_PATCH_STDOUT", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    assert!(output.status.success(), "{diagnostics:#?}");
}

#[test]
#[serial]
fn vite_config_file_wins_over_conventional_svelte_config() {
    let output = run_single_fixture(
        "src/ViteOnly.svelte",
        &[("SVELTE_CHECK_RS_TEST_VITE_CONFIG_FILE", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    assert!(output.status.success(), "{diagnostics:#?}");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
#[serial]
fn effective_config_without_preprocess_skips_preprocessing_workers() {
    let output = run_single_fixture(
        "src/NoPreprocess.svelte",
        &[("SVELTE_CHECK_RS_TEST_VITE_NO_PREPROCESS", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    assert!(output.status.success(), "{diagnostics:#?}");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("vite config stdout").count(), 1, "{stderr}");
}

#[test]
#[serial]
fn changed_mapless_output_becomes_a_clear_diagnostic() {
    let output = run_single_fixture(
        "src/App.svelte",
        &[("SVELTE_CHECK_RS_TEST_MAPLESS_CHANGE", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    assert!(!output.status.success());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "preprocess-error"
            && diagnostic
                .message
                .contains("without returning a source map")
    }));
}

#[test]
#[serial]
fn script_error_positions_are_fragment_relative() {
    let output = run_single_fixture(
        "src/FragmentError.svelte",
        &[("SVELTE_CHECK_RS_TEST_SCRIPT_ERROR", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("script fragment failure"))
        .expect("script preprocess diagnostic");
    assert_eq!(
        diagnostic.filename.replace('\\', "/"),
        "src/FragmentError.svelte"
    );
    assert_eq!(diagnostic.start.line, 4);
    assert_eq!(diagnostic.start.column, 2);
}

#[test]
#[serial]
fn external_style_errors_retain_the_referenced_file() {
    let output = run_single_fixture(
        "src/FragmentError.svelte",
        &[("SVELTE_CHECK_RS_TEST_EXTERNAL_STYLE_ERROR", "1")],
    );
    let diagnostics = json_diagnostics(&output);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("external Sass failure"))
        .expect("external style diagnostic");
    assert_eq!(diagnostic.filename.replace('\\', "/"), "src/_partial.scss");
    assert_eq!(diagnostic.start.line, 3);
    assert_eq!(diagnostic.start.column, 3);
}

#[test]
#[serial]
fn duplicate_script_fragments_do_not_report_a_guessed_position() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let temp = tempfile::tempdir().expect("temp config directory");
    let config = temp.path().join("duplicate-fragment.config.mjs");
    fs::write(
        &config,
        r#"export default { preprocess: { script() { const error = new Error('ambiguous fragment'); error.location = { start: { line: 1, column: 2 }, end: { line: 1, column: 3 } }; throw error; } } };"#,
    )
    .expect("write config");
    let config = Utf8PathBuf::from_path_buf(config).expect("utf-8 config path");
    let fragment = "\nconst identical = true;\n";
    let source =
        format!("<script>{fragment}</script>\n<script context=\"module\">{fragment}</script>");

    let processed = runtime
        .block_on(runner.preprocess_files(
            vec![BunInput {
                filename: Utf8PathBuf::from("Duplicate.svelte"),
                source,
                options: BunCompileOptions::default(),
            }],
            Some(&config),
        ))
        .expect("preprocess duplicate fragments");
    let error = processed[0].error.as_ref().expect("preprocess error");
    assert_eq!(error.phase, Some(BunPreprocessPhase::Script));
    assert_eq!(error.fragment_offset, None);
    assert!(error.start.is_none());
    assert!(error.end.is_none());
}

#[test]
#[serial]
fn script_fragment_offset_ignores_matching_attribute_text() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let temp = tempfile::tempdir().expect("temp config directory");
    let config = temp.path().join("attribute-fragment.config.mjs");
    fs::write(
        &config,
        r#"export default { preprocess: { script() { const error = new Error('attribute fragment'); error.location = { start: { line: 1, column: 0 } }; throw error; } } };"#,
    )
    .expect("write config");
    let config = Utf8PathBuf::from_path_buf(config).expect("utf-8 config path");
    let source = r#"<script lang="ts">ts</script>"#;

    let processed = runtime
        .block_on(runner.preprocess_files(
            vec![BunInput {
                filename: Utf8PathBuf::from("Attribute.svelte"),
                source: source.to_string(),
                options: BunCompileOptions::default(),
            }],
            Some(&config),
        ))
        .expect("preprocess fragment with matching attribute text");
    let error = processed[0].error.as_ref().expect("preprocess error");
    assert_eq!(error.phase, Some(BunPreprocessPhase::Script));
    assert_eq!(error.fragment_offset, Some(18));
}

#[test]
#[serial]
fn plain_file_url_objects_are_normalized_as_dependencies() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let temp = tempfile::tempdir().expect("temp config directory");
    let config = temp.path().join("plain-url.config.mjs");
    fs::write(
        &config,
        r#"export default { preprocess: { markup({ content }) { const url = new URL('./dependency.scss', import.meta.url); return { code: content, dependencies: [{ href: url.href }, { href: 'file:///bad%2Fslash' }] }; } } };"#,
    )
    .expect("write config");
    fs::write(temp.path().join("dependency.scss"), "").expect("write dependency");
    let config = Utf8PathBuf::from_path_buf(config).expect("utf-8 config path");

    let processed = runtime
        .block_on(runner.preprocess_files(
            vec![BunInput {
                filename: Utf8PathBuf::from("PlainUrl.svelte"),
                source: "<p>plain URL</p>".to_string(),
                options: BunCompileOptions::default(),
            }],
            Some(&config),
        ))
        .expect("preprocess plain file URL dependency");
    assert_eq!(processed[0].dependencies.len(), 2);
    let actual = processed[0].dependencies[0]
        .canonicalize_utf8()
        .expect("canonical actual dependency path");
    let expected = Utf8PathBuf::from_path_buf(temp.path().join("dependency.scss"))
        .expect("utf-8 dependency path")
        .canonicalize_utf8()
        .expect("canonical expected dependency path");
    assert_eq!(actual, expected);
    assert_eq!(
        processed[0].dependencies[1],
        Utf8PathBuf::from("file:///bad%2Fslash")
    );
}

#[test]
#[serial]
fn runtime_loads_every_supported_svelte_config_format() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let bun_path = bun_path_for(&project);
    let runner = BunRunner::new(bun_path, workspace, 1).expect("bun runner");
    let temp = tempfile::Builder::new()
        .prefix("svelte check ü ")
        .tempdir()
        .expect("temp config directory");

    for extension in ["js", "mjs", "ts", "mts", "cjs"] {
        let path = temp.path().join(format!("svelte.config.{extension}"));
        let source = if extension == "cjs" {
            "module.exports = { preprocess: { markup: ({ content }) => ({ code: content }) }, compilerOptions: { runes: true } };"
        } else if extension == "ts" || extension == "mts" {
            "const config: any = { preprocess: { markup: ({ content }: any) => ({ code: content }) }, compilerOptions: { runes: true } }; export default config;"
        } else {
            "export default { preprocess: { markup: ({ content }) => ({ code: content }) }, compilerOptions: { runes: true } };"
        };
        fs::write(&path, source).expect("write config fixture");
        let path = Utf8PathBuf::from_path_buf(path).expect("utf-8 config path");
        let loaded = runtime
            .block_on(runner.load_config_from(&path))
            .unwrap_or_else(|error| panic!("failed to load {extension}: {error}"));
        assert!(loaded.found, "{extension}");
        assert!(loaded.has_preprocess, "{extension}");
        assert_eq!(loaded.runes, Some(true), "{extension}");
    }
}

#[test]
#[serial]
fn runtime_loads_every_supported_vite_config_format() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let bun_path = bun_path_for(&project);
    let runner = BunRunner::new(bun_path, workspace, 1).expect("bun runner");
    let temp = tempfile::Builder::new()
        .prefix("vite formats ü ")
        .tempdir_in(&project)
        .expect("temp Vite config directory");

    for extension in ["js", "mjs", "ts", "cjs", "mts", "cts"] {
        let path = temp.path().join(format!("vite.config.{extension}"));
        let source = if extension == "cjs" || extension == "cts" {
            "const { svelte } = require('@sveltejs/vite-plugin-svelte'); module.exports = { plugins: [svelte({ configFile: false, compilerOptions: { runes: true } })] };"
        } else {
            "import { svelte } from '@sveltejs/vite-plugin-svelte'; export default { plugins: [svelte({ configFile: false, compilerOptions: { runes: true } })] };"
        };
        fs::write(&path, source).expect("write Vite config fixture");
        let path = Utf8PathBuf::from_path_buf(path).expect("utf-8 config path");
        let loaded = runtime
            .block_on(runner.load_config_from(&path))
            .unwrap_or_else(|error| panic!("failed to load Vite {extension}: {error}"));
        assert!(loaded.found, "{extension}");
        assert_eq!(loaded.config_source.as_deref(), Some("vite"), "{extension}");
        assert_eq!(loaded.runes, Some(true), "{extension}");
    }
}

#[test]
#[serial]
fn relative_nested_vite_config_path_is_resolved_from_the_workspace() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let temp = tempfile::tempdir_in(&project).expect("nested config directory");
    let config = temp.path().join("vite.config.ts");
    fs::write(
        &config,
        "import { svelte } from '@sveltejs/vite-plugin-svelte'; export default { plugins: [svelte({ configFile: false, compilerOptions: { runes: true } })] };",
    )
    .expect("write nested Vite config");
    let relative = config
        .strip_prefix(&project)
        .expect("workspace-relative path");
    let relative = Utf8PathBuf::from_path_buf(relative.to_owned()).expect("utf-8 relative path");

    let loaded = runtime
        .block_on(runner.load_config_from(&relative))
        .expect("load relative Vite config");
    assert_eq!(loaded.config_source.as_deref(), Some("vite"));
    assert_eq!(loaded.runes, Some(true));
}

#[test]
#[serial]
fn arbitrary_explicit_config_falls_back_to_svelte_loading() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let config = Utf8PathBuf::from_path_buf(project.join("custom.svelte.config.js"))
        .expect("utf-8 config path");

    let loaded = runtime
        .block_on(runner.load_config_from(&config))
        .expect("load arbitrary config file");
    assert!(loaded.found);
    assert!(loaded.has_preprocess);
    assert_eq!(loaded.config_source.as_deref(), Some("svelte"));
}

#[test]
#[serial]
fn large_preprocess_requests_and_responses_do_not_deadlock() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let temp = tempfile::tempdir().expect("temp config directory");
    let config = temp.path().join("large-output.config.mjs");
    fs::write(
        &config,
        r#"export default { preprocess: { markup({ content }) { process.stdout.write('L'.repeat(131072)); return { code: content }; } } };"#,
    )
    .expect("write config");
    let config = Utf8PathBuf::from_path_buf(config).expect("utf-8 config path");
    let source = format!(
        "<script>const payload = {:?};</script>",
        "x".repeat(1_048_576)
    );
    let inputs: Vec<_> = (0..4)
        .map(|index| BunInput {
            filename: Utf8PathBuf::from(format!("Large{index}.svelte")),
            source: source.clone(),
            options: BunCompileOptions::default(),
        })
        .collect();

    let processed = runtime
        .block_on(async {
            tokio::time::timeout(
                Duration::from_secs(20),
                runner.preprocess_files(inputs, Some(&config)),
            )
            .await
        })
        .expect("preprocessing timed out")
        .expect("preprocessing failed");
    assert_eq!(processed.len(), 4);
    assert!(processed.iter().all(|result| result.source == source));
}

#[test]
#[serial]
fn effective_config_tracks_imported_config_modules() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let workspace = Utf8PathBuf::from_path_buf(project.clone()).expect("utf-8 fixture path");
    let runner = BunRunner::new(bun_path_for(&project), workspace, 1).expect("bun runner");
    let loaded = runtime.block_on(runner.load_config()).expect("load config");
    let expected =
        Utf8PathBuf::from_path_buf(project.join("config-dependency.js")).expect("utf-8 dependency");
    assert!(
        loaded.dependencies.contains(&expected),
        "{:?}",
        loaded.dependencies
    );
}

#[test]
#[serial]
fn watch_mode_rechecks_external_preprocessor_dependencies() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let temp = tempfile::tempdir().expect("external dependency directory");
    let dependency = temp.path().join("external-heading.txt");
    fs::write(&dependency, "first\n").expect("write dependency");

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/NoPreprocess.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput")
        .env("SVELTE_CHECK_RS_TEST_EXTERNAL_DEPENDENCY", &dependency);
    let watch = WatchProcess::spawn(&mut command);

    let ready = watch.wait_for("found 0 errors and 0 warnings");
    let replace_dependency = |contents: &str| {
        let replacement = dependency.with_extension("replacement");
        fs::write(&replacement, contents).expect("write replacement");
        if cfg!(windows) {
            let _ = fs::remove_file(&dependency);
        }
        fs::rename(&replacement, &dependency).expect("replace dependency");
    };

    let first_recheck = if ready.is_ok() {
        std::thread::sleep(Duration::from_secs(1));
        replace_dependency("second\n");
        watch.wait_for("File changed, re-checking")
    } else {
        Err(Vec::new())
    };
    let first_complete = watch.wait_for("found 0 errors and 0 warnings");
    let second_recheck = if first_complete.is_ok() {
        std::thread::sleep(Duration::from_millis(500));
        replace_dependency("third\n");
        watch.wait_for("File changed, re-checking")
    } else {
        Err(Vec::new())
    };
    assert!(ready.is_ok(), "watch mode never became ready: {ready:?}");
    assert!(
        first_recheck.is_ok(),
        "atomic dependency replacement did not trigger a recheck: {first_recheck:?}"
    );
    assert!(
        second_recheck.is_ok(),
        "watch was stale after the first replacement: {second_recheck:?}"
    );
}

#[test]
#[serial]
fn watch_mode_rechecks_nested_workspace_dependencies() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let dependency = project.join("src/_partial.scss");
    let original = fs::read(&dependency).expect("read nested dependency");
    let _restore = RestoreFile {
        path: dependency.clone(),
        contents: original.clone(),
    };

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/NoPreprocess.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput")
        .env("SVELTE_CHECK_RS_TEST_EXTERNAL_DEPENDENCY", &dependency);
    let watch = WatchProcess::spawn(&mut command);
    let ready = watch.wait_for("found 0 errors and 0 warnings");
    let watching = watch.wait_for("Watching for changes");
    if ready.is_ok() && watching.is_ok() {
        let mut changed = original;
        changed.extend_from_slice(b"\n/* watch change */\n");
        fs::write(&dependency, changed).expect("update nested dependency");
    }
    let rechecked = watch.wait_for("File changed, re-checking");

    assert!(ready.is_ok(), "watch mode never became ready: {ready:?}");
    assert!(
        rechecked.is_ok(),
        "nested workspace dependency did not trigger a recheck: {rechecked:?}"
    );
}

#[test]
#[serial]
fn watch_mode_tracks_referenced_file_from_initial_preprocess_error() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let dependency = project.join("src/_partial.scss");
    let original = fs::read(&dependency).expect("read referenced Sass file");
    let _restore = RestoreFile {
        path: dependency.clone(),
        contents: original.clone(),
    };

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/FragmentError.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput")
        .env("SVELTE_CHECK_RS_TEST_EXTERNAL_STYLE_ERROR", "1");
    let watch = WatchProcess::spawn(&mut command);
    let failed = watch.wait_for("external Sass failure");
    let watching = watch.wait_for("Watching for changes");
    if failed.is_ok() && watching.is_ok() {
        let mut changed = original;
        changed.extend_from_slice(b"\n/* repair attempt */\n");
        fs::write(&dependency, changed).expect("update referenced Sass file");
    }
    let rechecked = watch.wait_for("File changed, re-checking");

    assert!(
        failed.is_ok(),
        "initial preprocess error was not reported: {failed:?}"
    );
    assert!(
        rechecked.is_ok(),
        "referenced error file did not trigger a recheck: {rechecked:?}"
    );
}

#[test]
#[serial]
fn watch_mode_reloads_when_config_precedence_changes() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let vite_override = project.join("vite.config.js");
    assert!(!vite_override.exists(), "test override already exists");
    let _cleanup = RemoveFile(vite_override.clone());

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/ViteOnly.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput");
    let watch = WatchProcess::spawn(&mut command);
    let ready = watch.wait_for("found 0 errors and 0 warnings");
    let watching = watch.wait_for("Watching for changes");

    if ready.is_ok() && watching.is_ok() {
        let temporary = project.join("vite.config.js.replacement");
        fs::write(
            &temporary,
            r#"import { svelte } from '@sveltejs/vite-plugin-svelte';
export default { plugins: [svelte({ configFile: false, preprocess: { markup() { throw new Error('dynamic Vite precedence'); } } })] };"#,
        )
        .expect("write Vite override");
        fs::rename(temporary, &vite_override).expect("install Vite override");
    }
    let vite_won = watch.wait_for("dynamic Vite precedence");

    if vite_won.is_ok() {
        fs::remove_file(&vite_override).expect("remove Vite override");
    }
    let svelte_restored = watch.wait_for("found 0 errors and 0 warnings");

    assert!(ready.is_ok(), "watch mode never became ready: {ready:?}");
    assert!(
        vite_won.is_ok(),
        "new higher-precedence Vite config was not loaded: {vite_won:?}"
    );
    assert!(
        svelte_restored.is_ok(),
        "removing Vite config did not restore Svelte fallback: {svelte_restored:?}"
    );
}

#[test]
#[serial]
fn watch_mode_recovers_after_imported_config_module_is_restored() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let dependency = project.join("config-dependency.js");
    let backup = project.join("config-dependency.js.watch-backup");
    let _restore = RestoreFile {
        path: dependency.clone(),
        contents: fs::read(&dependency).expect("read config dependency"),
    };
    let _remove_backup = RemoveFile(backup.clone());

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/NoPreprocess.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput");
    let watch = WatchProcess::spawn(&mut command);
    let ready = watch.wait_for("found 0 errors and 0 warnings");
    let watching = watch.wait_for("Watching for changes");

    if ready.is_ok() && watching.is_ok() {
        fs::rename(&dependency, &backup).expect("remove imported config module");
    }
    let broken = watch.wait_for("config-dependency.js");
    if backup.exists() {
        fs::rename(&backup, &dependency).expect("restore imported config module");
    }
    let restored = watch.wait_for("found 0 errors and 0 warnings");

    assert!(ready.is_ok(), "watch mode never became ready: {ready:?}");
    assert!(
        broken.is_ok(),
        "removing an imported config module did not surface an error: {broken:?}"
    );
    assert!(
        restored.is_ok(),
        "restoring an imported config module did not reload config: {restored:?}"
    );
}

#[test]
#[serial]
fn watch_mode_recovers_when_imported_config_module_is_initially_missing() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let dependency = project.join("config-dependency.js");
    let original = fs::read(&dependency).expect("read config dependency");
    let _restore = RestoreFile {
        path: dependency.clone(),
        contents: original.clone(),
    };
    fs::remove_file(&dependency).expect("remove config dependency before startup");

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/NoPreprocess.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput");
    let watch = WatchProcess::spawn(&mut command);
    let broken = watch.wait_for("config-dependency.js");
    let watching = watch.wait_for("Watching for changes");
    if broken.is_ok() && watching.is_ok() {
        fs::write(&dependency, &original).expect("restore imported config module");
    }
    let restored = watch.wait_for("found 0 errors and 0 warnings");

    assert!(
        broken.is_ok(),
        "initial config import failure was not reported: {broken:?}"
    );
    assert!(
        restored.is_ok(),
        "restoring the initially missing config module did not reload config: {restored:?}"
    );
}

#[test]
#[serial]
fn watch_mode_recovers_when_vite_import_is_initially_missing() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let config = project.join("vite.config.ts");
    let original_config = fs::read(&config).expect("read Vite config");
    let _restore_config = RestoreFile {
        path: config.clone(),
        contents: original_config.clone(),
    };
    let dependency = project.join("vite-config-dependency.js");
    assert!(!dependency.exists(), "test dependency already exists");
    let _remove_dependency = RemoveFile(dependency.clone());

    let mut broken_config = b"import './vite-config-dependency.js';\n".to_vec();
    broken_config.extend_from_slice(&original_config);
    fs::write(&config, broken_config).expect("add missing Vite config import");

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/ViteOnly.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput")
        .env("SVELTE_CHECK_RS_TEST_VITE_INLINE", "1");
    let watch = WatchProcess::spawn(&mut command);
    let fell_back = watch.wait_for("the conventional Svelte config must not win over Vite");
    let watching = watch.wait_for("Watching for changes");
    if fell_back.is_ok() && watching.is_ok() {
        fs::write(&dependency, "export const loaded = true;\n")
            .expect("restore missing Vite config dependency");
    }
    let restored = watch.wait_for("found 0 errors and 0 warnings");

    assert!(
        fell_back.is_ok(),
        "broken Vite config did not use the Svelte fallback: {fell_back:?}"
    );
    assert!(
        restored.is_ok(),
        "restoring the initially missing Vite import did not reload config: {restored:?}"
    );
}

#[test]
#[serial]
fn watch_mode_recreates_a_failed_preprocessor_worker() {
    let project = fixture_path();
    ensure_fixture_ready(&project);
    let temp = tempfile::tempdir().expect("worker control directory");
    let control = temp.path().join("worker-control.txt");
    fs::write(&control, "ok\n").expect("write worker control");

    let mut command = Command::new(binary_path());
    command
        .arg("--workspace")
        .arg(&project)
        .arg("--single-file")
        .arg("src/NoPreprocess.svelte")
        .arg("--watch")
        .arg("--skip-tsgo")
        .arg("--preserveWatchOutput")
        .env("SVELTE_CHECK_RS_TEST_WORKER_CONTROL", &control);
    let watch = WatchProcess::spawn(&mut command);
    let ready = watch.wait_for("found 0 errors and 0 warnings");
    let watching = watch.wait_for("Watching for changes");

    if ready.is_ok() && watching.is_ok() {
        fs::write(&control, "crash\n").expect("request worker crash");
    }
    let rechecking = watch.wait_for("File changed, re-checking");
    let recovered = watch.wait_for("found 0 errors and 0 warnings");

    assert!(ready.is_ok(), "watch mode never became ready: {ready:?}");
    assert!(
        rechecking.is_ok(),
        "worker control did not trigger a recheck: {rechecking:?}"
    );
    assert!(
        recovered.is_ok(),
        "watch mode did not recover from a dead Bun worker: {recovered:?}"
    );
}
