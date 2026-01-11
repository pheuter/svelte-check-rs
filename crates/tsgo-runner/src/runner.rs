//! tsgo process runner.

use crate::kit;
use crate::parser::{parse_tsgo_output, TsgoDiagnostic};
use blake3::Hasher;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use source_map::SourceMap;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::Builder;
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

  // Svelte 5 rune type declarations
  // These match the ambient declarations in svelte/types/index.d.ts
  declare function $state<T>(initial: T): T;
  declare function $state<T>(): T | undefined;
  declare namespace $state {
    export function raw<T>(initial: T): T;
    export function raw<T>(): T | undefined;
    export function snapshot<T>(value: T): T;
  }

  declare function $derived<T>(expression: T): T;
  declare namespace $derived {
    export function by<T>(fn: () => T): T;
  }

  declare function $effect(fn: () => void | (() => void)): void;
  declare namespace $effect {
    export function pre(fn: () => void | (() => void)): void;
    export function root(fn: () => (() => void)): () => void;
    export function tracking(): boolean;
  }

  declare function $props<T>(): T;
  declare function $bindable<T>(fallback?: T): T;
  declare function $inspect<T>(...values: T[]): { with: (fn: (type: 'init' | 'update', ...values: T[]) => void) => void };
  declare function $host<T extends HTMLElement = HTMLElement>(): T;

  type __StoreValue<S> = S extends { subscribe(fn: (value: infer T) => void): any } ? T : never;

  type __SvelteOptionalProps<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

  type __SvelteLoosen<T> =
    T extends (...args: any) => any ? T :
    T extends readonly any[] ? T :
    T extends object ? T & Record<string, unknown> : T;

  type __SveltePropsAccessor<T> = { [K in keyof T]: () => T[K] } & Record<string, () => any>;

  declare const __svelte_snippet_return: ReturnType<SvelteSnippet<[]>>;

  type __SvelteEvent<Target extends EventTarget, E extends Event> = E & {
    currentTarget: Target;
    target: Target;
  };

  // Backwards-compatibility namespace for libraries augmenting `svelteHTML`.
  namespace svelteHTML {
    interface HTMLAttributes<T extends EventTarget = any> {}
  }

  type __SvelteIntrinsicElements = SvelteHTMLElements;
  type __SvelteHTMLAttributesCompat<T extends EventTarget> =
    SvelteHTMLAttributes<T> & svelteHTML.HTMLAttributes<T>;
  type __SvelteEventProps<T> =
    T & { [K in keyof T as K extends `on:${infer E}` ? `on${E}` : never]?: T[K] };
  type __SvelteElementAttributes<K extends string> =
    __SvelteEventProps<
      K extends keyof __SvelteIntrinsicElements
        ? __SvelteIntrinsicElements[K] & svelteHTML.HTMLAttributes<any>
        : __SvelteHTMLAttributesCompat<any>
    >;
  type __SvelteEventHandler<K extends string, E extends string, A = {}> =
    A extends { [key in `on:${E}`]?: infer H }
      ? H | undefined
      : A extends { [key in `on${E}`]?: infer H }
        ? H | undefined
        : __SvelteElementAttributes<K> extends { [key in `on:${E}`]?: infer H }
          ? H | undefined
          : __SvelteElementAttributes<K> extends { [key in `on${E}`]?: infer H }
            ? H | undefined
            : ((e: Event) => void) | null | undefined;

  type __SvelteActionReturnType = {
    update?: (parameter: any) => void;
    destroy?: () => void;
    $$_attributes?: Record<string, any>;
  } | void;
  declare function __svelte_ensure_action<T extends __SvelteActionReturnType>(
    actionCall: T
  ): T extends { $$_attributes?: any } ? T["$$_attributes"] : {};

  type __SvelteUnionToIntersection<U> =
    (U extends any ? (k: U) => void : never) extends (k: infer I) => void ? I : never;
  declare function __svelte_union<T extends any[]>(...args: T): __SvelteUnionToIntersection<T[number]>;

  declare function __svelte_create_element<K extends string, T>(
    tag: K | undefined | null,
    actionAttrs: T,
    attrs: __SvelteElementAttributes<K> & T
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

    /// node_modules not found when resolving the cache directory.
    #[error(
        "node_modules not found starting at {0} (searched parent directories). Please run npm/pnpm/yarn/bun install first."
    )]
    NodeModulesNotFound(Utf8PathBuf),

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
    /// Additional path aliases to merge into the tsconfig.
    extra_paths: HashMap<String, Vec<String>>,
    /// Whether to cache .svelte-kit contents in a stable location.
    use_sveltekit_cache: bool,
    /// Whether to use the persistent cache directory and incremental build info.
    use_cache: bool,
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

#[derive(Debug, Default, Clone)]
struct TsconfigSnapshot {
    base_url: Option<String>,
    base_url_base: Option<Utf8PathBuf>,
    root_dirs: Option<Vec<String>>,
    root_dirs_base: Option<Utf8PathBuf>,
    paths: HashMap<String, Vec<String>>,
    paths_base: Option<Utf8PathBuf>,
    allow_js: Option<bool>,
    check_js: Option<bool>,
    include: Option<Vec<String>>,
    include_base: Option<Utf8PathBuf>,
    exclude: Option<Vec<String>>,
    exclude_base: Option<Utf8PathBuf>,
    files: Option<Vec<String>>,
    files_base: Option<Utf8PathBuf>,
}

struct TsconfigOverlayOptions<'a> {
    temp_root: &'a Utf8Path,
    tsconfig_path: &'a Utf8Path,
    overrides: Option<&'a Map<String, Value>>,
    kit_include: Option<&'a Utf8Path>,
    patched_sources: &'a [Utf8PathBuf],
    extra_files: &'a [Utf8PathBuf],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SvelteKitFileStamp {
    hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct SvelteKitSyncManifest {
    files: BTreeMap<String, SvelteKitFileStamp>,
}

impl TsgoRunner {
    /// Creates a new tsgo runner.
    pub fn new(
        tsgo_path: Utf8PathBuf,
        project_root: Utf8PathBuf,
        tsconfig_path: Option<Utf8PathBuf>,
        extra_paths: HashMap<String, Vec<String>>,
        use_sveltekit_cache: bool,
        use_cache: bool,
    ) -> Self {
        Self {
            tsgo_path,
            project_root,
            tsconfig_path,
            extra_paths,
            use_sveltekit_cache,
            use_cache,
        }
    }

    fn find_node_modules_dir(project_root: &Utf8Path) -> Option<Utf8PathBuf> {
        let mut current = Some(project_root);

        while let Some(dir) = current {
            let candidate = dir.join("node_modules");
            if candidate.is_dir() {
                return Some(candidate);
            }
            current = dir.parent();
        }

        None
    }

    fn project_cache_root_for(project_root: &Utf8Path) -> Result<Utf8PathBuf, TsgoError> {
        let node_modules = Self::find_node_modules_dir(project_root)
            .ok_or_else(|| TsgoError::NodeModulesNotFound(project_root.to_owned()))?;
        let cache_root = node_modules.join(".cache/svelte-check-rs");

        let legacy_cache = project_root.join(".svelte-check-rs");
        if legacy_cache.exists() {
            eprintln!("Migrating cache: removing legacy {}", legacy_cache.as_str());
            let _ = std::fs::remove_dir_all(&legacy_cache);
        }

        std::fs::create_dir_all(&cache_root)
            .map_err(|e| TsgoError::TempFileFailed(format!("create cache dir: {e}")))?;

        Ok(cache_root)
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
    pub async fn ensure_sveltekit_sync(
        project_root: &Utf8Path,
        use_cache: bool,
    ) -> Result<bool, TsgoError> {
        // Check if this is a SvelteKit project by searching for @sveltejs/kit
        // in node_modules, including parent directories for monorepo support
        if !Self::is_sveltekit_project(project_root) {
            return Ok(false);
        }

        if !use_cache {
            return Self::run_sveltekit_sync(project_root, None).await;
        }

        let cache_root = Self::project_cache_root_for(project_root)?;
        let state_path = cache_root.join("kit-sync.manifest.json");
        let previous = read_sync_manifest(&state_path);
        let manifest = match Self::compute_sveltekit_sync_manifest(project_root) {
            Ok(manifest) => manifest,
            Err(err) => {
                eprintln!("Warning: failed to compute svelte-kit sync manifest: {err}");
                // Fall back to running sync to avoid stale types.
                return Self::run_sveltekit_sync(project_root, None).await;
            }
        };

        if let Some(previous) = &previous {
            if previous == &manifest {
                return Ok(false);
            }
        }

        Self::run_sveltekit_sync(project_root, Some((&state_path, &manifest))).await
    }

    async fn run_sveltekit_sync(
        project_root: &Utf8Path,
        manifest: Option<(&Utf8Path, &SvelteKitSyncManifest)>,
    ) -> Result<bool, TsgoError> {
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

        if let Some((path, manifest)) = manifest {
            write_sync_manifest(path, manifest)?;
        }

        Ok(true)
    }

    fn compute_sveltekit_sync_manifest(
        project_root: &Utf8Path,
    ) -> Result<SvelteKitSyncManifest, TsgoError> {
        let mut files = BTreeMap::new();

        for path in Self::collect_sveltekit_sync_files(project_root) {
            let rel = path.strip_prefix(project_root).unwrap_or(&path).to_string();
            let stamp = file_hash(&path)?;
            files.insert(rel, stamp);
        }

        Ok(SvelteKitSyncManifest { files })
    }

    fn collect_sveltekit_sync_files(project_root: &Utf8Path) -> Vec<Utf8PathBuf> {
        let mut files = Vec::new();

        let config_candidates = [
            "svelte.config.js",
            "svelte.config.cjs",
            "svelte.config.mjs",
            "svelte.config.ts",
        ];
        for name in config_candidates {
            let path = project_root.join(name);
            if path.exists() {
                files.push(path);
            }
        }

        let hooks_dir = project_root.join("src");
        if hooks_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(hooks_dir.as_std_path()) {
                for entry in entries.flatten() {
                    let path = Utf8Path::from_path(&entry.path()).map(Utf8Path::to_owned);
                    let Some(path) = path else { continue };
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
                    files.push(path);
                }
            }
        }

        let params_dir = project_root.join("src/params");
        if params_dir.exists() {
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
                files.push(path.to_owned());
            }
        }

        let routes_dir = project_root.join("src/routes");
        if routes_dir.exists() {
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
                if route_signature_kind(path).is_some() {
                    files.push(path.to_owned());
                }
            }
        }

        files.sort();
        files.dedup();
        files
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

    fn load_tsconfig_snapshot(&self, tsconfig_path: &Utf8Path) -> TsconfigSnapshot {
        let mut visited = HashSet::new();
        self.load_tsconfig_snapshot_inner(tsconfig_path, &mut visited)
    }

    fn load_tsconfig_snapshot_inner(
        &self,
        tsconfig_path: &Utf8Path,
        visited: &mut HashSet<Utf8PathBuf>,
    ) -> TsconfigSnapshot {
        if !visited.insert(tsconfig_path.to_owned()) {
            return TsconfigSnapshot::default();
        }

        let value = match read_tsconfig_value(tsconfig_path) {
            Some(value) => value,
            None => return TsconfigSnapshot::default(),
        };

        let mut snapshot = if let Some(extends_value) = value.get("extends").and_then(Value::as_str)
        {
            if let Some(parent) = self.resolve_extends_path(tsconfig_path, extends_value) {
                self.load_tsconfig_snapshot_inner(&parent, visited)
            } else {
                TsconfigSnapshot::default()
            }
        } else {
            TsconfigSnapshot::default()
        };

        if let Some(options) = value.get("compilerOptions").and_then(Value::as_object) {
            if let Some(base_url) = options.get("baseUrl").and_then(Value::as_str) {
                snapshot.base_url = Some(base_url.to_string());
                snapshot.base_url_base = tsconfig_path.parent().map(|p| p.to_owned());
            }
            if let Some(root_dirs) = parse_string_array(options.get("rootDirs")) {
                snapshot.root_dirs = Some(root_dirs);
                snapshot.root_dirs_base = tsconfig_path.parent().map(|p| p.to_owned());
            }
            if let Some(paths) = options.get("paths").and_then(Value::as_object) {
                snapshot.paths_base = tsconfig_path.parent().map(|p| p.to_owned());
                for (key, value) in paths {
                    if let Some(entries) = parse_string_array(Some(value)) {
                        if !entries.is_empty() {
                            snapshot.paths.insert(key.clone(), entries);
                        }
                    }
                }
            }
            if let Some(allow_js) = options.get("allowJs").and_then(Value::as_bool) {
                snapshot.allow_js = Some(allow_js);
            }
            if let Some(check_js) = options.get("checkJs").and_then(Value::as_bool) {
                snapshot.check_js = Some(check_js);
            }
        }

        if let Some(include) = parse_string_array(value.get("include")) {
            snapshot.include = Some(include);
            snapshot.include_base = tsconfig_path.parent().map(|p| p.to_owned());
        }
        if let Some(exclude) = parse_string_array(value.get("exclude")) {
            snapshot.exclude = Some(exclude);
            snapshot.exclude_base = tsconfig_path.parent().map(|p| p.to_owned());
        }
        if let Some(files) = parse_string_array(value.get("files")) {
            snapshot.files = Some(files);
            snapshot.files_base = tsconfig_path.parent().map(|p| p.to_owned());
        }

        snapshot
    }

    fn resolve_extends_path(
        &self,
        base_path: &Utf8Path,
        extends_value: &str,
    ) -> Option<Utf8PathBuf> {
        let base_dir = base_path.parent().unwrap_or(&self.project_root);
        let extends_path = Utf8Path::new(extends_value);
        let mut candidates = Vec::new();

        if extends_path.is_absolute() || extends_value.starts_with('.') {
            let resolved = if extends_path.is_absolute() {
                extends_path.to_owned()
            } else {
                base_dir.join(extends_path)
            };
            candidates.push(resolved.clone());
            if resolved.extension().is_none() {
                candidates.push(resolved.with_extension("json"));
            }
        } else if let Some(node_modules) = Self::find_node_modules_dir(&self.project_root) {
            let mut resolved = node_modules.join(extends_value);
            if resolved.is_dir() {
                candidates.push(resolved.join("tsconfig.json"));
            } else {
                candidates.push(resolved.clone());
                if resolved.extension().is_none() {
                    resolved.set_extension("json");
                    candidates.push(resolved);
                }
            }
        }

        candidates.into_iter().find(|path| path.exists())
    }

    /// Prepare a tsconfig overlay inside the cache directory.
    ///
    /// Generates a standalone config with absolute paths and rootDirs so we
    /// can avoid symlinks and only cache patched sources.
    fn prepare_tsconfig_overlay(
        &self,
        options: &TsconfigOverlayOptions<'_>,
        stats: &mut TsgoCacheStats,
    ) -> Result<Utf8PathBuf, TsgoError> {
        let overlay_path = options.temp_root.join("tsconfig.tsgo.json");
        let tsconfig_dir = options.tsconfig_path.parent().unwrap_or(&self.project_root);
        let snapshot = self.load_tsconfig_snapshot(options.tsconfig_path);

        let mut compiler_options = Map::new();
        if let Some(overrides) = options.overrides {
            for (key, value) in overrides {
                compiler_options.insert(key.clone(), value.clone());
            }
        }

        let mut root_dirs: Vec<String> = Vec::new();
        if let Some(root_dirs_raw) = snapshot.root_dirs.as_ref() {
            let base = snapshot.root_dirs_base.as_deref().unwrap_or(tsconfig_dir);
            root_dirs.extend(
                root_dirs_raw
                    .iter()
                    .map(|dir| resolve_path_value(base, dir)),
            );
        }
        let project_root = clean_path(&self.project_root).to_string();
        let temp_root = clean_path(options.temp_root).to_string();
        root_dirs.retain(|dir| dir != &project_root && dir != &temp_root);
        let mut ordered_root_dirs = Vec::new();
        // Prefer cached files over project sources when both exist.
        ordered_root_dirs.push(temp_root);
        ordered_root_dirs.push(project_root);
        ordered_root_dirs.extend(root_dirs);
        root_dirs = ordered_root_dirs;
        if let Some(kit_path) = options.kit_include {
            let types_dir = kit_path.join("types");
            if types_dir.exists() {
                root_dirs.push(types_dir.to_string());
            }
        }

        let project_kit_root = clean_path(&self.project_root.join(".svelte-kit"));
        let use_cached_kit = options
            .kit_include
            .map(|path| clean_path(path) != project_kit_root)
            .unwrap_or(false);
        if use_cached_kit {
            let project_types = project_kit_root.join("types").to_string();
            root_dirs.retain(|dir| dir != &project_types);
        }
        let mut seen = HashSet::new();
        root_dirs.retain(|dir| seen.insert(dir.clone()));
        compiler_options.insert(
            "rootDirs".to_string(),
            Value::Array(root_dirs.into_iter().map(Value::String).collect()),
        );

        let mut merged_paths = snapshot.paths.clone();
        for (alias, values) in &self.extra_paths {
            merged_paths
                .entry(alias.clone())
                .or_insert_with(|| values.clone());
        }

        let base_url_base = snapshot.base_url_base.as_deref().unwrap_or(tsconfig_dir);
        let resolved_base = snapshot
            .base_url
            .as_deref()
            .map(|base| resolve_base_url(base_url_base, base));
        let paths_base = snapshot.paths_base.as_deref().unwrap_or(tsconfig_dir);
        let fallback_base = resolved_base
            .as_ref()
            .map(|path| path.as_path())
            .unwrap_or(paths_base);

        if !merged_paths.is_empty() {
            let mut paths_map = Map::new();
            for (alias, values) in merged_paths {
                let mut resolved_values: Vec<Value> = Vec::new();
                let mut seen = HashSet::new();
                for value in values {
                    let resolved = resolve_path_value(fallback_base, &value);
                    if let Ok(relative) = Utf8Path::new(&resolved).strip_prefix(&self.project_root)
                    {
                        let cached = clean_path(&options.temp_root.join(relative)).to_string();
                        if seen.insert(cached.clone()) {
                            resolved_values.push(Value::String(cached));
                        }
                    }
                    if seen.insert(resolved.clone()) {
                        resolved_values.push(Value::String(resolved));
                    }
                }
                paths_map.insert(alias, Value::Array(resolved_values));
            }
            compiler_options.insert("paths".to_string(), Value::Object(paths_map));
        }

        if let Some(resolved) = resolved_base {
            compiler_options.insert("baseUrl".to_string(), Value::String(resolved.to_string()));
        }

        let mut includes: Vec<String> = Vec::new();
        if let Some(files) = snapshot.files.as_ref() {
            let base = snapshot.files_base.as_deref().unwrap_or(tsconfig_dir);
            includes.extend(
                files
                    .iter()
                    .map(|pattern| absolutize_pattern(base, pattern)),
            );
        }
        if let Some(include) = snapshot.include.as_ref() {
            let base = snapshot.include_base.as_deref().unwrap_or(tsconfig_dir);
            includes.extend(
                include
                    .iter()
                    .map(|pattern| absolutize_pattern(base, pattern)),
            );
        }
        let default_includes = includes.is_empty();
        if default_includes {
            includes.push(absolutize_pattern(tsconfig_dir, "src/**/*"));
            if self.project_root.join("tests").exists() {
                includes.push(absolutize_pattern(tsconfig_dir, "tests/**/*"));
            }
            if self.project_root.join("workflows").exists() {
                includes.push(absolutize_pattern(tsconfig_dir, "workflows/**/*"));
            }
        }

        includes.push(format!("{}/src/**/*.ts", options.temp_root));
        includes.push(format!("{}/src/**/*.d.ts", options.temp_root));

        if let Some(kit_path) = options.kit_include {
            includes.push(kit_path.join("ambient.d.ts").to_string());
            includes.push(kit_path.join("non-ambient.d.ts").to_string());
            includes.push(kit_path.join("types/**/$types.d.ts").to_string());
        }

        if use_cached_kit {
            let kit_prefix = project_kit_root.to_string();
            includes.retain(|path| !path.starts_with(&kit_prefix));
        }

        let exclude_base = snapshot.exclude_base.as_deref().unwrap_or(tsconfig_dir);
        let mut excludes: Vec<String> = snapshot
            .exclude
            .as_ref()
            .map(|patterns| {
                patterns
                    .iter()
                    .map(|pattern| absolutize_pattern(exclude_base, pattern))
                    .collect()
            })
            .unwrap_or_default();

        if use_cached_kit {
            let kit_prefix = project_kit_root.to_string();
            excludes.retain(|path| !path.starts_with(&kit_prefix));
        }

        let clean_project_root = clean_path(&self.project_root);
        excludes.push(format!("{}/**/*.svelte.ts", clean_project_root));
        excludes.push(format!("{}/**/*.svelte.js", clean_project_root));
        for source in options.patched_sources {
            excludes.push(clean_path(source).to_string());
        }

        let mut root = Map::new();
        root.insert(
            "extends".to_string(),
            Value::String(options.tsconfig_path.to_string()),
        );
        root.insert(
            "compilerOptions".to_string(),
            Value::Object(compiler_options),
        );
        let allow_js = snapshot
            .allow_js
            .unwrap_or(snapshot.check_js.unwrap_or(false));
        let extra_files: Vec<&Utf8PathBuf> = if allow_js {
            options.extra_files.iter().collect()
        } else {
            options
                .extra_files
                .iter()
                .filter(|path| !is_js_like_file(path))
                .collect()
        };
        if !extra_files.is_empty() {
            root.insert(
                "files".to_string(),
                Value::Array(
                    extra_files
                        .into_iter()
                        .map(|path| Value::String(path.to_string()))
                        .collect(),
                ),
            );
        }
        root.insert(
            "include".to_string(),
            Value::Array(includes.into_iter().map(Value::String).collect()),
        );
        if !excludes.is_empty() {
            root.insert(
                "exclude".to_string(),
                Value::Array(excludes.into_iter().map(Value::String).collect()),
            );
        }

        let content = serde_json::to_string_pretty(&Value::Object(root))
            .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        let wrote = write_if_changed(&overlay_path, content.as_bytes(), "write tsconfig overlay")?;
        if wrote {
            stats.tsconfig_written += 1;
        } else {
            stats.tsconfig_skipped += 1;
        }

        Ok(overlay_path)
    }

    /// Writes patched source files and SvelteKit source files into the cache.
    /// This avoids mirroring the entire source tree and relies on rootDirs for resolution.
    fn write_source_patches(
        project_root: &Utf8Path,
        project_src: &Utf8Path,
        temp_src: &Utf8Path,
        stats: &mut TsgoCacheStats,
    ) -> Result<Vec<Utf8PathBuf>, TsgoError> {
        // Always create temp_src so shim can be written even if project has no src dir
        std::fs::create_dir_all(temp_src).map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;

        // Track files and directories we write to the cache
        let mut cache_files: HashSet<Utf8PathBuf> = HashSet::new();
        let mut cache_dirs: HashSet<Utf8PathBuf> = HashSet::new();
        cache_dirs.insert(temp_src.to_owned());
        let mut patched_sources: Vec<Utf8PathBuf> = Vec::new();

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

                if entry.file_type().is_dir() {
                    stats.source_dirs += 1;
                    continue;
                }

                stats.source_files += 1;

                // Skip .svelte files - we write transformed .ts versions
                if path.extension() == Some("svelte") {
                    stats.source_svelte_skipped += 1;
                    continue;
                }

                let mut write_contents: Option<String> = None;
                let mut is_kit_file = false;

                if let Some(kind) = kit::kit_file_kind(path, project_root) {
                    let source = std::fs::read_to_string(path)
                        .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                    let transformed = kit::transform_kit_source(kind, path, &source)
                        .unwrap_or_else(|| source.clone());
                    // Always cache SvelteKit files so relative imports resolve within temp_src.
                    write_contents = Some(transformed);
                    is_kit_file = true;
                } else if is_ts_like_file(path) {
                    // For non-kit TypeScript files, apply Promise.all empty array fix if needed
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Some(patched) =
                            kit::transform_promise_all_empty_arrays(path, &content)
                        {
                            write_contents = Some(patched);
                        }
                    }
                }

                let Some(contents) = write_contents else {
                    continue;
                };

                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                    let mut dir = parent.to_owned();
                    while dir.starts_with(temp_src) && dir != temp_src {
                        cache_dirs.insert(dir.clone());
                        if let Some(p) = dir.parent() {
                            dir = p.to_owned();
                        } else {
                            break;
                        }
                    }
                }

                let wrote = write_if_changed(&target, contents.as_bytes(), "write patched source")?;
                if is_kit_file {
                    if wrote {
                        stats.kit_written += 1;
                    } else {
                        stats.kit_skipped += 1;
                    }
                } else if wrote {
                    stats.patched_written += 1;
                } else {
                    stats.patched_skipped += 1;
                }

                cache_files.insert(target);
                patched_sources.push(path.to_owned());
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

        Ok(patched_sources)
    }

    /// Runs type-checking on the transformed files.
    pub async fn check(
        &self,
        files: &TransformedFiles,
        emit_diagnostics: bool,
    ) -> Result<TsgoCheckOutput, TsgoError> {
        let total_start = Instant::now();
        let mut stats = TsgoCheckStats::default();
        let cache_root = if self.use_cache {
            Some(Self::project_cache_root_for(&self.project_root)?)
        } else {
            None
        };
        let mut tsconfig_overrides = Map::new();

        if self.use_cache {
            // Enable incremental builds for faster subsequent runs
            tsconfig_overrides.insert("incremental".to_string(), Value::Bool(true));
            let tsbuildinfo_path = cache_root
                .as_ref()
                .expect("cache_root set when use_cache is true")
                .join("tsgo.tsbuildinfo");
            tsconfig_overrides.insert(
                "tsBuildInfoFile".to_string(),
                Value::String(tsbuildinfo_path.to_string()),
            );
        } else {
            // Force incremental off to avoid writing tsbuildinfo files.
            tsconfig_overrides.insert("incremental".to_string(), Value::Bool(false));
        }

        // Verify tsgo exists
        if !self.tsgo_path.exists() {
            return Err(TsgoError::NotFound(self.tsgo_path.clone()));
        }

        let temp_dir = if self.use_cache {
            None
        } else {
            Some(
                Builder::new()
                    .prefix("svelte-check-rs-")
                    .tempdir()
                    .map_err(|e| TsgoError::TempFileFailed(format!("create temp dir: {e}")))?,
            )
        };

        // Use a stable cache directory for transformed files under node_modules/.cache when enabled.
        // Otherwise, use a fresh temp directory per run.
        let temp_path = if let Some(cache_root) = &cache_root {
            cache_root.clone()
        } else if let Some(dir) = &temp_dir {
            Utf8PathBuf::try_from(dir.path().to_path_buf())
                .map_err(|_| TsgoError::TempFileFailed("temp dir path is not valid UTF-8".into()))?
        } else {
            return Err(TsgoError::TempFileFailed("temp dir not available".into()));
        };

        std::fs::create_dir_all(&temp_path)
            .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        let cache_path = temp_path.as_path();

        let write_start = Instant::now();
        // Write transformed files
        let mut tsconfig_files: Vec<Utf8PathBuf> = Vec::new();
        let mut cache_files: HashSet<Utf8PathBuf> = HashSet::new();
        let mut cache_dirs: HashSet<Utf8PathBuf> = HashSet::new();
        cache_dirs.insert(cache_path.to_owned());

        for (virtual_path, file) in &files.files {
            let full_path = temp_path.join(virtual_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                // Track all parent directories
                let mut dir = parent.to_owned();
                while dir.starts_with(cache_path) && dir != cache_path {
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
            tsconfig_files.push(full_path.clone());

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
                cache_files.insert(stub_path.clone());
                tsconfig_files.push(stub_path);
            }
        }

        // Clean up stale cache files (from deleted .svelte files)
        // Note: we only clean .svelte.ts and .svelte.d.ts here.
        // Other source files are cleaned in write_source_patches.
        for entry in WalkDir::new(cache_path).follow_links(false).into_iter() {
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

        // Keep a stable copy of .svelte-kit to avoid invalidating tsgo incremental builds.
        // When caching is enabled, write directly into the cache root to avoid duplicate copies.
        let kit_source = self.project_root.join(".svelte-kit");
        let kit_include = if kit_source.exists() {
            if self.use_sveltekit_cache {
                let target = temp_path.join(".svelte-kit");
                sync_sveltekit_cache(&kit_source, &target)?;
                Some(target)
            } else {
                let cached = temp_path.join(".svelte-kit");
                if cached.exists() {
                    let _ = std::fs::remove_dir_all(&cached);
                }
                Some(kit_source)
            }
        } else {
            None
        };
        if let Some(kit_path) = &kit_include {
            let ambient = kit_path.join("ambient.d.ts");
            if ambient.exists() {
                tsconfig_files.push(ambient);
            }
            let non_ambient = kit_path.join("non-ambient.d.ts");
            if non_ambient.exists() {
                tsconfig_files.push(non_ambient);
            }
        }

        let project_src = self.project_root.join("src");

        // Write patched source files (SvelteKit transforms, Promise.all fixes).
        let temp_src = temp_path.join("src");
        let source_start = Instant::now();
        let patched_sources = Self::write_source_patches(
            &self.project_root,
            &project_src,
            &temp_src,
            &mut stats.cache,
        )?;
        stats.timings.source_tree_time = source_start.elapsed();

        let helpers_path = temp_path.join(SHARED_HELPERS_FILENAME);
        let _ = write_if_changed(
            &helpers_path,
            SHARED_HELPERS_DTS.as_bytes(),
            "write helpers",
        )?;
        tsconfig_files.push(helpers_path.clone());

        for source in &patched_sources {
            if let Ok(relative) = source.strip_prefix(&self.project_root) {
                tsconfig_files.push(temp_path.join(relative));
            }
        }

        // Generate a standalone tsconfig overlay with rootDirs and absolute paths.
        let project_tsconfig = self.resolve_tsconfig_path()?;
        let tsconfig_start = Instant::now();
        let overlay_options = TsconfigOverlayOptions {
            temp_root: temp_path.as_path(),
            tsconfig_path: &project_tsconfig,
            overrides: Some(&tsconfig_overrides),
            kit_include: kit_include.as_deref(),
            patched_sources: &patched_sources,
            extra_files: &tsconfig_files,
        };
        let temp_tsconfig = self.prepare_tsconfig_overlay(&overlay_options, &mut stats.cache)?;
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

fn parse_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let arr = value?.as_array()?;
    let mut values = Vec::new();
    for item in arr {
        if let Some(text) = item.as_str() {
            values.push(text.to_string());
        }
    }
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn read_tsconfig_value(path: &Utf8Path) -> Option<Value> {
    let contents = std::fs::read_to_string(path).ok()?;
    let contents = strip_json_comments(&contents);
    serde_json::from_str(&contents).ok()
}

fn clean_path(path: &Utf8Path) -> Utf8PathBuf {
    let mut prefix: Option<String> = None;
    let mut root: Option<String> = None;
    let mut parts: Vec<String> = Vec::new();

    for component in path.components() {
        match component {
            camino::Utf8Component::Prefix(prefix_component) => {
                prefix = Some(prefix_component.as_str().to_string());
            }
            camino::Utf8Component::RootDir => {
                root = Some(component.as_str().to_string());
            }
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir => {
                if let Some(last) = parts.last() {
                    if last != ".." {
                        parts.pop();
                        continue;
                    }
                }
                if root.is_none() && prefix.is_none() {
                    parts.push("..".to_string());
                }
            }
            camino::Utf8Component::Normal(name) => {
                parts.push(name.to_string());
            }
        }
    }

    let mut out = Utf8PathBuf::new();
    if let Some(prefix) = prefix {
        out.push(prefix);
    }
    if let Some(root) = root {
        out.push(root);
    }
    for part in parts {
        out.push(part);
    }
    out
}

fn absolutize_pattern(base_dir: &Utf8Path, pattern: &str) -> String {
    let pattern = pattern.trim();
    let path = Utf8Path::new(pattern);
    if path.is_absolute() {
        clean_path(path).to_string()
    } else {
        clean_path(&base_dir.join(path)).to_string()
    }
}

fn resolve_base_url(base_dir: &Utf8Path, base_url: &str) -> Utf8PathBuf {
    let base = Utf8Path::new(base_url);
    if base.is_absolute() {
        clean_path(base)
    } else {
        clean_path(&base_dir.join(base))
    }
}

fn resolve_path_value(base_dir: &Utf8Path, value: &str) -> String {
    let path = Utf8Path::new(value);
    if path.is_absolute() {
        clean_path(path).to_string()
    } else {
        clean_path(&base_dir.join(path)).to_string()
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

/// Removes single-line and multi-line comments from JSON.
fn strip_json_comments(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut chars = json.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '"' {
                in_string = false;
            } else if c == '\\' {
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
        } else if c == '"' {
            result.push(c);
            in_string = true;
        } else if c == '/' {
            match chars.peek() {
                Some('/') => {
                    chars.next();
                    for n in chars.by_ref() {
                        if n == '\n' {
                            result.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    while let Some(n) = chars.next() {
                        if n == '*' {
                            if let Some('/') = chars.peek() {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
                _ => result.push(c),
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn read_sync_manifest(path: &Utf8Path) -> Option<SvelteKitSyncManifest> {
    let contents = std::fs::read(path).ok()?;
    serde_json::from_slice(&contents).ok()
}

fn write_sync_manifest(path: &Utf8Path, manifest: &SvelteKitSyncManifest) -> Result<(), TsgoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| TsgoError::TempFileFailed(format!("create sync dir: {e}")))?;
    }
    let contents =
        serde_json::to_vec(manifest).map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
    let _ = write_if_changed(path, &contents, "write kit sync manifest")?;
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

fn file_hash(path: &Utf8Path) -> Result<SvelteKitFileStamp, TsgoError> {
    let contents = std::fs::read(path).map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
    let hash = Hasher::new()
        .update(&contents)
        .finalize()
        .to_hex()
        .to_string();
    Ok(SvelteKitFileStamp { hash })
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

fn is_js_like_file(path: &Utf8Path) -> bool {
    matches!(path.extension(), Some("js" | "jsx" | "mjs" | "cjs"))
}

fn is_ts_like_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    )
}

/// A transformed file ready for type-checking.
#[derive(Debug, Clone)]
pub struct TransformedFile {
    /// The original Svelte file path.
    pub original_path: Utf8PathBuf,
    /// The generated TSX content.
    pub tsx_content: String,
    /// Line index for the generated content (for fast source mapping).
    pub generated_line_index: source_map::LineIndex,
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
                generated_line_index: LineIndex::new("// generated"),
                source_map: SourceMap::new(),
                original_line_index: LineIndex::new(original_source),
            },
        );

        assert!(files.get(Utf8Path::new("src/App.svelte.ts")).is_some());
        assert!(files
            .find_by_original(Utf8Path::new("src/App.svelte"))
            .is_some());
    }
}
