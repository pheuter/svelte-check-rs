//! tsgo process runner.

use crate::kit;
use crate::parser::{parse_tsgo_output, TsgoDiagnostic};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Map, Value};
use source_map::SourceMap;
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};
use swc_common::{FileName, SourceMap as SwcSourceMap};
use swc_ecma_ast::{Decl, ExportDecl, FnDecl, Module, ModuleDecl, ModuleItem, Pat};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax};
use thiserror::Error;
use tokio::process::Command;
use walkdir::WalkDir;

const SHARED_HELPERS_FILENAME: &str = "__svelte_check_rs_helpers.d.ts";
const SHARED_HELPERS_DTS: &str = r#"import type { ComponentInternals as SvelteComponentInternals, Snippet as SvelteSnippet } from "svelte";
import type { SvelteHTMLElements as SvelteHTMLElements, HTMLAttributes as SvelteHTMLAttributes } from "svelte/elements";

export {};

declare global {
  type __SvelteComponent<
    Props extends Record<string, any> = {},
    Exports extends Record<string, any> = {}
  > = {
    (this: void, internals: SvelteComponentInternals, props: Props): {
      $on?(type: string, callback: (e: any) => void): () => void;
      $set?(props: Partial<Props>): void;
    } & Exports;
    element?: typeof HTMLElement;
    z_$$bindings?: string;
  };

  type __SvelteSnippet<T extends any[] = any[]> = SvelteSnippet<T>;

  type __SvelteEachItem<T> =
    T extends ArrayLike<infer U> ? U :
    T extends Iterable<infer U> ? U :
    never;

  declare function __svelte_each_indexed<
    T extends ArrayLike<unknown> | Iterable<unknown> | null | undefined
  >(arr: T): [number, __SvelteEachItem<T>][];
  declare function __svelte_is_empty<T extends ArrayLike<unknown> | Iterable<unknown> | null | undefined>(arr: T): boolean;

  declare function __svelte_store_get<T>(store: { subscribe(fn: (value: T) => void): any }): T;

  declare function __svelte_effect(fn: () => void | (() => void)): void;
  declare function __svelte_effect_pre(fn: () => void | (() => void)): void;
  declare function __svelte_effect_root(fn: (...args: any[]) => any): void;

  type __StoreValue<S> = S extends { subscribe(fn: (value: infer T) => void): any } ? T : never;

  type __SvelteOptionalProps<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

  type __SvelteLoosen<T> =
    T extends (...args: any) => any ? T :
    T extends readonly any[] ? T :
    T extends object ? T & Record<string, any> : T;

  type __SveltePropsAccessor<T> = { [K in keyof T]: () => T[K] } & Record<string, () => any>;

  declare const __svelte_snippet_return: ReturnType<SvelteSnippet<[]>>;

  type __SvelteEvent<Target extends EventTarget, E extends Event> = E & {
    currentTarget: Target;
    target: Target;
  };

  type __SvelteIntrinsicElements = SvelteHTMLElements;
  type __SvelteEventProps<T> =
    T & { [K in keyof T as K extends `on:${infer E}` ? `on${E}` : never]?: T[K] };
  type __SvelteElementAttributes<K extends string> =
    __SvelteEventProps<
      K extends keyof __SvelteIntrinsicElements ? __SvelteIntrinsicElements[K] : SvelteHTMLAttributes<any>
    >;

  declare function __svelte_check_element<K extends string>(
    tag: K | undefined | null,
    attrs: __SvelteElementAttributes<K>
  ): void;

  declare const __svelte_any: any;

  declare function __svelte_catch_error<T>(value: T): unknown;
}
"#;

/// Supported package managers for installing tsgo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// Detect package manager from lockfiles in the workspace.
    /// Walks up the directory tree to support monorepo setups where lockfiles
    /// are at the root rather than in nested packages.
    pub fn detect_from_workspace(workspace_root: &Utf8Path) -> Option<Self> {
        let mut current = Some(workspace_root);

        while let Some(dir) = current {
            // Check most specific lockfiles first
            if dir.join("pnpm-lock.yaml").exists() {
                return Some(Self::Pnpm);
            }
            if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
                return Some(Self::Bun);
            }
            if dir.join("yarn.lock").exists() {
                return Some(Self::Yarn);
            }
            if dir.join("package-lock.json").exists() {
                return Some(Self::Npm);
            }

            current = dir.parent();
        }

        None
    }

    /// Detect any available package manager from PATH.
    pub fn detect_from_path() -> Option<Self> {
        // Check in order of preference
        if which::which("npm").is_ok() {
            return Some(Self::Npm);
        }
        if which::which("pnpm").is_ok() {
            return Some(Self::Pnpm);
        }
        if which::which("yarn").is_ok() {
            return Some(Self::Yarn);
        }
        if which::which("bun").is_ok() {
            return Some(Self::Bun);
        }
        None
    }

    /// Returns the command name for this package manager.
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    /// Returns the install arguments for this package manager.
    pub fn install_args(&self, package: &str) -> Vec<String> {
        match self {
            Self::Npm => vec!["install".to_string(), package.to_string()],
            Self::Pnpm => vec!["add".to_string(), package.to_string()],
            Self::Yarn => vec!["add".to_string(), package.to_string()],
            Self::Bun => vec!["add".to_string(), package.to_string()],
        }
    }
}

/// Error types for tsgo runner.
#[derive(Debug, Error)]
pub enum TsgoError {
    /// Failed to spawn tsgo process.
    #[error("failed to spawn tsgo: {0}")]
    SpawnFailed(#[from] std::io::Error),

    /// tsgo process exited with error.
    #[error("tsgo exited with code {code}: {stderr}")]
    ProcessFailed { code: i32, stderr: String },

    /// Failed to parse tsgo output.
    #[error("failed to parse tsgo output: {0}")]
    ParseFailed(String),

    /// tsconfig not found.
    #[error("tsconfig not found at: {0}")]
    TsconfigNotFound(Utf8PathBuf),

    /// tsgo binary not found.
    #[error("tsgo binary not found at: {0}")]
    NotFound(Utf8PathBuf),

    /// Failed to write temporary files.
    #[error("failed to write temporary files: {0}")]
    TempFileFailed(String),

    /// Failed to install tsgo.
    #[error("failed to install tsgo: {0}")]
    InstallFailed(String),

    /// No package manager found.
    #[error(
        "no package manager found - please install npm, pnpm, yarn, or bun to auto-download tsgo"
    )]
    PackageManagerNotFound,

    /// svelte-kit sync failed.
    #[error("svelte-kit sync failed: {0}")]
    SvelteKitSyncFailed(String),
}

/// The tsgo runner.
pub struct TsgoRunner {
    /// Path to the tsgo binary.
    tsgo_path: Utf8PathBuf,
    /// Project root directory.
    project_root: Utf8PathBuf,
    /// Optional tsconfig path override.
    tsconfig_path: Option<Utf8PathBuf>,
    /// Whether to cache .svelte-kit contents in a stable location.
    use_sveltekit_cache: bool,
}

#[derive(Debug, Default, Clone)]
pub struct TsgoCacheStats {
    pub tsx_written: usize,
    pub tsx_skipped: usize,
    pub stub_written: usize,
    pub stub_skipped: usize,
    pub kit_written: usize,
    pub kit_skipped: usize,
    pub patched_written: usize,
    pub patched_skipped: usize,
    pub shim_written: usize,
    pub shim_skipped: usize,
    pub tsconfig_written: usize,
    pub tsconfig_skipped: usize,
    pub source_entries: usize,
    pub source_dirs: usize,
    pub source_files: usize,
    pub source_svelte_skipped: usize,
    pub source_existing_skipped: usize,
    pub source_linked: usize,
    pub source_copied: usize,
    pub stale_removed: usize,
}

#[derive(Debug, Default, Clone)]
pub struct TsgoTimingStats {
    pub write_time: Duration,
    pub source_tree_time: Duration,
    pub tsconfig_time: Duration,
    pub tsgo_time: Duration,
    pub parse_time: Duration,
    pub total_time: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct TsgoCheckStats {
    pub cache: TsgoCacheStats,
    pub timings: TsgoTimingStats,
    pub diagnostics: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct TsgoCheckOutput {
    pub diagnostics: Vec<TsgoDiagnostic>,
    pub stats: TsgoCheckStats,
}

impl TsgoRunner {
    /// Creates a new tsgo runner.
    pub fn new(
        tsgo_path: Utf8PathBuf,
        project_root: Utf8PathBuf,
        tsconfig_path: Option<Utf8PathBuf>,
        use_sveltekit_cache: bool,
    ) -> Self {
        Self {
            tsgo_path,
            project_root,
            tsconfig_path,
            use_sveltekit_cache,
        }
    }

    /// Attempts to find tsgo in workspace, PATH, or common locations.
    ///
    /// Search order:
    /// 1. Workspace node_modules/.bin/tsgo (if workspace_root provided)
    /// 2. System PATH
    /// 3. Common installation locations
    /// 4. Cache directory
    pub fn find_tsgo(workspace_root: Option<&Utf8Path>) -> Option<Utf8PathBuf> {
        // Check workspace node_modules first (most reliable for user's project)
        if let Some(workspace) = workspace_root {
            let workspace_tsgo = workspace.join("node_modules/.bin/tsgo");
            if workspace_tsgo.exists() {
                return Some(workspace_tsgo);
            }
        }

        // Try PATH
        if let Ok(path) = which::which("tsgo") {
            if let Ok(utf8_path) = Utf8PathBuf::try_from(path) {
                return Some(utf8_path);
            }
        }

        // Try common locations
        let common_paths = ["/usr/local/bin/tsgo", "/usr/bin/tsgo", "~/.local/bin/tsgo"];

        for path in common_paths {
            let expanded = shellexpand::tilde(path);
            let path = Utf8Path::new(expanded.as_ref());
            if path.exists() {
                return Some(path.to_owned());
            }
        }

        // Try cache directory
        if let Some(cache_dir) = Self::get_cache_dir() {
            let tsgo_path = cache_dir.join("node_modules/.bin/tsgo");
            if tsgo_path.exists() {
                return Some(tsgo_path);
            }
        }

        None
    }

    /// Gets the cache directory for svelte-check-rs.
    pub fn get_cache_dir() -> Option<Utf8PathBuf> {
        // Use XDG cache dir on Linux, ~/Library/Caches on macOS, etc.
        dirs::cache_dir()
            .and_then(|p| Utf8PathBuf::try_from(p).ok())
            .map(|p| p.join("svelte-check-rs"))
    }

    /// Gets the version of the installed tsgo binary.
    ///
    /// Returns a tuple of (version_string, path) or an error if tsgo is not found.
    pub async fn get_tsgo_version() -> Result<(String, Utf8PathBuf), TsgoError> {
        let tsgo_path = Self::find_tsgo(None).ok_or_else(|| {
            TsgoError::InstallFailed("tsgo not found - run with --tsgo-update to install".into())
        })?;

        let output = Command::new(&tsgo_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(TsgoError::SpawnFailed)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TsgoError::ProcessFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            });
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((version, tsgo_path))
    }

    /// Updates tsgo to the specified version or latest if None.
    ///
    /// This will install/update tsgo using the detected package manager in the cache directory.
    pub async fn update_tsgo(version: Option<&str>) -> Result<Utf8PathBuf, TsgoError> {
        // Detect package manager from PATH
        let pm = PackageManager::detect_from_path().ok_or(TsgoError::PackageManagerNotFound)?;

        // Get or create cache directory
        let cache_dir = Self::get_cache_dir().ok_or_else(|| {
            TsgoError::InstallFailed("could not determine cache directory".into())
        })?;

        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| TsgoError::InstallFailed(format!("failed to create cache dir: {e}")))?;

        let package_spec = match version {
            Some(v) => format!("@typescript/native-preview@{}", v),
            None => "@typescript/native-preview@latest".to_string(),
        };

        eprintln!("Installing {} using {}...", package_spec, pm.command_name());

        // Run package manager install in cache directory
        let install_args = pm.install_args(&package_spec);
        let output = Command::new(pm.command_name())
            .args(&install_args)
            .current_dir(&cache_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                TsgoError::InstallFailed(format!("failed to run {}: {e}", pm.command_name()))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TsgoError::InstallFailed(format!(
                "{} install failed: {stderr}",
                pm.command_name()
            )));
        }

        // Verify installation
        let tsgo_path = cache_dir.join("node_modules/.bin/tsgo");
        if !tsgo_path.exists() {
            return Err(TsgoError::InstallFailed(format!(
                "tsgo binary not found after {} install",
                pm.command_name()
            )));
        }

        // Get and display the installed version
        let version_output = Command::new(&tsgo_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .output()
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        if let Some(v) = version_output {
            eprintln!("tsgo {} installed at {}", v, tsgo_path);
        } else {
            eprintln!("tsgo installed at {}", tsgo_path);
        }

        Ok(tsgo_path)
    }

    /// Runs `svelte-kit sync` to generate types before type-checking.
    ///
    /// This will:
    /// 1. Check if the project uses SvelteKit (has @sveltejs/kit in node_modules)
    /// 2. Find `svelte-kit` binary in node_modules/.bin (searches parent dirs for monorepos)
    /// 3. Run `svelte-kit sync` to generate/update types
    ///
    /// Returns `Ok(true)` if sync was run, `Ok(false)` if skipped (not a SvelteKit project).
    pub async fn ensure_sveltekit_sync(project_root: &Utf8Path) -> Result<bool, TsgoError> {
        // Check if this is a SvelteKit project by searching for @sveltejs/kit
        // in node_modules, including parent directories for monorepo support
        if !Self::is_sveltekit_project(project_root) {
            return Ok(false);
        }

        let signature = Self::compute_sveltekit_sync_signature(project_root);
        if let Some(signature) = signature {
            let state_path = project_root.join(".svelte-check-rs/kit-sync.sig");
            if let Some(previous) = read_signature(&state_path) {
                if previous == signature {
                    return Ok(false);
                }
            }

            // Find svelte-kit binary in node_modules
            let svelte_kit_bin = Self::find_sveltekit_binary(project_root)?;

            // Run svelte-kit sync
            let output = Command::new(&svelte_kit_bin)
                .arg("sync")
                .current_dir(project_root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| TsgoError::SvelteKitSyncFailed(format!("failed to run: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(TsgoError::SvelteKitSyncFailed(format!(
                    "exited with code {}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr
                )));
            }

            write_signature(&state_path, &signature)?;
            return Ok(true);
        }

        // Avoid re-running sync when no route/config inputs changed.
        if !Self::sveltekit_sync_needed(project_root) {
            return Ok(false);
        }

        // Run svelte-kit sync
        let svelte_kit_bin = Self::find_sveltekit_binary(project_root)?;
        let output = Command::new(&svelte_kit_bin)
            .arg("sync")
            .current_dir(project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| TsgoError::SvelteKitSyncFailed(format!("failed to run: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TsgoError::SvelteKitSyncFailed(format!(
                "exited with code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }

        Ok(true)
    }

    /// Checks if this is a SvelteKit project by searching for @sveltejs/kit
    /// in node_modules, walking up the directory tree for monorepo support.
    fn is_sveltekit_project(project_root: &Utf8Path) -> bool {
        let mut current = Some(project_root);

        while let Some(dir) = current {
            let kit_package = dir.join("node_modules/@sveltejs/kit");
            if kit_package.exists() {
                return true;
            }
            current = dir.parent();
        }

        false
    }

    /// Finds the svelte-kit binary by searching node_modules/.bin up the directory tree.
    /// This handles monorepo setups where dependencies may be hoisted to a parent directory.
    pub fn find_sveltekit_binary(project_root: &Utf8Path) -> Result<Utf8PathBuf, TsgoError> {
        let mut current = Some(project_root);

        while let Some(dir) = current {
            // Try node_modules/.bin/svelte-kit
            let bin_path = dir.join("node_modules/.bin/svelte-kit");
            if bin_path.exists() {
                return Ok(bin_path);
            }

            // Try the package's bin directly
            let pkg_bin = dir.join("node_modules/@sveltejs/kit/svelte-kit.js");
            if pkg_bin.exists() {
                return Ok(pkg_bin);
            }

            current = dir.parent();
        }

        Err(TsgoError::SvelteKitSyncFailed(
            "svelte-kit binary not found in node_modules (searched parent directories)".into(),
        ))
    }

    fn sveltekit_sync_needed(project_root: &Utf8Path) -> bool {
        let kit_tsconfig = project_root.join(".svelte-kit/tsconfig.json");
        let baseline = match std::fs::metadata(&kit_tsconfig).and_then(|m| m.modified()) {
            Ok(time) => time,
            Err(_) => return true,
        };

        let config_candidates = [
            "svelte.config.js",
            "svelte.config.cjs",
            "svelte.config.mjs",
            "svelte.config.ts",
        ];
        for name in config_candidates {
            let path = project_root.join(name);
            if is_newer_than(&path, baseline) {
                return true;
            }
        }

        let hooks_dir = project_root.join("src");
        if hooks_dir.exists() && src_hooks_changed(&hooks_dir, baseline) {
            return true;
        }

        let params_dir = project_root.join("src/params");
        if params_dir.exists() && dir_has_newer_mtime(&params_dir, baseline) {
            return true;
        }

        let routes_dir = project_root.join("src/routes");
        if routes_dir.exists() && routes_changed(&routes_dir, baseline) {
            return true;
        }

        false
    }

    fn compute_sveltekit_sync_signature(project_root: &Utf8Path) -> Option<String> {
        let mut signature = String::new();
        signature.push_str("v1\n");

        let config_candidates = [
            "svelte.config.js",
            "svelte.config.cjs",
            "svelte.config.mjs",
            "svelte.config.ts",
        ];
        for name in config_candidates {
            let path = project_root.join(name);
            if path.exists() {
                let content = std::fs::read_to_string(&path).ok()?;
                signature.push_str("config:");
                signature.push_str(name);
                signature.push('\n');
                signature.push_str(&content);
                signature.push_str("\n--\n");
            }
        }

        let hooks_dir = project_root.join("src");
        if hooks_dir.exists() {
            let mut hook_entries: Vec<(String, Vec<String>)> = Vec::new();
            let entries = std::fs::read_dir(hooks_dir.as_std_path()).ok()?;
            for entry in entries {
                let entry = entry.ok()?;
                let path_buf = entry.path();
                let path = Utf8Path::from_path(&path_buf)?;
                if !path.is_file() {
                    continue;
                }
                let file_name = match path.file_name() {
                    Some(name) => name,
                    None => continue,
                };
                if !file_name.starts_with("hooks.") {
                    continue;
                }
                if !matches!(path.extension(), Some("ts" | "js")) {
                    continue;
                }
                let source = std::fs::read_to_string(path).ok()?;
                let exports = extract_relevant_exports(path, &source, is_relevant_hook_export)?;
                let rel = path.strip_prefix(project_root).unwrap_or(path).to_string();
                hook_entries.push((rel, exports));
            }
            hook_entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (rel, exports) in hook_entries {
                signature.push_str("hook:");
                signature.push_str(&rel);
                signature.push('|');
                signature.push_str(&exports.join(","));
                signature.push('\n');
            }
        }

        let params_dir = project_root.join("src/params");
        if params_dir.exists() {
            let mut params: Vec<String> = Vec::new();
            for entry in WalkDir::new(&params_dir).follow_links(false) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = match Utf8Path::from_path(entry.path()) {
                    Some(path) => path,
                    None => continue,
                };
                let file_name = match path.file_name() {
                    Some(file_name) => file_name,
                    None => continue,
                };
                if file_name.contains(".test") || file_name.contains(".spec") {
                    continue;
                }
                if !matches!(path.extension(), Some("ts" | "js")) {
                    continue;
                }
                let rel = path.strip_prefix(project_root).unwrap_or(path).to_string();
                params.push(rel);
            }
            params.sort();
            for rel in params {
                signature.push_str("param:");
                signature.push_str(&rel);
                signature.push('\n');
            }
        }

        let routes_dir = project_root.join("src/routes");
        if routes_dir.exists() {
            struct RouteEntry {
                rel: String,
                detail: String,
            }
            let mut routes: Vec<RouteEntry> = Vec::new();
            for entry in WalkDir::new(&routes_dir).follow_links(false) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = match Utf8Path::from_path(entry.path()) {
                    Some(path) => path,
                    None => continue,
                };
                let kind = match route_signature_kind(path) {
                    Some(kind) => kind,
                    None => continue,
                };
                let rel = path.strip_prefix(project_root).unwrap_or(path).to_string();
                let detail = match kind {
                    RouteSignatureKind::Svelte => "svelte".to_string(),
                    RouteSignatureKind::Script => {
                        let source = std::fs::read_to_string(path).ok()?;
                        let exports =
                            extract_relevant_exports(path, &source, is_relevant_route_export)?;
                        format!("exports:{}", exports.join(","))
                    }
                };
                routes.push(RouteEntry { rel, detail });
            }
            routes.sort_by(|a, b| a.rel.cmp(&b.rel));
            for entry in routes {
                signature.push_str("route:");
                signature.push_str(&entry.rel);
                signature.push('|');
                signature.push_str(&entry.detail);
                signature.push('\n');
            }
        }

        Some(signature)
    }

    /// Finds tsgo or installs it if not found.
    ///
    /// This will:
    /// 1. Check workspace node_modules/.bin/tsgo (if workspace_root provided)
    /// 2. Check if tsgo is in PATH
    /// 3. Check common installation locations
    /// 4. Check the cache directory
    /// 5. If not found, install using detected package manager in the cache directory
    pub async fn ensure_tsgo(workspace_root: Option<&Utf8Path>) -> Result<Utf8PathBuf, TsgoError> {
        // First try to find existing installation
        if let Some(path) = Self::find_tsgo(workspace_root) {
            return Ok(path);
        }

        // Detect package manager (prefer workspace lockfile, fallback to PATH)
        let pm = workspace_root
            .and_then(PackageManager::detect_from_workspace)
            .or_else(PackageManager::detect_from_path)
            .ok_or(TsgoError::PackageManagerNotFound)?;

        // Get or create cache directory
        let cache_dir = Self::get_cache_dir().ok_or_else(|| {
            TsgoError::InstallFailed("could not determine cache directory".into())
        })?;

        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| TsgoError::InstallFailed(format!("failed to create cache dir: {e}")))?;

        eprintln!(
            "tsgo not found, installing @typescript/native-preview using {}...",
            pm.command_name()
        );

        // Run package manager install in cache directory
        let install_args = pm.install_args("@typescript/native-preview");
        let output = Command::new(pm.command_name())
            .args(&install_args)
            .current_dir(&cache_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                TsgoError::InstallFailed(format!("failed to run {}: {e}", pm.command_name()))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TsgoError::InstallFailed(format!(
                "{} install failed: {stderr}",
                pm.command_name()
            )));
        }

        // Verify installation
        let tsgo_path = cache_dir.join("node_modules/.bin/tsgo");
        if !tsgo_path.exists() {
            return Err(TsgoError::InstallFailed(format!(
                "tsgo binary not found after {} install",
                pm.command_name()
            )));
        }

        eprintln!("tsgo installed successfully at {}", tsgo_path);
        Ok(tsgo_path)
    }

    /// Resolve the tsconfig path to use.
    fn resolve_tsconfig_path(&self) -> Result<Utf8PathBuf, TsgoError> {
        let candidate = if let Some(path) = &self.tsconfig_path {
            if path.is_relative() {
                self.project_root.join(path)
            } else {
                path.clone()
            }
        } else {
            self.project_root.join("tsconfig.json")
        };

        if candidate.exists() {
            Ok(candidate)
        } else {
            Err(TsgoError::TsconfigNotFound(candidate))
        }
    }

    /// Prepare a tsconfig overlay inside the temp directory.
    ///
    /// This symlinks the existing project tsconfig into the temp workspace so
    /// relative paths (like `./.svelte-kit/tsconfig.json`) resolve against the temp root.
    fn prepare_tsconfig_overlay(
        &self,
        temp_root: &Utf8Path,
        tsconfig_path: &Utf8Path,
        stats: &mut TsgoCacheStats,
        overrides: Option<&Map<String, Value>>,
    ) -> Result<Utf8PathBuf, TsgoError> {
        let temp_tsconfig = if let Ok(rel) = tsconfig_path.strip_prefix(&self.project_root) {
            temp_root.join(rel)
        } else {
            // Fall back to placing the config at the temp root
            let name = tsconfig_path.file_name().unwrap_or("tsconfig.json");
            temp_root.join(name)
        };

        if let Some(parent) = temp_tsconfig.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        }

        if !temp_tsconfig.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(tsconfig_path, &temp_tsconfig)
                .map_err(|e| TsgoError::TempFileFailed(format!("symlink tsconfig: {e}")))?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(tsconfig_path, &temp_tsconfig)
                .map_err(|e| TsgoError::TempFileFailed(format!("symlink tsconfig: {e}")))?;
        }

        if let Some(overrides) = overrides {
            if !overrides.is_empty() {
                let overlay_path = temp_tsconfig.with_file_name("tsconfig.tsgo.json");
                let extends_value = temp_tsconfig
                    .file_name()
                    .unwrap_or("tsconfig.json")
                    .to_string();
                let mut root = Map::new();
                root.insert(
                    "extends".to_string(),
                    Value::String(format!("./{}", extends_value)),
                );
                root.insert(
                    "compilerOptions".to_string(),
                    Value::Object(overrides.clone()),
                );
                let content = serde_json::to_string_pretty(&Value::Object(root))
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                let wrote =
                    write_if_changed(&overlay_path, content.as_bytes(), "write tsconfig overlay")?;
                if wrote {
                    stats.tsconfig_written += 1;
                } else {
                    stats.tsconfig_skipped += 1;
                }
                return Ok(overlay_path);
            }
        }

        Ok(temp_tsconfig)
    }

    /// Mirrors the entire source directory tree from the project to the temp directory.
    /// This preserves the exact directory structure so all relative imports resolve correctly.
    /// .svelte files are skipped since we write transformed .tsx versions.
    fn symlink_source_tree(
        project_root: &Utf8Path,
        project_src: &Utf8Path,
        temp_src: &Utf8Path,
        apply_tsgo_fixes: bool,
        stats: &mut TsgoCacheStats,
    ) -> Result<(), TsgoError> {
        // Always create temp_src so shim can be written even if project has no src dir
        std::fs::create_dir_all(temp_src).map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;

        // Track files and directories we write to the cache
        let mut cache_files: HashSet<Utf8PathBuf> = HashSet::new();
        let mut cache_dirs: HashSet<Utf8PathBuf> = HashSet::new();
        cache_dirs.insert(temp_src.to_owned());

        if project_src.exists() {
            for entry in WalkDir::new(project_src).into_iter().filter_map(|e| e.ok()) {
                stats.source_entries += 1;
                let path = match Utf8Path::from_path(entry.path()) {
                    Some(p) => p,
                    None => continue,
                };

                // Calculate relative path
                let relative = match path.strip_prefix(project_src) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let target = temp_src.join(relative);

                // Create directories
                if entry.file_type().is_dir() {
                    stats.source_dirs += 1;
                    std::fs::create_dir_all(&target)
                        .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                    cache_dirs.insert(target);
                    continue;
                }

                stats.source_files += 1;

                // Skip .svelte files - we write transformed .ts versions
                if path.extension() == Some("svelte") {
                    stats.source_svelte_skipped += 1;
                    continue;
                }

                if let Some(kind) = kit::kit_file_kind(path, project_root) {
                    let source = std::fs::read_to_string(path)
                        .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                    let mut transformed =
                        kit::transform_kit_source(kind, path, &source).unwrap_or(source);
                    if apply_tsgo_fixes {
                        if let Some(patched) = patch_promise_all_empty_arrays(&transformed) {
                            transformed = patched;
                        }
                    }
                    let wrote =
                        write_if_changed(&target, transformed.as_bytes(), "write kit transform")?;
                    if wrote {
                        stats.kit_written += 1;
                    } else {
                        stats.kit_skipped += 1;
                    }
                    cache_files.insert(target);
                    continue;
                }

                // Skip if target already exists (transformed file takes precedence)
                if target.exists() {
                    stats.source_existing_skipped += 1;
                    cache_files.insert(target);
                    continue;
                }

                if apply_tsgo_fixes && is_ts_like_file(path) {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if content.contains("Promise.all") {
                            if let Some(patched) = patch_promise_all_empty_arrays(&content) {
                                let wrote = write_if_changed(
                                    &target,
                                    patched.as_bytes(),
                                    "write patched ts",
                                )?;
                                if wrote {
                                    stats.patched_written += 1;
                                } else {
                                    stats.patched_skipped += 1;
                                }
                                cache_files.insert(target);
                                continue;
                            }
                        }
                    }
                }

                // Prefer hard links to keep paths under the temp root. Fall back to copying.
                if let Err(err) = std::fs::hard_link(path, &target) {
                    stats.source_copied += 1;
                    std::fs::copy(path, &target).map_err(|e| {
                        TsgoError::TempFileFailed(format!(
                            "link/copy {}: hard link error {}, copy error {}",
                            relative, err, e
                        ))
                    })?;
                } else {
                    stats.source_linked += 1;
                }
                cache_files.insert(target);
            }
        }

        // Clean up stale source files (from deleted .ts files)
        // Skip .svelte.ts and .svelte.d.ts files - those are handled elsewhere
        for entry in WalkDir::new(temp_src).follow_links(false).into_iter() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let path = match Utf8Path::from_path(entry.path()) {
                Some(path) => path,
                None => continue,
            };
            if entry.file_type().is_file() {
                let path_str = path.as_str();
                // Skip .svelte.ts and .svelte.d.ts - handled in check()
                if path_str.ends_with(".svelte.ts") || path_str.ends_with(".svelte.d.ts") {
                    continue;
                }
                // Skip shim file
                if path_str.ends_with("__svelte_check_rs_shims.d.ts") {
                    continue;
                }
                if !cache_files.contains(path) {
                    let _ = std::fs::remove_file(path);
                    stats.stale_removed += 1;
                }
            }
        }
        // Remove empty directories (contents_first ensures we process children before parents)
        for entry in WalkDir::new(temp_src)
            .follow_links(false)
            .contents_first(true)
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let path = match Utf8Path::from_path(entry.path()) {
                Some(path) => path,
                None => continue,
            };
            if entry.file_type().is_dir() && !cache_dirs.contains(path) {
                // Try to remove - will fail if not empty, which is fine
                let _ = std::fs::remove_dir(path);
            }
        }

        Ok(())
    }

    /// Runs type-checking on the transformed files.
    pub async fn check(
        &self,
        files: &TransformedFiles,
        emit_diagnostics: bool,
    ) -> Result<TsgoCheckOutput, TsgoError> {
        let total_start = Instant::now();
        let mut stats = TsgoCheckStats::default();
        let strict_function_types =
            read_env_bool("SVELTE_CHECK_RS_TSGO_STRICT_FUNCTION_TYPES").unwrap_or(false);
        let apply_tsgo_fixes = !strict_function_types;
        let mut tsconfig_overrides = Map::new();
        tsconfig_overrides.insert(
            "strictFunctionTypes".to_string(),
            Value::Bool(strict_function_types),
        );

        // Enable incremental builds for faster subsequent runs
        tsconfig_overrides.insert("incremental".to_string(), Value::Bool(true));
        let tsbuildinfo_path = self.project_root.join(".svelte-check-rs/tsgo.tsbuildinfo");
        tsconfig_overrides.insert(
            "tsBuildInfoFile".to_string(),
            Value::String(tsbuildinfo_path.to_string()),
        );

        // Verify tsgo exists
        if !self.tsgo_path.exists() {
            return Err(TsgoError::NotFound(self.tsgo_path.clone()));
        }

        // Use a stable cache directory for transformed files inside the project root.
        // This enables tsgo incremental builds (file paths stay the same between runs)
        // and avoids the overhead of creating/deleting temp directories.
        let cache_path = self.project_root.join(".svelte-check-rs/cache");
        std::fs::create_dir_all(&cache_path)
            .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        let temp_path = &cache_path;

        let write_start = Instant::now();
        // Write transformed files
        let mut tsx_files = Vec::new();
        let mut cache_files: HashSet<Utf8PathBuf> = HashSet::new();
        let mut cache_dirs: HashSet<Utf8PathBuf> = HashSet::new();
        cache_dirs.insert(cache_path.clone());

        for (virtual_path, file) in &files.files {
            let full_path = temp_path.join(virtual_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                // Track all parent directories
                let mut dir = parent.to_owned();
                while dir.starts_with(&cache_path) && dir != cache_path {
                    cache_dirs.insert(dir.clone());
                    if let Some(p) = dir.parent() {
                        dir = p.to_owned();
                    } else {
                        break;
                    }
                }
            }
            let wrote = write_if_changed(&full_path, file.tsx_content.as_bytes(), "write tsx")?;
            if wrote {
                stats.cache.tsx_written += 1;
            } else {
                stats.cache.tsx_skipped += 1;
            }
            cache_files.insert(full_path.clone());

            if let Some(file_name) = full_path.file_name() {
                let stub_path = full_path.with_extension("d.ts");
                let stub_content = format!(
                    "export * from \"./{}\";\nexport {{ default }} from \"./{}\";\n",
                    file_name, file_name
                );
                let wrote = write_if_changed(&stub_path, stub_content.as_bytes(), "write stub")?;
                if wrote {
                    stats.cache.stub_written += 1;
                } else {
                    stats.cache.stub_skipped += 1;
                }
                cache_files.insert(stub_path);
            }
            tsx_files.push(full_path.to_string());
        }

        // Clean up stale cache files (from deleted .svelte files)
        // Note: we only clean .svelte.ts and .svelte.d.ts here.
        // Other source files are cleaned in symlink_source_tree_with_cleanup.
        for entry in WalkDir::new(&cache_path).follow_links(false).into_iter() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let path = match Utf8Path::from_path(entry.path()) {
                Some(path) => path,
                None => continue,
            };
            // Only clean up .svelte.ts and .svelte.d.ts files (transformed files)
            if entry.file_type().is_file() {
                let path_str = path.as_str();
                if (path_str.ends_with(".svelte.ts") || path_str.ends_with(".svelte.d.ts"))
                    && !cache_files.contains(path)
                {
                    let _ = std::fs::remove_file(path);
                    stats.cache.stale_removed += 1;
                }
            }
        }

        stats.timings.write_time = write_start.elapsed();

        // Symlink key directories/files from project to temp directory for module resolution
        // Note: Don't symlink 'src' since we write transformed files there
        let symlinks = [
            ("node_modules", self.project_root.join("node_modules")),
            ("tests", self.project_root.join("tests")),
            ("workflows", self.project_root.join("workflows")),
            ("vite.config.js", self.project_root.join("vite.config.js")),
            ("vite.config.ts", self.project_root.join("vite.config.ts")),
        ];

        for (name, source) in symlinks {
            let target = temp_path.join(name);
            if source.exists() && !target.exists() {
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&source, &target)
                        .map_err(|e| TsgoError::TempFileFailed(format!("symlink {name}: {e}")))?;
                }
                #[cfg(windows)]
                {
                    if source.is_dir() {
                        std::os::windows::fs::symlink_dir(&source, &target).map_err(|e| {
                            TsgoError::TempFileFailed(format!("symlink {name}: {e}"))
                        })?;
                    } else {
                        std::os::windows::fs::symlink_file(&source, &target).map_err(|e| {
                            TsgoError::TempFileFailed(format!("symlink {name}: {e}"))
                        })?;
                    }
                }
            }
        }

        // Keep a stable copy of .svelte-kit to avoid invalidating tsgo incremental builds.
        // When caching is enabled, write directly into the cache root to avoid duplicate copies.
        let kit_source = self.project_root.join(".svelte-kit");
        if kit_source.exists() {
            let target = temp_path.join(".svelte-kit");
            if self.use_sveltekit_cache {
                // Clean up legacy cache location if present.
                let legacy_cache = self.project_root.join(".svelte-check-rs/kit");
                if legacy_cache.exists() {
                    let _ = std::fs::remove_dir_all(&legacy_cache);
                }
                if let Ok(meta) = std::fs::symlink_metadata(&target) {
                    if meta.file_type().is_symlink() || meta.is_file() {
                        let _ = std::fs::remove_file(&target);
                    } else if meta.is_dir() {
                        // Keep existing directory; sync will update contents.
                    } else {
                        let _ = std::fs::remove_file(&target);
                    }
                }
                sync_sveltekit_cache(&kit_source, &target)?;
            } else {
                let mut needs_link = true;
                if let Ok(meta) = std::fs::symlink_metadata(&target) {
                    if meta.file_type().is_symlink() {
                        if let Ok(existing) = std::fs::read_link(&target) {
                            if existing == kit_source.as_std_path() {
                                needs_link = false;
                            }
                        }
                    }
                    if needs_link {
                        if meta.is_dir() {
                            let _ = std::fs::remove_dir_all(&target);
                        } else {
                            let _ = std::fs::remove_file(&target);
                        }
                    }
                }
                if needs_link {
                    #[cfg(unix)]
                    {
                        std::os::unix::fs::symlink(&kit_source, &target).map_err(|e| {
                            TsgoError::TempFileFailed(format!("symlink .svelte-kit: {e}"))
                        })?;
                    }
                    #[cfg(windows)]
                    {
                        std::os::windows::fs::symlink_dir(&kit_source, &target).map_err(|e| {
                            TsgoError::TempFileFailed(format!("symlink .svelte-kit: {e}"))
                        })?;
                    }
                }
            }
        }

        let project_src = self.project_root.join("src");

        // Symlink the entire source tree for proper module resolution
        // This preserves directory structure so relative imports like `./schema` work
        // and SvelteKit route files (+page.ts, etc.) can access ./$types
        let temp_src = temp_path.join("src");
        let source_start = Instant::now();
        Self::symlink_source_tree(
            &self.project_root,
            &project_src,
            &temp_src,
            apply_tsgo_fixes,
            &mut stats.cache,
        )?;
        stats.timings.source_tree_time = source_start.elapsed();

        // Add a local shim for tsgo-only helpers used in patched sources.
        // Placing this under src keeps it within typical tsconfig include globs.
        let shim_path = temp_src.join("__svelte_check_rs_shims.d.ts");
        let shim_content =
            "declare function __svelte_empty_array<T>(value: () => T): Awaited<T>;\n";
        let wrote = write_if_changed(&shim_path, shim_content.as_bytes(), "write shim")?;
        if wrote {
            stats.cache.shim_written += 1;
        } else {
            stats.cache.shim_skipped += 1;
        }

        let helpers_path = temp_path.join(SHARED_HELPERS_FILENAME);
        let _ = write_if_changed(
            &helpers_path,
            SHARED_HELPERS_DTS.as_bytes(),
            "write helpers",
        )?;

        // Use the existing tsconfig via symlink overlay
        let project_tsconfig = self.resolve_tsconfig_path()?;
        let tsconfig_start = Instant::now();
        let temp_tsconfig = self.prepare_tsconfig_overlay(
            temp_path,
            &project_tsconfig,
            &mut stats.cache,
            Some(&tsconfig_overrides),
        )?;
        stats.timings.tsconfig_time = tsconfig_start.elapsed();

        // Run tsgo on the temp directory
        let tsgo_start = Instant::now();
        let mut command = Command::new(&self.tsgo_path);
        command.arg("--project").arg(&temp_tsconfig);
        if emit_diagnostics {
            command.arg("--diagnostics").arg("--extendedDiagnostics");
        }
        let output = command
            .current_dir(temp_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        stats.timings.tsgo_time = tsgo_start.elapsed();

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // tsgo may exit with non-zero on type errors, which is expected
        if !output.status.success() && stderr.contains("error") && !stdout.contains(":") {
            return Err(TsgoError::ProcessFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            });
        }

        // Parse diagnostics from output
        let parse_start = Instant::now();
        let diagnostics = parse_tsgo_output(&stdout, files)?;
        stats.timings.parse_time = parse_start.elapsed();

        if emit_diagnostics {
            stats.diagnostics = extract_tsgo_diagnostics(&stdout);
        }
        stats.timings.total_time = total_start.elapsed();

        Ok(TsgoCheckOutput { diagnostics, stats })
    }
}

fn read_env_bool(name: &str) -> Option<bool> {
    let value = std::env::var(name).ok()?;
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn write_if_changed(path: &Utf8Path, contents: &[u8], context: &str) -> Result<bool, TsgoError> {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() == contents.len() as u64 {
            if let Ok(existing) = std::fs::read(path) {
                if existing == contents {
                    return Ok(false);
                }
            }
        }
    }

    std::fs::write(path, contents)
        .map_err(|e| TsgoError::TempFileFailed(format!("{context}: {e}")))?;
    Ok(true)
}

fn read_signature(path: &Utf8Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn write_signature(path: &Utf8Path, signature: &str) -> Result<(), TsgoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| TsgoError::TempFileFailed(format!("create sync dir: {e}")))?;
    }
    let _ = write_if_changed(path, signature.as_bytes(), "write kit sync signature")?;
    Ok(())
}

fn sync_sveltekit_cache(source: &Utf8Path, target: &Utf8Path) -> Result<(), TsgoError> {
    if !source.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(target)
        .map_err(|e| TsgoError::TempFileFailed(format!("create kit cache dir: {e}")))?;

    let mut seen_files: HashSet<Utf8PathBuf> = HashSet::new();
    let mut seen_dirs: HashSet<Utf8PathBuf> = HashSet::new();
    seen_dirs.insert(target.to_owned());

    for entry in WalkDir::new(source).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = match Utf8Path::from_path(entry.path()) {
            Some(path) => path,
            None => continue,
        };
        let relative = match path.strip_prefix(source) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        let dest = target.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)
                .map_err(|e| TsgoError::TempFileFailed(format!("kit cache dir: {e}")))?;
            seen_dirs.insert(dest);
            continue;
        }

        if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TsgoError::TempFileFailed(format!("kit cache dir: {e}")))?;
                seen_dirs.insert(parent.to_owned());
            }
            let contents = std::fs::read(path)
                .map_err(|e| TsgoError::TempFileFailed(format!("read kit file: {e}")))?;
            let _ = write_if_changed(&dest, &contents, "write kit cache")?;
            seen_files.insert(dest);
        }
    }

    // Remove stale files and empty directories
    for entry in WalkDir::new(target).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = match Utf8Path::from_path(entry.path()) {
            Some(path) => path,
            None => continue,
        };
        if entry.file_type().is_file() && !seen_files.contains(path) {
            let _ = std::fs::remove_file(path);
        }
    }
    for entry in WalkDir::new(target)
        .follow_links(false)
        .contents_first(true)
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = match Utf8Path::from_path(entry.path()) {
            Some(path) => path,
            None => continue,
        };
        if entry.file_type().is_dir() && !seen_dirs.contains(path) {
            let _ = std::fs::remove_dir(path);
        }
    }

    Ok(())
}

fn extract_tsgo_diagnostics(stdout: &str) -> Option<String> {
    let mut collected = Vec::new();
    let mut found = false;
    for line in stdout.lines() {
        if !found {
            if line.starts_with("Files:") {
                found = true;
            } else {
                continue;
            }
        }
        collected.push(line);
    }
    if found {
        Some(collected.join("\n"))
    } else {
        None
    }
}

fn is_newer_than(path: &Utf8Path, baseline: SystemTime) -> bool {
    if !path.exists() {
        return false;
    }

    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(modified) => modified > baseline,
        Err(_) => true,
    }
}

fn src_hooks_changed(src_dir: &Utf8Path, baseline: SystemTime) -> bool {
    let entries = match std::fs::read_dir(src_dir.as_std_path()) {
        Ok(entries) => entries,
        Err(_) => return true,
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return true,
        };
        let path_buf = entry.path();
        let path = match Utf8Path::from_path(&path_buf) {
            Some(path) => path,
            None => continue,
        };
        if let Some(file_name) = path.file_name() {
            if file_name.starts_with("hooks.") && is_newer_than(path, baseline) {
                return true;
            }
        }
    }

    false
}

fn routes_changed(routes_dir: &Utf8Path, baseline: SystemTime) -> bool {
    for entry in WalkDir::new(routes_dir).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return true,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = match Utf8Path::from_path(entry.path()) {
            Some(path) => path,
            None => continue,
        };
        let file_name = match path.file_name() {
            Some(file_name) => file_name,
            None => continue,
        };
        if is_route_sync_file(file_name) && is_newer_than(path, baseline) {
            return true;
        }
    }

    false
}

#[derive(Clone, Copy)]
enum RouteSignatureKind {
    Script,
    Svelte,
}

fn route_signature_kind(path: &Utf8Path) -> Option<RouteSignatureKind> {
    let file_name = path.file_name()?;
    let ext = path.extension()?;
    let base = route_base_name(file_name)?;

    match ext {
        "svelte" => match base {
            "+page" | "+layout" | "+error" => Some(RouteSignatureKind::Svelte),
            _ => None,
        },
        "ts" | "js" => match base {
            "+page" | "+layout" | "+page.server" | "+layout.server" | "+server" => {
                Some(RouteSignatureKind::Script)
            }
            _ => None,
        },
        _ => None,
    }
}

fn route_base_name(file_name: &str) -> Option<&str> {
    let (stem, _ext) = file_name.rsplit_once('.')?;
    if let Some((base, _)) = stem.split_once('@') {
        Some(base)
    } else {
        Some(stem)
    }
}

fn extract_relevant_exports(
    path: &Utf8Path,
    source: &str,
    is_relevant: fn(&str) -> bool,
) -> Option<Vec<String>> {
    let is_ts = matches!(path.extension(), Some("ts" | "mts" | "cts"));
    let module = parse_module_for_exports(path, source, is_ts)?;
    let mut exports = collect_export_names(&module);
    exports.retain(|name| is_relevant(name));
    exports.sort();
    exports.dedup();
    Some(exports)
}

fn parse_module_for_exports(path: &Utf8Path, source: &str, is_ts: bool) -> Option<Module> {
    let cm: SwcSourceMap = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom(path.to_string()).into(),
        source.to_string(),
    );
    let syntax = if is_ts {
        Syntax::Typescript(TsSyntax {
            tsx: false,
            ..Default::default()
        })
    } else {
        Syntax::Es(EsSyntax {
            jsx: false,
            ..Default::default()
        })
    };
    let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
    parser.parse_module().ok()
}

fn collect_export_names(module: &Module) -> Vec<String> {
    let mut names = Vec::new();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl, .. })) = item else {
            continue;
        };
        match decl {
            Decl::Fn(FnDecl { ident, .. }) => names.push(ident.sym.to_string()),
            Decl::Var(var) => {
                for decl in &var.decls {
                    if let Pat::Ident(ident) = &decl.name {
                        names.push(ident.id.sym.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

fn is_relevant_route_export(name: &str) -> bool {
    matches!(
        name,
        "load"
            | "actions"
            | "entries"
            | "prerender"
            | "trailingSlash"
            | "ssr"
            | "csr"
            | "GET"
            | "PUT"
            | "POST"
            | "PATCH"
            | "DELETE"
            | "OPTIONS"
            | "HEAD"
            | "fallback"
    )
}

fn is_relevant_hook_export(name: &str) -> bool {
    matches!(name, "handle" | "handleFetch" | "handleError" | "reroute")
}

fn is_route_sync_file(file_name: &str) -> bool {
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((stem, ext)) => (stem, ext),
        None => return false,
    };
    if !matches!(ext, "ts" | "js") {
        return false;
    }
    let base = if let Some((base, _)) = stem.split_once('@') {
        base
    } else {
        stem
    };
    matches!(
        base,
        "+page" | "+page.server" | "+layout" | "+layout.server" | "+server"
    )
}

fn dir_has_newer_mtime(dir: &Utf8Path, baseline: SystemTime) -> bool {
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return true,
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => return true,
        };
        match metadata.modified() {
            Ok(modified) => {
                if modified > baseline {
                    return true;
                }
            }
            Err(_) => return true,
        }
    }

    false
}

fn is_ts_like_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    )
}

/// Work around tsgo's inference bug for conditional empty arrays inside Promise.all.
/// This rewrites `? [] :` / `: []` branches to `([] as Awaited<typeof (<other branch>)>)`
/// inside Promise.all calls so the empty array branch inherits the sibling type.
fn patch_promise_all_empty_arrays(input: &str) -> Option<String> {
    if !input.contains("Promise.all") {
        return None;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Depth {
        paren: i32,
        bracket: i32,
        brace: i32,
    }

    #[derive(Clone, Copy, Debug)]
    struct TernaryPair {
        question: usize,
        colon: usize,
        depth: Depth,
    }

    fn is_ternary_question(bytes: &[u8], i: usize) -> bool {
        if bytes[i] != b'?' {
            return false;
        }
        let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
        let next = bytes.get(i + 1).copied();
        if prev == Some(b'?') || next == Some(b'?') || next == Some(b'.') {
            return false;
        }
        true
    }

    fn scan_ternary_pairs(
        input: &str,
    ) -> (
        Vec<TernaryPair>,
        std::collections::HashMap<usize, usize>,
        std::collections::HashMap<usize, usize>,
    ) {
        let bytes = input.as_bytes();
        let mut pairs = Vec::new();
        let mut question_map = std::collections::HashMap::new();
        let mut colon_map = std::collections::HashMap::new();
        let mut stack: Vec<(usize, Depth)> = Vec::new();
        let mut depth = Depth {
            paren: 0,
            bracket: 0,
            brace: 0,
        };
        let mut i = 0usize;
        let mut in_string: Option<u8> = None;
        let mut in_line_comment = false;
        let mut in_block_comment = false;

        while i < bytes.len() {
            let b = bytes[i];

            if in_line_comment {
                if b == b'\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }

            if in_block_comment {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    i += 2;
                    in_block_comment = false;
                    continue;
                }
                i += 1;
                continue;
            }

            if let Some(quote) = in_string {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == quote {
                    in_string = None;
                }
                i += 1;
                continue;
            }

            if b == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
            }

            if b == b'\'' || b == b'"' || b == b'`' {
                in_string = Some(b);
                i += 1;
                continue;
            }

            match b {
                b'(' => depth.paren += 1,
                b')' => depth.paren = depth.paren.saturating_sub(1),
                b'[' => depth.bracket += 1,
                b']' => depth.bracket = depth.bracket.saturating_sub(1),
                b'{' => depth.brace += 1,
                b'}' => depth.brace = depth.brace.saturating_sub(1),
                b'?' if is_ternary_question(bytes, i) => {
                    stack.push((i, depth));
                }
                b':' => {
                    if let Some(&(q_pos, q_depth)) = stack.last() {
                        if depth == q_depth {
                            stack.pop();
                            let pair = TernaryPair {
                                question: q_pos,
                                colon: i,
                                depth,
                            };
                            let idx = pairs.len();
                            pairs.push(pair);
                            question_map.insert(q_pos, idx);
                            colon_map.insert(i, idx);
                        }
                    }
                }
                _ => {}
            }

            i += 1;
        }

        (pairs, question_map, colon_map)
    }

    fn trim_span(input: &str, start: usize, end: usize) -> Option<&str> {
        if start >= end || end > input.len() {
            return None;
        }
        let mut s = start;
        let mut e = end;
        let bytes = input.as_bytes();
        while s < e && bytes[s].is_ascii_whitespace() {
            s += 1;
        }
        while e > s && bytes[e - 1].is_ascii_whitespace() {
            e -= 1;
        }
        if s >= e {
            None
        } else {
            Some(&input[s..e])
        }
    }

    fn scan_false_branch_end(input: &str, start: usize, base_depth: Depth) -> usize {
        let bytes = input.as_bytes();
        let mut i = start;
        let mut depth = base_depth;
        let mut in_string: Option<u8> = None;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let mut ternary_depth = 0usize;

        while i < bytes.len() {
            let b = bytes[i];

            if in_line_comment {
                if b == b'\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }

            if in_block_comment {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    i += 2;
                    in_block_comment = false;
                    continue;
                }
                i += 1;
                continue;
            }

            if let Some(quote) = in_string {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == quote {
                    in_string = None;
                }
                i += 1;
                continue;
            }

            if b == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
            }

            if b == b'\'' || b == b'"' || b == b'`' {
                in_string = Some(b);
                i += 1;
                continue;
            }

            if b == b'?' && is_ternary_question(bytes, i) {
                ternary_depth += 1;
                i += 1;
                continue;
            }

            if b == b':' && ternary_depth > 0 {
                ternary_depth -= 1;
                i += 1;
                continue;
            }

            if ternary_depth == 0
                && depth == base_depth
                && matches!(b, b',' | b')' | b']' | b'}' | b';')
            {
                break;
            }

            match b {
                b'(' => depth.paren += 1,
                b')' => depth.paren = depth.paren.saturating_sub(1),
                b'[' => depth.bracket += 1,
                b']' => depth.bracket = depth.bracket.saturating_sub(1),
                b'{' => depth.brace += 1,
                b'}' => depth.brace = depth.brace.saturating_sub(1),
                _ => {}
            }

            i += 1;
        }

        i
    }

    let (pairs, question_map, colon_map) = scan_ternary_pairs(input);

    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut in_string: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut depth = Depth {
        paren: 0,
        bracket: 0,
        brace: 0,
    };
    struct PromiseAllContext {
        paren_depth: i32,
        array_bracket_depth: Option<i32>,
        base_brace: i32,
    }
    let mut promise_all_stack: Vec<PromiseAllContext> = Vec::new();
    let mut pending_promise_all = false;
    let mut last_non_ws: Option<u8> = None;
    let mut last_non_ws_idx: Option<usize> = None;
    let mut changed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if in_line_comment {
            out.push(b as char);
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            out.push(b as char);
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                out.push('/');
                i += 2;
                in_block_comment = false;
                continue;
            }
            i += 1;
            continue;
        }

        if let Some(quote) = in_string {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if b == quote {
                in_string = None;
            }
            i += 1;
            continue;
        }

        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                out.push('/');
                out.push('/');
                i += 2;
                in_line_comment = true;
                continue;
            }
            if bytes[i + 1] == b'*' {
                out.push('/');
                out.push('*');
                i += 2;
                in_block_comment = true;
                continue;
            }
        }

        if b == b'\'' || b == b'"' || b == b'`' {
            out.push(b as char);
            in_string = Some(b);
            i += 1;
            continue;
        }

        if bytes[i..].starts_with(b"Promise.all") {
            let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
            let next = bytes.get(i + "Promise.all".len()).copied();
            let prev_ok = prev.map_or(true, |c| !is_ident_char(c));
            let next_ok = next.map_or(true, |c| !is_ident_char(c));
            if prev_ok && next_ok {
                out.push_str("Promise.all");
                i += "Promise.all".len();
                pending_promise_all = true;
                last_non_ws = Some(b'l');
                last_non_ws_idx = Some(i.saturating_sub(1));
                continue;
            }
        }

        if b == b'(' {
            depth.paren += 1;
            out.push('(');
            if pending_promise_all {
                promise_all_stack.push(PromiseAllContext {
                    paren_depth: depth.paren,
                    array_bracket_depth: None,
                    base_brace: depth.brace,
                });
                pending_promise_all = false;
            }
            last_non_ws = Some(b'(');
            last_non_ws_idx = Some(i);
            i += 1;
            continue;
        }

        if b == b')' {
            out.push(')');
            if promise_all_stack
                .last()
                .is_some_and(|ctx| ctx.paren_depth == depth.paren)
            {
                promise_all_stack.pop();
            }
            depth.paren = depth.paren.saturating_sub(1);
            last_non_ws = Some(b')');
            last_non_ws_idx = Some(i);
            i += 1;
            continue;
        }

        if pending_promise_all && !b.is_ascii_whitespace() {
            pending_promise_all = false;
        }

        if b == b'[' && !promise_all_stack.is_empty() {
            if let Some(ctx) = promise_all_stack.last_mut() {
                if ctx.array_bracket_depth.is_none()
                    && depth.paren == ctx.paren_depth
                    && depth.brace == ctx.base_brace
                {
                    ctx.array_bracket_depth = Some(depth.bracket + 1);
                }
            }
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                let can_patch = promise_all_stack.last().is_some_and(|ctx| {
                    ctx.array_bracket_depth == Some(depth.bracket) && depth.brace == ctx.base_brace
                });
                if can_patch && matches!(last_non_ws, Some(b'?' | b':')) {
                    let mut other_expr = None;
                    if let Some(last_idx) = last_non_ws_idx {
                        if last_non_ws == Some(b'?') {
                            if let Some(&pair_idx) = question_map.get(&last_idx) {
                                let pair = pairs[pair_idx];
                                let start = pair.colon + 1;
                                let end = scan_false_branch_end(input, start, pair.depth);
                                if let Some(expr) = trim_span(input, start, end) {
                                    if expr != "[]" {
                                        other_expr = Some(expr.to_string());
                                    }
                                }
                            }
                        } else if let Some(&pair_idx) = colon_map.get(&last_idx) {
                            let pair = pairs[pair_idx];
                            if let Some(expr) = trim_span(input, pair.question + 1, pair.colon) {
                                if expr != "[]" {
                                    other_expr = Some(expr.to_string());
                                }
                            }
                        }
                    }
                    if let Some(expr) = other_expr {
                        out.push_str("__svelte_empty_array(() => (");
                        out.push_str(&expr);
                        out.push_str("))");
                    } else {
                        out.push_str("([] as any[])");
                    }
                    changed = true;
                    i = j + 1;
                    last_non_ws = Some(b')');
                    last_non_ws_idx = Some(j);
                    continue;
                }
            }
        }

        if b == b'[' {
            depth.bracket += 1;
        } else if b == b']' {
            depth.bracket = depth.bracket.saturating_sub(1);
            if let Some(ctx) = promise_all_stack.last_mut() {
                if let Some(array_depth) = ctx.array_bracket_depth {
                    if depth.bracket < array_depth {
                        ctx.array_bracket_depth = None;
                    }
                }
            }
        } else if b == b'{' {
            depth.brace += 1;
        } else if b == b'}' {
            depth.brace = depth.brace.saturating_sub(1);
        }

        out.push(b as char);
        if !b.is_ascii_whitespace() {
            last_non_ws = Some(b);
            last_non_ws_idx = Some(i);
        }
        i += 1;
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// A transformed file ready for type-checking.
#[derive(Debug, Clone)]
pub struct TransformedFile {
    /// The original Svelte file path.
    pub original_path: Utf8PathBuf,
    /// The generated TSX content.
    pub tsx_content: String,
    /// The source map for position mapping.
    pub source_map: SourceMap,
    /// Line index for the original source (for position mapping).
    pub original_line_index: source_map::LineIndex,
}

/// A collection of transformed files.
#[derive(Debug, Clone, Default)]
pub struct TransformedFiles {
    /// Map of virtual path to transformed file.
    pub files: HashMap<Utf8PathBuf, TransformedFile>,
}

impl TransformedFiles {
    /// Creates a new empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a transformed file.
    pub fn add(&mut self, virtual_path: Utf8PathBuf, file: TransformedFile) {
        self.files.insert(virtual_path, file);
    }

    /// Gets a transformed file by its virtual path.
    pub fn get(&self, virtual_path: &Utf8Path) -> Option<&TransformedFile> {
        self.files.get(virtual_path)
    }

    /// Finds a file by its original path.
    pub fn find_by_original(&self, original_path: &Utf8Path) -> Option<&TransformedFile> {
        self.files
            .values()
            .find(|f| f.original_path == original_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transformed_files() {
        use source_map::LineIndex;

        let original_source = "<script>let x = 1;</script>";
        let mut files = TransformedFiles::new();
        files.add(
            Utf8PathBuf::from("src/App.svelte.ts"),
            TransformedFile {
                original_path: Utf8PathBuf::from("src/App.svelte"),
                tsx_content: "// generated".to_string(),
                source_map: SourceMap::new(),
                original_line_index: LineIndex::new(original_source),
            },
        );

        assert!(files.get(Utf8Path::new("src/App.svelte.ts")).is_some());
        assert!(files
            .find_by_original(Utf8Path::new("src/App.svelte"))
            .is_some());
    }

    #[test]
    fn test_patch_promise_all_empty_arrays() {
        let input = "const [a] = await Promise.all([cond ? foo() : []]);";
        let patched = patch_promise_all_empty_arrays(input).unwrap();
        assert!(
            patched.contains("cond ? foo() : __svelte_empty_array(() => (foo()))"),
            "patched: {patched}"
        );
    }

    #[test]
    fn test_patch_promise_all_empty_arrays_other_branch() {
        let input = "const [a] = await Promise.all([cond ? [] : foo()]);";
        let patched = patch_promise_all_empty_arrays(input).unwrap();
        assert!(
            patched.contains("cond ? __svelte_empty_array(() => (foo())) : foo()"),
            "patched: {patched}"
        );
    }

    #[test]
    fn test_patch_promise_all_empty_arrays_ignores_other_calls() {
        let input = "const value = cond ? [] : foo();";
        assert!(patch_promise_all_empty_arrays(input).is_none());
    }
}
