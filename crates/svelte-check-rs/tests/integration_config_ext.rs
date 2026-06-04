//! Integration tests for `svelte.config.{ts,mts,cjs}` discovery (issue #3009)
//! and `vite.config.*` Svelte-config reading (issue #3031).
//!
//! Upstream commit f53efb39 widens the config-file extension set from
//! `{js,cjs,mjs}` to `{js,ts,cjs,mjs,mts}`. svelte-check-rs parses configs
//! statically with SWC, so the full set is adopted unconditionally (no
//! `process.features.typescript` strip-types gate).
//!
//! Upstream commit 5b13da15 (#3031) additionally reads Svelte config options
//! from `vite.config.{js,mjs,ts,cjs,mts,cts}`, preferring it over svelte.config
//! when it yields plugin options. Upstream runs `vite.resolveConfig` at runtime;
//! svelte-check-rs parses the vite config STATICALLY with SWC (best-effort
//! literal-case approximation) and falls through to svelte.config otherwise.
//!
//! Each test builds a self-contained project on disk under `target/test-tmp/`
//! and runs the CLI with `--show-config --skip-tsgo`. `--show-config` exits
//! before tsgo or bun are invoked and prints the resolved `kit.alias` (read
//! from the svelte config), so these tests don't require `bun install` or
//! `tsgo` to be present and prove the config flows through the orchestrator.

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
        .join("integration_config_ext")
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

struct RunOutput {
    exit_code: i32,
    #[allow(dead_code)]
    stdout: String,
    stderr: String,
}

/// `--show-config` prints resolved config to stderr and returns early (no tsgo
/// or bun). The `kit.alias` line is sourced from the svelte config, so it
/// proves the config file was discovered AND parsed.
fn run_show_config(project: &Path) -> RunOutput {
    ensure_binary_built();
    let output = Command::new(binary_path())
        .arg("--workspace")
        .arg(project)
        .arg("--skip-tsgo")
        .arg("--show-config")
        .output()
        .expect("Failed to execute svelte-check-rs");
    RunOutput {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

/// `svelte.config.ts` (TypeScript syntax with `satisfies` + `const config`
/// export) must be discovered and parsed, with its `$lib` alias honored.
#[test]
fn test_svelte_config_ts_alias_is_honored() {
    let project = make_project("config_ts");
    write(
        &project.join("svelte.config.ts"),
        r#"import type { Config } from '@sveltejs/kit';

const config = {
	kit: {
		alias: {
			'$lib': './src/lib'
		}
	},
	compilerOptions: {
		runes: true
	}
} satisfies Config;

export default config;
"#,
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./src/lib"),
        "expected $lib alias from svelte.config.ts in --show-config output, got stderr:\n{stderr}"
    );
}

/// `svelte.config.mts` uses TypeScript syntax but does NOT end with `.ts`;
/// before the fix it would parse with the ES branch and drop the alias. This
/// asserts the TS branch is selected for `.mts`.
#[test]
fn test_svelte_config_mts_alias_is_honored() {
    let project = make_project("config_mts");
    write(
        &project.join("svelte.config.mts"),
        r#"type Config = { kit: { alias: Record<string, string> } };

const config = {
	kit: {
		alias: {
			'$lib': './src/lib'
		}
	}
} satisfies Config;

export default config;
"#,
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./src/lib"),
        "expected $lib alias from svelte.config.mts (TS syntax branch) in --show-config output, got stderr:\n{stderr}"
    );
}

/// `svelte.config.cjs` with a *real* CommonJS body (`module.exports = { ... }`)
/// must be discovered and parsed through the orchestrator, with its `$lib` alias
/// honored. Previously this test wrote ESM `export default` into a `.cjs` file,
/// masking the gap that the static extractor only understood `export default`.
#[test]
fn test_svelte_config_cjs_alias_is_honored() {
    let project = make_project("config_cjs");
    write(
        &project.join("svelte.config.cjs"),
        "module.exports = { kit: { alias: { '$lib': './src/lib' } } };\n",
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./src/lib"),
        "expected $lib alias from CommonJS svelte.config.cjs in --show-config output, got stderr:\n{stderr}"
    );
}

/// A `.js` config using CommonJS `module.exports = { ... }` must also flow
/// through the orchestrator (the ES branch parses `.js`, and the CommonJS
/// extraction path picks up `module.exports`).
#[test]
fn test_svelte_config_js_commonjs_alias_is_honored() {
    let project = make_project("config_js_cjs");
    write(
        &project.join("svelte.config.js"),
        "module.exports = { kit: { alias: { '$lib': './src/lib' } } };\n",
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./src/lib"),
        "expected $lib alias from CommonJS svelte.config.js in --show-config output, got stderr:\n{stderr}"
    );
}

/// Issue #3031: a `vite.config.ts` declaring `svelte({ kit: { alias } })` (and
/// NO svelte.config.*) must be discovered and its `$lib` alias honored. This is
/// the static approximation; upstream runs vite.resolveConfig at runtime.
#[test]
fn test_vite_config_ts_alias_is_honored() {
    let project = make_project("vite_config_ts");
    write(
        &project.join("vite.config.ts"),
        r#"import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
	plugins: [
		svelte({
			kit: {
				alias: {
					'$lib': './src/lib'
				}
			}
		})
	]
});
"#,
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./src/lib"),
        "expected $lib alias from vite.config.ts svelte() plugin in --show-config output, got stderr:\n{stderr}"
    );
}

/// Issue #3031: when BOTH a vite.config.ts (with options) and a svelte.config.js
/// exist, the vite.config options win (vite-preferred precedence, no merge).
#[test]
fn test_vite_config_wins_over_svelte_config() {
    let project = make_project("vite_config_precedence");
    write(
        &project.join("vite.config.ts"),
        r#"import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
	plugins: [
		svelte({
			kit: {
				alias: {
					'$lib': './from-vite'
				}
			}
		})
	]
});
"#,
    );
    write(
        &project.join("svelte.config.js"),
        "export default { kit: { alias: { '$lib': './from-svelte' } } };\n",
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./from-vite"),
        "expected vite.config alias (./from-vite) to win, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("./from-svelte"),
        "svelte.config alias (./from-svelte) must NOT appear when vite.config yields options, got stderr:\n{stderr}"
    );
}

/// Issue #3031 regression: a bare `sveltekit()` vite.config (no options object)
/// yields nothing, so load() must fall through to svelte.config.js. This guards
/// the existing sveltekit-bundler/nodenext/svelte-modules fixtures.
#[test]
fn test_bare_sveltekit_vite_config_falls_through_to_svelte_config() {
    let project = make_project("vite_config_bare_sveltekit");
    write(
        &project.join("vite.config.ts"),
        r#"import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
	plugins: [sveltekit()]
});
"#,
    );
    write(
        &project.join("svelte.config.js"),
        "export default { kit: { alias: { '$lib': './src/lib' } } };\n",
    );
    write_minimal_tsconfig(&project);
    write(&project.join("src/App.svelte"), "<p>hi</p>\n");

    let RunOutput {
        exit_code, stderr, ..
    } = run_show_config(&project);

    assert_eq!(exit_code, 0, "exit code should be 0, stderr: {stderr}");
    assert!(
        stderr.contains("kit.alias:") && stderr.contains("$lib") && stderr.contains("./src/lib"),
        "expected fall-through to svelte.config.js $lib alias when vite has only bare sveltekit(), got stderr:\n{stderr}"
    );
}
