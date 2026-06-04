//! Integration coverage for the diagnostic-parity fixes against upstream
//! `svelte-check` discovered while running against careswitch-web.  Each
//! test exercises one root cause that was previously silently swallowed
//! by svelte-check-rs's transformer and asserts the corresponding TypeScript
//! error now surfaces at the exact location upstream reports.
//!
//! Categories covered:
//!   1. **Param-matcher narrowing** — `(p: string) => p === 'a'` infers a
//!      type predicate (TS 5.5+).  The earlier transform forced `: boolean`
//!      on the matcher's return, killing the predicate and silently widening
//!      `RouteParams[X]` back to `string`.  Now narrowed-vs-mismatched
//!      assignments at the route consumer surface TS2322.
//!
//!   2. **HTTP method param-annotation respect** — when the user types
//!      `export const GET: RequestHandler` against `@sveltejs/kit`'s loose
//!      `RequestHandler`, an inner `params: RequestEvent` annotation used
//!      to silently override it and mask `params.X is string | undefined`
//!      errors.  Now the outer annotation wins and the TS2345 surfaces.
//!
//!   3. **`bind:value={expr}` write-direction type-checking** — the prop
//!      slot used to emit `value: undefined as any`, dropping the user's
//!      expression type.  Now the bound expression flows through, so an
//!      assignment-incompatible type (e.g. `string | null` → `string |
//!      undefined`) surfaces as TS2322.
//!
//!   7. **JS param-matcher: no TS-only `satisfies` operator** — the params
//!      transform appended a trailing `satisfies ParamMatcher` reference
//!      unconditionally, leaking TS-only syntax into checked `.js` files
//!      (false-positive TS8010/TS8037).  Now the `.js` path uses a JSDoc
//!      `@satisfies` cast, which preserves the inferred predicate (TS2322
//!      still surfaces at the consumer) while staying valid JavaScript.
//!
//! These tests reuse the existing `sveltekit-bundler` fixture so they share
//! its bun install and `svelte-kit sync`.  They live in their own file (not
//! `integration_issues.rs`) so a future shake-up of the issues file doesn't
//! drag them along.
//!
//! Skipped on Windows in line with the other tsgo-backed integration tests.

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

// ----- shared test infra (mirrors integration_issues.rs but isolated here) -----

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
static DIAGS: OnceLock<Vec<JsonDiagnostic>> = OnceLock::new();
static BUNDLER_LOCK: Mutex<()> = Mutex::new(());
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

fn ensure_fixture_ready(fixture_path: &PathBuf) {
    BUNDLER_READY.get_or_init(|| {
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

        let bun_path = bun_path_for(fixture_path);
        let _ = Command::new(bun_path.as_std_path())
            .args(["x", "svelte-kit", "sync"])
            .current_dir(fixture_path)
            .output();
    });
}

fn ensure_binary_built() {
    BIN_READY.get_or_init(|| {
        let _ = Command::new("cargo")
            .args(["build", "-p", "svelte-check-rs"])
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
    file.lock_exclusive().expect("lock fixture");
    file
}

fn diagnostics() -> Vec<JsonDiagnostic> {
    DIAGS
        .get_or_init(|| {
            let _guard = BUNDLER_LOCK.lock().expect("lock sveltekit-bundler mutex");
            let _file_lock = lock_fixture("sveltekit-bundler");

            let fixture_path = fixtures_dir().join("sveltekit-bundler");
            ensure_fixture_ready(&fixture_path);
            ensure_binary_built();

            let output = Command::new(binary_path())
                .arg("--workspace")
                .arg(&fixture_path)
                .arg("--output")
                .arg("json")
                .output()
                .expect("Failed to execute svelte-check-rs");

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            serde_json::from_str(&stdout).unwrap_or_else(|e| {
                panic!("Invalid JSON output: {e}\nRaw output:\n{stdout}");
            })
        })
        .clone()
}

fn find_diagnostic<'a>(
    diagnostics: &'a [JsonDiagnostic],
    filename_suffix: &str,
    code: &str,
    line: u32,
) -> Option<&'a JsonDiagnostic> {
    diagnostics
        .iter()
        .find(|d| d.filename.ends_with(filename_suffix) && d.code == code && d.start.line == line)
}

fn assert_diagnostic(
    diagnostics: &[JsonDiagnostic],
    filename_suffix: &str,
    code: &str,
    line: u32,
    message_contains: &str,
) {
    match find_diagnostic(diagnostics, filename_suffix, code, line) {
        Some(d) => assert!(
            d.message.contains(message_contains),
            "expected diagnostic at {filename_suffix}:{line} [{code}] to contain '{message_contains}', got '{}'",
            d.message
        ),
        None => panic!(
            "missing diagnostic at {filename_suffix}:{line} [{code}].\nAll diagnostics in file:\n{:#?}",
            diagnostics
                .iter()
                .filter(|d| d.filename.ends_with(filename_suffix))
                .collect::<Vec<_>>()
        ),
    }
}

fn assert_no_diagnostic_at(
    diagnostics: &[JsonDiagnostic],
    filename_suffix: &str,
    code: &str,
    line: u32,
) {
    if let Some(d) = find_diagnostic(diagnostics, filename_suffix, code, line) {
        panic!(
            "did not expect diagnostic at {filename_suffix}:{line} [{code}], but got: {}",
            d.message
        );
    }
}

// =============================================================================
// 1. Param-matcher narrowing
// =============================================================================

/// The matcher in `src/params/restricted.ts` returns
/// `param === 'alpha' || param === 'beta' || param === 'gamma'`.  TS 5.5+
/// infers a type predicate, which SvelteKit's `MatcherParam<typeof match>`
/// uses to narrow `params.id` to that union at consumer sites.
///
/// `+page.svelte` then assigns `params.id` (the narrow union) to a
/// `'other'` literal — that must error with TS2322.  Before the fix the
/// `: boolean` annotation killed the predicate and `params.id` widened to
/// `string`, so the assignment was silently accepted.
#[test]
fn test_param_matcher_inferred_predicate_narrows_consumer() {
    let diagnostics = diagnostics();
    assert_diagnostic(
        &diagnostics,
        "issue-parity-matcher/[id=restricted]/+page.svelte",
        "TS2322",
        17,
        "not assignable to type '\"other\"'",
    );
}

/// Sibling-positive: assigning `params.id` to the matching union is
/// well-typed and must NOT trigger TS2322.  This guards against a future
/// regression where the matcher overcorrects the other way and widens the
/// type to `never` (or similar).
#[test]
fn test_param_matcher_correct_assignment_is_accepted() {
    let diagnostics = diagnostics();
    assert_no_diagnostic_at(
        &diagnostics,
        "issue-parity-matcher/[id=restricted]/+page.svelte",
        "TS2322",
        12,
    );
}

// =============================================================================
// 2. HTTP method param-annotation respect
// =============================================================================

/// `src/routes/issue-parity-server/+server.ts` types `GET` with the broad
/// `RequestHandler` from `@sveltejs/kit`, where `params` is
/// `Partial<Record<string, string>>`.  Passing `params.id` (a
/// `string | undefined`) to a `string`-only function must error with
/// TS2345 — the loose typing is the safety net the user opted into.
///
/// Before the fix, the route transform unconditionally injected an inner
/// `: RequestEvent` annotation that silently overrode the outer
/// `RequestHandler` and made `params.id` a plain `string`, masking the
/// error.
#[test]
fn test_http_method_with_kit_request_handler_surfaces_param_undefined() {
    let diagnostics = diagnostics();
    assert_diagnostic(
        &diagnostics,
        "issue-parity-server/+server.ts",
        "TS2345",
        22,
        "string | undefined",
    );
}

// =============================================================================
// 3. bind:value write-direction type-checking
// =============================================================================

/// `BindTarget.svelte` exposes `value?: string` — a bindable `string |
/// undefined`.  `+page.svelte` binds `nullable as string | null` to it.
/// `null` isn't assignable to `undefined`, so upstream svelte-check reports
/// TS2322.
///
/// Before the fix, the component-prop slot for `bind:value` emitted
/// `value: undefined as any`, throwing away the user's expression and
/// hiding the mismatch.  Now the bound expression flows through and the
/// error surfaces.
#[test]
fn test_bind_value_with_mismatched_type_surfaces_error() {
    let diagnostics = diagnostics();
    // The error site is at the bind:value attribute on the BindTarget
    // component.  The exact column depends on indentation; we anchor on
    // the line and code instead.
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.ends_with("issue-parity-bind/+page.svelte")
                && d.code == "TS2322"
                && d.start.line == 13
        })
        .collect();

    assert!(
        !matching.is_empty(),
        "expected TS2322 on issue-parity-bind/+page.svelte:13 (bind:value mismatch).\nAll diagnostics in file:\n{:#?}",
        diagnostics
            .iter()
            .filter(|d| d.filename.ends_with("issue-parity-bind/+page.svelte"))
            .collect::<Vec<_>>()
    );

    // The mismatch must specifically be about `string | null` vs the
    // target prop type, not some unrelated error.
    assert!(
        matching
            .iter()
            .any(|d| d.message.contains("string | null") || d.message.contains("null")),
        "expected error to mention `null` / `string | null`, got:\n{:#?}",
        matching
    );
}

// =============================================================================
// 4. SvelteKit zero-types: JSDoc transform helpers for `.js` route/server
// =============================================================================

/// `src/routes/issue-parity-jsdoc/+page.js` and `.../api/+server.js` are
/// *checked* `.js` files (`allowJs` + `checkJs`).  The route transform used
/// to inject TypeScript type-annotation syntax (`load(event: PageLoadEvent)`,
/// `export const prerender: boolean`, `GET(event: RequestEvent)`)
/// unconditionally, which is illegal in a `.js` file and produces TS8010
/// ("Type annotation can only be used in TypeScript files").
///
/// The fix branches on the file extension and emits JSDoc comments instead.
/// This test asserts that ZERO TS8010 diagnostics surface anywhere under
/// `issue-parity-jsdoc/` — the parity-critical assertion mirroring upstream
/// commit b914d010.
#[test]
fn test_jsdoc_zero_types_no_ts8010_on_js_route() {
    let diagnostics = diagnostics();
    let offenders: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.contains("issue-parity-jsdoc") && d.code == "TS8010")
        .collect();
    assert!(
        offenders.is_empty(),
        "expected NO TS8010 on issue-parity-jsdoc/*.js (JSDoc transform must \
         not emit TS type-annotation syntax in checked .js files), got:\n{:#?}",
        offenders
    );
}

/// Proof-of-effectiveness for the JSDoc fix: the injected
/// `/** @param {import('./$types.js').PageLoadEvent} event */` must resolve
/// to the *real* `PageLoadEvent` type, not silently widen to `any` (which
/// would make the no-TS8010 assertion vacuous).  `+page.js`'s `load` reads
/// `event.bogus`, a property that does not exist on `PageLoadEvent`, so a
/// TS2339 must surface at line 22 — and it must NOT be a TS8010.
#[test]
fn test_jsdoc_zero_types_carries_real_load_event_type() {
    let diagnostics = diagnostics();
    let on_file: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.ends_with("issue-parity-jsdoc/+page.js"))
        .collect();

    assert!(
        on_file
            .iter()
            .any(|d| d.code == "TS2339" && d.start.line == 20),
        "expected TS2339 (property does not exist) at issue-parity-jsdoc/+page.js:20, \
         proving the JSDoc @param resolved to a real PageLoadEvent.\nAll diagnostics in file:\n{:#?}",
        on_file
    );
    assert!(
        !on_file.iter().any(|d| d.code == "TS8010"),
        "the JSDoc-typed load must not produce TS8010 in a .js file:\n{:#?}",
        on_file
    );
}

// =============================================================================
// 5. Existing JSDoc @satisfies de-dup (upstream #2946 / commit d69eb726)
// =============================================================================

/// `src/routes/issue-2946-jsdoc-satisfies/+page.js` and `+page.server.js` are
/// *checked* `.js` files whose `load`/`actions` exports the user already typed
/// with a leading `/** @satisfies {...} */` JSDoc tag.  swc strips comments,
/// so the tag never lands in any AST node span — the `expr_contains_satisfies`
/// operator guard can't see it.  Before the fix the route transform injected a
/// *second* `@satisfies` wrap (or a function-like `@param`), producing
/// duplicate-injection syntax/type clashes on the file.
///
/// The fix detects the leading JSDoc `@satisfies` (mirroring upstream's shared
/// `!isTsFile && getJSDocTags(...).some(@satisfies)` `hasTypeDefinition` gate)
/// and leaves the user's source untouched.  This test asserts NO TS8010 and no
/// syntax-level duplicate-injection diagnostics surface anywhere under the
/// route.
#[test]
fn test_jsdoc_satisfies_dedup_no_duplicate_injection() {
    let diagnostics = diagnostics();
    let offenders: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.filename.contains("issue-2946-jsdoc-satisfies")
                // TS8010: TS annotation syntax leaked into a .js file.
                // TS1005/TS1109/TS1128/TS1136: parse errors a duplicated
                // `@satisfies` wrap would produce.
                && matches!(
                    d.code.as_str(),
                    "TS8010" | "TS1005" | "TS1109" | "TS1128" | "TS1136"
                )
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "expected NO TS8010/syntax diagnostics under issue-2946-jsdoc-satisfies \
         (a leading JSDoc @satisfies must suppress re-injection), got:\n{:#?}",
        offenders
    );
}

/// Proof-of-effectiveness for the #2946 fix: the retained
/// `/** @satisfies {import('./$types').PageLoad} */` must resolve to the *real*
/// `PageLoad`/`LoadEvent` type, not silently widen to `any` (which would make
/// the no-duplicate-injection assertion vacuous).  `+page.js`'s `load` reads
/// `event.bogus`, a property absent from `LoadEvent`, so a TS2339 must surface
/// at line 18 — and it must NOT be a TS8010.
#[test]
fn test_jsdoc_satisfies_dedup_carries_real_load_type() {
    let diagnostics = diagnostics();
    let on_file: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.filename.ends_with("issue-2946-jsdoc-satisfies/+page.js"))
        .collect();

    assert!(
        on_file
            .iter()
            .any(|d| d.code == "TS2339" && d.start.line == 18),
        "expected TS2339 (property does not exist) at \
         issue-2946-jsdoc-satisfies/+page.js:18, proving the retained JSDoc \
         @satisfies resolved to a real LoadEvent.\nAll diagnostics in file:\n{:#?}",
        on_file
    );
    assert!(
        !on_file.iter().any(|d| d.code == "TS8010"),
        "the JSDoc @satisfies-typed load must not produce TS8010 in a .js file:\n{:#?}",
        on_file
    );
}

// =============================================================================
// 6. In-tag `@ts-ignore` / `eslint-disable` comments (upstream #2950 / 3a3d6e3a)
// =============================================================================

/// `src/routes/issue-2950-ts-ignore-attribute/+page.svelte` has a `<div>` whose
/// `dir={x}` attribute is type-erroring (`x` is a boolean; `dir` expects a
/// string union → TS2322), but is preceded *inside the tag* by a
/// `// @ts-ignore` comment.
///
/// Before the fix the parser discarded in-tag `//` / `/* */` comments, so the
/// `// @ts-ignore` never reached the generated TypeScript and the error fired.
/// Now the comment is captured as a leading comment of the attribute and
/// re-emitted directly above the attribute's `"dir": x,` line in the generated
/// object literal, so TypeScript suppresses the diagnostic — exactly like
/// upstream svelte2tsx.
#[test]
fn test_ts_ignore_attribute_suppresses_error() {
    let diagnostics = diagnostics();
    // The suppressed `dir={x}` is on line 11 of the fixture.
    assert_no_diagnostic_at(
        &diagnostics,
        "issue-2950-ts-ignore-attribute/+page.svelte",
        "TS2322",
        11,
    );
}

/// Proof-of-effectiveness / targeting guard for the #2950 fix: an identical
/// `<div dir={x}>` WITHOUT the leading `// @ts-ignore` (line 16) must still
/// report TS2322.  This proves the suppression is targeted at the annotated
/// attribute, not a blanket effect of the change.
#[test]
fn test_ts_ignore_attribute_is_targeted() {
    let diagnostics = diagnostics();
    assert_diagnostic(
        &diagnostics,
        "issue-2950-ts-ignore-attribute/+page.svelte",
        "TS2322",
        16,
        "not assignable to type",
    );
}

/// Completion of the #2950 fix to directives / `{@attach}` / `bind:this`
/// (upstream extended comment passthrough to Action/Transition/Animation/
/// AttachTag/Binding/EventHandler in commit 3a3d6e3a).  Previously only
/// element `on:`/`bind:`(non-this) and component prop/spread/bind/attach
/// attributes preserved in-tag comments; every other directive dropped them.
///
/// `src/routes/issue-2950-directives/+page.svelte` has three deliberately
/// type-erroring directives each preceded *inside the tag* by `// @ts-ignore`:
///   - `use:badAction`        (line 25) — would be TS2349 (not callable)
///   - `{@attach badAttach}`  (line 30) — would be TS2345 (wrong attachment)
///   - `bind:this={wrongTypedRef}` (line 35) — would be TS2322 (ref mismatch)
///
/// With the comment re-emitted directly above the generated type-checked
/// statement, none of them fire.
#[test]
fn test_ts_ignore_use_action_suppresses_error() {
    let diagnostics = diagnostics();
    assert_no_diagnostic_at(
        &diagnostics,
        "issue-2950-directives/+page.svelte",
        "TS2349",
        25,
    );
}

#[test]
fn test_ts_ignore_attach_suppresses_error() {
    let diagnostics = diagnostics();
    assert_no_diagnostic_at(
        &diagnostics,
        "issue-2950-directives/+page.svelte",
        "TS2345",
        30,
    );
}

#[test]
fn test_ts_ignore_bind_this_suppresses_error() {
    let diagnostics = diagnostics();
    assert_no_diagnostic_at(
        &diagnostics,
        "issue-2950-directives/+page.svelte",
        "TS2322",
        35,
    );
}

/// Targeting guards: the identical mismatches WITHOUT a leading `// @ts-ignore`
/// (the control `<div>`s on lines 48-50) must still error, proving the
/// suppression is scoped to the annotated directive and not a blanket effect.
#[test]
fn test_ts_ignore_directives_are_targeted() {
    let diagnostics = diagnostics();
    assert_diagnostic(
        &diagnostics,
        "issue-2950-directives/+page.svelte",
        "TS2349",
        48,
        "not callable",
    );
    assert_diagnostic(
        &diagnostics,
        "issue-2950-directives/+page.svelte",
        "TS2345",
        49,
        "not assignable to parameter",
    );
    assert_diagnostic(
        &diagnostics,
        "issue-2950-directives/+page.svelte",
        "TS2322",
        50,
        "not assignable to type",
    );
}

// =============================================================================
// 7. JS param-matcher: no TS8010/TS8037 (upstream #2939 / commit b914d010)
// =============================================================================

/// The params transform appends a trailing `ParamMatcher` constraint to any
/// params file that exports a `match`.  For `.ts` it uses the bare TS
/// `satisfies` operator; for a checked `.js` file (allowJs+checkJs) that
/// operator is TS-only syntax and tsgo reports a false-positive TS8010/TS8037.
///
/// `src/params/restrictedjs.js` is the `.js` counterpart of `restricted.ts`.
/// After the fix the transform emits the JSDoc `@satisfies` cast on the `.js`
/// path instead, so NO TS8010/TS8037 must surface for the matcher file (nor
/// for its consumer route).
#[test]
fn test_js_params_matcher_no_ts8010_ts8037() {
    let diagnostics = diagnostics();
    let leaked: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            (d.filename.contains("params/restrictedjs.js")
                || d.filename.contains("issue-2939-js-params-matcher"))
                && matches!(d.code.as_str(), "TS8010" | "TS8037")
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "no TS-only syntax must leak into the checked .js param matcher \
         (expected zero TS8010/TS8037), got:\n{leaked:#?}"
    );
}

/// Proof-of-effectiveness: the JSDoc `@satisfies` cast must preserve the
/// TS 5.5+ inferred type predicate exactly like the TS operator — otherwise
/// the no-TS8010 assertion above would be vacuous (a cast that widened to
/// `string`/`never` would also be TS8010-free).
///
/// The consumer assigns `params.slug` (narrowed to `"js-a" | "js-b"`) to a
/// non-overlapping `'nope'` literal on line 19 — that must error with TS2322.
/// The matching assignment on line 12 must NOT error.
#[test]
fn test_js_params_matcher_predicate_narrows_consumer() {
    let diagnostics = diagnostics();
    assert_diagnostic(
        &diagnostics,
        "issue-2939-js-params-matcher/[slug=restrictedjs]/+page.svelte",
        "TS2322",
        19,
        "not assignable to type '\"nope\"'",
    );
    assert_no_diagnostic_at(
        &diagnostics,
        "issue-2939-js-params-matcher/[slug=restrictedjs]/+page.svelte",
        "TS2322",
        12,
    );
}
