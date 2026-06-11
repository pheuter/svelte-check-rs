//! Integration tests for issue #2942: rewrite relative imports reaching outside
//! the workspace root.
//!
//! When transformed files are written to the generated cache folder, a relative
//! import that reaches OUTSIDE the workspace (e.g. `../../../../shared-external/value`)
//! cannot be resolved from the generated file's location via tsconfig `rootDirs`,
//! producing a spurious TS2307. The fix rewrites such specifiers so they resolve
//! from the generated cache location instead.
//!
//! These tests run against the `sveltekit-bundler` fixture (which already ships
//! installed dependencies + tsgo) using JSON output for precise verification:
//! - `issue-2942-external-import/+page.svelte` imports a sibling module that
//!   EXISTS outside the workspace -> must produce NO TS2307.
//! - `issue-2942-external-import/state.svelte.ts` is a module file importing the
//!   same external target -> must produce NO TS2307.
//! - `issue-2942-missing-external/+page.svelte` imports a genuinely-missing
//!   out-of-root module -> must STILL produce TS2307 (the fix must not mask
//!   real missing modules).
//!
//! The external target lives at `test-fixtures/projects/shared-external/value.ts`,
//! a sibling of the `sveltekit-bundler` workspace root.
//!
//! Skipped on Windows due to tsgo/path handling differences.

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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixtures_dir() -> PathBuf {
    workspace_root().join("test-fixtures").join("projects")
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

fn cache_root(fixture_path: &Path) -> PathBuf {
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
#[allow(dead_code)]
struct JsonPosition {
    line: u32,
    column: u32,
    offset: u32,
}

static BIN_READY: OnceLock<()> = OnceLock::new();
static BUNDLER_READY: OnceLock<()> = OnceLock::new();
static BUNDLER_CACHE: OnceLock<(i32, Vec<JsonDiagnostic>)> = OnceLock::new();
static BUNDLER_LOCK: Mutex<()> = Mutex::new(());
static BUN_PATH: OnceLock<Utf8PathBuf> = OnceLock::new();

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

fn ensure_fixture_ready(fixture_path: &PathBuf) {
    BUNDLER_READY.get_or_init(|| {
        // Clean cache to ensure fresh state.
        let cache_path = cache_root(fixture_path);
        let _ = fs::remove_dir_all(&cache_path);

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

        // Generate SvelteKit types.
        let bun_path = bun_path_for(fixture_path);
        let _ = Command::new(bun_path.as_std_path())
            .args(["x", "svelte-kit", "sync"])
            .current_dir(fixture_path)
            .output();
    });
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
    FileExt::lock_exclusive(&file).expect("lock fixture");
    file
}

fn with_bundler_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = BUNDLER_LOCK.lock().expect("lock sveltekit-bundler mutex");
    let _file_lock = lock_fixture("sveltekit-bundler");
    f()
}

fn run_check_json_uncached(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    ensure_fixture_ready(fixture_path);
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

    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON output: {}", e);
        eprintln!("Raw output:\n{}", stdout);
        vec![]
    });

    (exit_code, diagnostics)
}

fn run_check_json(fixture_path: &PathBuf) -> (i32, Vec<JsonDiagnostic>) {
    BUNDLER_CACHE
        .get_or_init(|| with_bundler_lock(|| run_check_json_uncached(fixture_path)))
        .clone()
}

fn assert_no_diagnostics_in_file(diagnostics: &[JsonDiagnostic], filename: &str) {
    let in_file: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.ends_with(filename))
        .collect();
    assert!(
        in_file.is_empty(),
        "Expected no diagnostics in {}, but found:\n{:#?}",
        filename,
        in_file
    );
}

/// A resolvable relative import reaching outside the workspace must NOT produce
/// a spurious TS2307: the specifier is rewritten so it resolves from the
/// generated cache location.
#[test]
fn test_external_import_resolves_no_ts2307() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);

    assert_no_diagnostics_in_file(&diagnostics, "issue-2942-external-import/+page.svelte");
}

/// A `.svelte.ts` module file importing the same external target must also
/// resolve without a TS2307.
#[test]
fn test_external_import_module_file_no_ts2307() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);

    assert_no_diagnostics_in_file(&diagnostics, "issue-2942-external-import/state.svelte.ts");
}

/// Negative companion: a genuinely-missing out-of-root import must STILL produce
/// TS2307. The rewrite is purely lexical, so a missing target is rewritten to an
/// unresolvable path and tsgo reports it — the fix never masks real missing
/// modules.
///
/// This runs in a throwaway project (so it doesn't pollute the shared
/// `sveltekit-bundler` "clean project" invariants) that reuses the bundler's
/// installed `node_modules`, tsconfig, and svelte.config so tsgo + SvelteKit
/// types are available.
#[test]
fn test_missing_external_import_still_ts2307() {
    let bundler = fixtures_dir().join("sveltekit-bundler");
    // Make sure the bundler's deps + SvelteKit types exist before we reuse them.
    let _ = run_check_json(&bundler);

    let project = make_temp_project_reusing_bundler(&bundler, "missing-external");

    // A route importing a NONEXISTENT module that reaches outside the workspace.
    let route_dir = project.join("src/routes/missing");
    fs::create_dir_all(&route_dir).expect("create route dir");
    write_file(
        &route_dir.join("+page.svelte"),
        "<script lang=\"ts\">\n\timport { nope } from '../../../../shared-external/does-not-exist';\n\tconst x = nope;\n</script>\n<p>{x}</p>\n",
    );

    let (_exit_code, diagnostics) = run_check_json_for_temp(&project);

    let found = diagnostics
        .iter()
        .any(|d| d.filename.ends_with("missing/+page.svelte") && d.code == "TS2307");
    assert!(
        found,
        "Expected TS2307 for the missing out-of-root import, but it was not present.\nAll diagnostics:\n{:#?}",
        diagnostics
    );
}

/// Issue #2942 refinement (side-effect imports): a BARE side-effect import
/// (an import declaration with no clause, e.g. `import '../../x';`) reaching
/// outside the workspace must also be rewritten. The specifier scanner
/// originally recognized only `from`, dynamic `import()`, and `require()`
/// contexts, so a side-effect import was left unrewritten and produced a
/// spurious TS2307. After the fix it resolves cleanly.
#[test]
fn test_side_effect_external_import_no_ts2307() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);

    let ts2307: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename
                .ends_with("issue-2942-side-effect-import/+page.svelte")
                && d.code == "TS2307"
        })
        .collect();
    assert!(
        ts2307.is_empty(),
        "Expected no TS2307 for the bare side-effect out-of-root import, but found:\n{:#?}",
        ts2307
    );
}

/// Issue #2942 refinement (source-map drift): a relative import reaching outside
/// the workspace grows when rewritten (+N bytes per added `../`), shifting every
/// generated byte offset after it. A REAL type error on a LATER line must still
/// map back to the EXACT line AND column in the `.svelte` source. Before the
/// drift fix the column was shifted by the number of bytes the import grew (here
/// `../../../../shared-external/value` -> 8 `../`, a +12-byte grow).
///
/// Uses the static `issue-2942-drift/+page.svelte` fixture in the shared bundler
/// (a temp project at a different depth could not resolve the out-of-root
/// target, so the import must live where the relative path is real). The fixture
/// is kept free of `.svelte` imports so the out-of-scope `.svelte` ->
/// `.svelte.js` rewrite cannot perturb the asserted column. Its expected TS2322
/// is registered in the bundler exact-error list (integration_tsconfig.rs).
#[test]
fn test_external_import_drift_maps_correct_column() {
    let fixture_path = fixtures_dir().join("sveltekit-bundler");
    let (_exit_code, diagnostics) = run_check_json(&fixture_path);

    let drift: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.ends_with("issue-2942-drift/+page.svelte") && d.code == "TS2322")
        .collect();
    assert_eq!(
        drift.len(),
        1,
        "Expected exactly one TS2322 for the drift fixture, got:\n{:#?}",
        diagnostics
    );
    let diag = drift[0];
    // The TS2322 token `bad` is on line 13 (`\tconst bad: string = sharedValue;`).
    assert_eq!(
        diag.start.line, 13,
        "TS2322 should map to line 13 (the `const bad` line), got {} (full: {:#?})",
        diag.start.line, diag
    );
    // Column 8: a leading tab (col 1) + `const ` (cols 2-7) puts `bad` at byte
    // offset 7 -> 1-indexed column 8. A wrong column here means the source map
    // drifted by the grown-import byte delta (+12).
    assert_eq!(
        diag.start.column, 8,
        "TS2322 should map to column 8 (the `bad` token after `\\tconst `); a wrong \
         column here means the source map drifted by the grown-import byte delta. Got {} \
         (full: {:#?})",
        diag.start.column, diag
    );
}

/// Builds a throwaway project under `target/test-tmp/` that reuses the bundler's
/// installed dependencies (symlinked `node_modules` + generated `.svelte-kit`)
/// and config, so tsgo can run without a fresh install.
fn make_temp_project_reusing_bundler(bundler: &Path, name: &str) -> PathBuf {
    let dir = workspace_root()
        .join("target")
        .join("test-tmp")
        .join("integration_external_imports")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("clear previous temp project");
    }
    fs::create_dir_all(dir.join("src")).expect("create temp project dir");

    // Symlink node_modules (so tsgo, svelte, kit are all resolvable).
    std::os::unix::fs::symlink(bundler.join("node_modules"), dir.join("node_modules"))
        .expect("symlink node_modules");
    // Symlink the generated .svelte-kit types directory.
    let kit = bundler.join(".svelte-kit");
    if kit.exists() {
        std::os::unix::fs::symlink(&kit, dir.join(".svelte-kit")).expect("symlink .svelte-kit");
    }

    for cfg in ["tsconfig.json", "svelte.config.js", "package.json"] {
        let src = bundler.join(cfg);
        if src.exists() {
            fs::copy(&src, dir.join(cfg)).unwrap_or_else(|e| panic!("copy {cfg}: {e}"));
        }
    }

    dir
}

fn write_file(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap_or_else(|e| panic!("write {}: {}", path.display(), e));
}

/// Runs svelte-check-rs against a throwaway temp project (uncached, locked on
/// its own name).
/// Regression (careswitch monorepo): invoking with a RELATIVE `./` workspace
/// left an un-normalized `/./` in the absolutized workspace root, while the
/// cache/generated path is clean. The #2942 import rewrite compares those paths,
/// so IN-workspace imports (and the injected helper-shim import) were judged
/// "outside" and mangled into broken paths -> a flood of false TS2307/TS2882 in
/// every file. The existing #2942 tests passed an ABSOLUTE `--workspace`, so
/// they never exercised this; this one drives the binary with `./<name>`.
#[test]
fn test_relative_workspace_does_not_mangle_in_workspace_imports() {
    let bundler = fixtures_dir().join("sveltekit-bundler");
    let _ = run_check_json(&bundler); // ensure deps + .svelte-kit exist

    let name = "relative-workspace";
    let project = make_temp_project_reusing_bundler(&bundler, name);
    // An IN-workspace sibling import: `../util` from `src/lib/components`.
    fs::create_dir_all(project.join("src/lib/components")).expect("mkdir components");
    write_file(
        &project.join("src/lib/util.ts"),
        "export const helper = 41;\n",
    );
    write_file(
        &project.join("src/lib/components/Comp.svelte"),
        "<script lang=\"ts\">\n\timport { helper } from '../util';\n\tconst n = helper + 1;\n</script>\n<p>{n}</p>\n",
    );

    ensure_binary_built();
    let _lock = lock_fixture("sveltekit-bundler");
    // Drive with a RELATIVE `./<name>` workspace from the temp parent dir, so the
    // absolutized workspace contains the `/./` segment that triggered the bug.
    let parent = project.parent().expect("temp parent");
    let output = Command::new(binary_path())
        .current_dir(parent)
        .arg("--workspace")
        .arg(format!("./{name}"))
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

    // No file may get a mangled helper-shim import.
    let shim: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "TS2882" || d.message.contains("__svelte_check_rs_helpers"))
        .collect();
    assert!(
        shim.is_empty(),
        "relative `./` workspace mangled the injected helper-shim import:\n{:#?}",
        shim
    );

    // The in-workspace `../util` import must resolve (no spurious TS2307).
    let ts2307: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.ends_with("components/Comp.svelte") && d.code == "TS2307")
        .collect();
    assert!(
        ts2307.is_empty(),
        "relative `./` workspace mangled an in-workspace sibling import:\n{:#?}",
        ts2307
    );
}

/// Regression (found while fixing #157): a workspace path that traverses a
/// SYMLINK (e.g. macOS `/tmp` -> `/private/tmp`) was used as-given, so the
/// generated tsconfig's `rootDirs` entries did not prefix-match the physical
/// paths tsgo resolves on disk. The rootDirs mapping between the cache mirror
/// and the real sources then failed, and every relative import from a
/// transformed file surfaced as a false TS2307. The workspace root must be
/// canonicalized so all derived paths are physical.
#[test]
fn test_symlinked_workspace_resolves_relative_imports() {
    let bundler = fixtures_dir().join("sveltekit-bundler");
    let _ = run_check_json(&bundler); // ensure deps + .svelte-kit exist

    let name = "symlinked-workspace";
    let project = make_temp_project_reusing_bundler(&bundler, name);
    // The wholesale `node_modules` symlink would put the cache's physical path
    // outside the workspace, hiding the bug. Replicate the real-world layout
    // instead: `node_modules` is a REAL directory inside the project (so the
    // cache lives physically under the symlinked workspace root), with each
    // dependency entry symlinked individually.
    fs::remove_file(project.join("node_modules")).expect("remove node_modules symlink");
    fs::create_dir_all(project.join("node_modules")).expect("mkdir node_modules");
    for entry in fs::read_dir(bundler.join("node_modules")).expect("read bundler node_modules") {
        let entry = entry.expect("node_modules entry");
        if entry.file_name() == ".cache" {
            continue;
        }
        std::os::unix::fs::symlink(
            entry.path(),
            project.join("node_modules").join(entry.file_name()),
        )
        .expect("symlink node_modules entry");
    }
    // Likewise `.svelte-kit` must be a real directory inside the project; the
    // check run regenerates it via `svelte-kit sync`.
    fs::remove_file(project.join(".svelte-kit")).expect("remove .svelte-kit symlink");
    // An in-workspace sibling import that only resolves if rootDirs maps the
    // cache mirror back onto the real source directory.
    fs::create_dir_all(project.join("src/lib/components")).expect("mkdir components");
    write_file(
        &project.join("src/lib/util.ts"),
        "export const helper = 41;\n",
    );
    write_file(
        &project.join("src/lib/components/Comp.svelte"),
        "<script lang=\"ts\">\n\timport { helper } from '../util';\n\tconst n = helper + 1;\n</script>\n<p>{n}</p>\n",
    );
    // Comp must be IMPORTED by another file: tsgo realpaths every
    // import-resolved module, so the imported cache-mirror Comp.svelte.ts is
    // tracked under its physical path, from which its own `../util` no longer
    // prefix-matches the symlink-based rootDirs. A file only reachable via the
    // tsconfig `files` list keeps its as-given path and would not reproduce.
    fs::create_dir_all(project.join("src/routes/symlink-comp")).expect("mkdir route");
    write_file(
        &project.join("src/routes/symlink-comp/+page.svelte"),
        "<script lang=\"ts\">\n\timport Comp from '$lib/components/Comp.svelte';\n</script>\n<Comp />\n",
    );

    // Drive the binary through a symlink to the project.
    let link = project
        .parent()
        .expect("temp parent")
        .join("symlinked-workspace-link");
    if link.exists() || fs::symlink_metadata(&link).is_ok() {
        fs::remove_file(&link).expect("remove stale symlink");
    }
    std::os::unix::fs::symlink(&project, &link).expect("symlink workspace");

    let (_exit_code, diagnostics) = run_check_json_for_temp(&link);

    let ts2307: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "TS2307" || d.code == "TS2882")
        .collect();
    assert!(
        ts2307.is_empty(),
        "symlinked workspace broke relative-import resolution:\n{:#?}",
        ts2307
    );
}

fn run_check_json_for_temp(project: &Path) -> (i32, Vec<JsonDiagnostic>) {
    ensure_binary_built();
    let _lock = lock_fixture("sveltekit-bundler");
    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(project)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute svelte-check-rs");
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let diagnostics: Vec<JsonDiagnostic> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON output: {}", e);
        eprintln!("Raw output:\n{}", stdout);
        vec![]
    });
    (exit_code, diagnostics)
}
