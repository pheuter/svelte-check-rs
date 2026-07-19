//! Main orchestration logic.

use crate::cli::{Args, TimingFormat};
use crate::config::{KitConfig, SvelteCompilerOptions, SvelteConfig, SvelteFileKind, TsConfig};
use crate::output::{CheckSummary, FormattedDiagnostic, Formatter, Position};
use bun_runner::{
    BunCompileOptions, BunConfigSession, BunDiagnostic, BunDiagnosticSeverity,
    BunExperimentalOptions, BunInput, BunLoadedConfig, BunPreprocessError, BunPreprocessed,
    BunRunner,
};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSetBuilder};
use rayon::prelude::*;
use source_map::{LineCol, LineIndex, PreprocessorMap, Span};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use svelte_diagnostics::{check as check_svelte, DiagnosticOptions, Severity};
use svelte_parser::parse;
use svelte_transformer::{transform, transform_module, TransformOptions};
use thiserror::Error;
use tsgo_runner::{
    TransformedFile, TransformedFiles, TsgoCheckOutput, TsgoCheckStats, TsgoDiagnostic, TsgoRunner,
};
use walkdir::WalkDir;

const SHARED_HELPERS_MODULE: &str = "__svelte_check_rs_helpers";
const CONFIG_FILENAMES: &[&str] = &[
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.ts",
    "vite.config.cjs",
    "vite.config.mts",
    "vite.config.cts",
    "svelte.config.js",
    "svelte.config.cjs",
    "svelte.config.mjs",
    "svelte.config.ts",
    "svelte.config.mts",
];

/// Returns the extension label (with leading dot) we should display for a
/// discovered file whose `SvelteFileKind` is unrecognized. Prefers the longest
/// configured user extension that the filename ends with; falls back to the
/// raw `Path::extension()` so unexpected files still get a useful label.
fn unsupported_extension_label(file_name: &str, user_extensions: &[&str]) -> String {
    let mut best: Option<&str> = None;
    for ext in user_extensions {
        if !file_name.ends_with(ext) {
            continue;
        }
        match best {
            None => best = Some(ext),
            Some(prev) if ext.len() > prev.len() => best = Some(ext),
            _ => {}
        }
    }
    if let Some(ext) = best {
        return ext.to_string();
    }
    match file_name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => format!(".{}", ext),
        _ => file_name.to_string(),
    }
}

/// Builds the per-extension "N files with unregistered extension (.X) skipped"
/// warning lines for files we discovered but can't process.
fn format_unsupported_warnings(files: &[Utf8PathBuf], user_extensions: &[&str]) -> Vec<String> {
    if files.is_empty() {
        return Vec::new();
    }
    let mut by_ext: BTreeMap<String, usize> = BTreeMap::new();
    for f in files {
        let label = unsupported_extension_label(f.file_name().unwrap_or(""), user_extensions);
        *by_ext.entry(label).or_insert(0) += 1;
    }
    by_ext
        .into_iter()
        .map(|(ext, count)| {
            let plural = if count == 1 { "file" } else { "files" };
            format!(
                "warning: {} {} with unregistered extension ({}) skipped",
                count, plural, ext
            )
        })
        .collect()
}

fn ensure_relative_path(path: &Utf8Path) -> Utf8PathBuf {
    if !path.is_absolute() {
        return path.to_owned();
    }

    let mut out = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            camino::Utf8Component::Prefix(_) | camino::Utf8Component::RootDir => {}
            _ => out.push(component.as_str()),
        }
    }
    out
}

fn virtual_path_for(file_path: &Utf8Path, workspace: &Utf8Path, suffix_ts: bool) -> Utf8PathBuf {
    let relative = file_path.strip_prefix(workspace).unwrap_or(file_path);
    let relative = ensure_relative_path(relative);
    // Use '/' so HashMap keys are stable across platforms — the lookup side
    // (`strip_cache_prefix` in tsgo-runner) always normalizes to forward
    // slashes, so a `\`-keyed map would always miss on Windows.
    let mut key = to_forward_slash(&relative);
    if suffix_ts {
        key.push_str(".ts");
    }
    Utf8PathBuf::from(key)
}

fn to_forward_slash(path: &Utf8Path) -> String {
    // Normalize unconditionally — the same logic runs on Unix and Windows so
    // unit tests behave identically across platforms.  In practice, the only
    // inputs come from `WalkDir`/`strip_prefix` results, so a legitimate `\`
    // in a Unix filename never reaches this code path.
    let s = path.as_str();
    if s.contains('\\') {
        s.replace('\\', "/")
    } else {
        s.to_string()
    }
}

fn relative_import_path(from_file: &Utf8Path, to: &Utf8Path) -> String {
    // Both inputs must be workspace-relative — the function drops `Prefix`
    // and `RootDir` components, so an accidentally absolute input would
    // silently produce the wrong number of `..` hops and emit a broken
    // module specifier.  Catch the violation in debug builds.
    debug_assert!(
        !from_file.is_absolute(),
        "relative_import_path: `from_file` must be workspace-relative, got {from_file}"
    );
    debug_assert!(
        !to.is_absolute(),
        "relative_import_path: `to` must be workspace-relative, got {to}"
    );

    let from_dir = from_file.parent().unwrap_or(Utf8Path::new(""));
    let from_components: Vec<&str> = from_dir
        .components()
        .filter_map(|c| match c {
            camino::Utf8Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let to_components: Vec<&str> = to
        .components()
        .filter_map(|c| match c {
            camino::Utf8Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();

    let mut common = 0usize;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    // Build the module specifier directly with '/' separators. Routing through
    // `Utf8PathBuf::push` would emit `\` on Windows, which TypeScript treats as
    // an escape sequence in the import string and fails to resolve.
    let parent_hops = from_components.len() - common;
    let mut segments: Vec<&str> = Vec::with_capacity(parent_hops + to_components.len() - common);
    segments.extend(std::iter::repeat_n("..", parent_hops));
    segments.extend_from_slice(&to_components[common..]);

    let mut rel_str = segments.join("/");
    if rel_str.is_empty() {
        rel_str.push('.');
    }
    if !rel_str.starts_with('.') {
        rel_str = format!("./{}", rel_str);
    }
    rel_str
}

fn helpers_import_path_for(virtual_path: &Utf8Path, use_nodenext_imports: bool) -> String {
    let mut path = relative_import_path(virtual_path, Utf8Path::new(SHARED_HELPERS_MODULE));
    if use_nodenext_imports {
        path.push_str(".js");
    }
    path
}

fn svelte_alias_paths(svelte_config: &SvelteConfig) -> HashMap<String, Vec<String>> {
    let mut ts_config = TsConfig::default();
    ts_config.merge_svelte_aliases(svelte_config);
    ts_config.compiler_options.paths
}

/// Normalizes a tsconfig exclude pattern to work with globset.
///
/// tsconfig patterns like "src/excluded/**" need to be normalized to match
/// how globset interprets them against relative paths.
fn normalize_tsconfig_pattern(pattern: &str) -> String {
    let pattern = pattern.trim();

    // If pattern already starts with ** or *, it's likely a rooted pattern
    if pattern.starts_with("**") || pattern.starts_with('*') {
        return pattern.to_string();
    }

    // If pattern starts with ./, remove it (relative to project root)
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);

    // If the pattern doesn't contain **, it might need it for matching
    // e.g., "src/excluded" should match "src/excluded" and "src/excluded/**"
    if !pattern.contains("**") {
        // Check if it ends with a path separator or already has a glob
        if pattern.ends_with('/') || pattern.ends_with("/*") {
            pattern.to_string()
        } else {
            // Pattern like "src/excluded" - could be a directory
            // We want to match both "src/excluded" exactly and "src/excluded/**"
            // Return as-is and let globset handle it, or make it match the directory and all contents
            if pattern.contains('*') {
                pattern.to_string()
            } else {
                // Treat as directory pattern - match the path and everything under it
                format!("{}/**", pattern)
            }
        }
    } else {
        pattern.to_string()
    }
}

/// Normalizes the user-supplied `--ignore` patterns into globset patterns.
///
/// `--ignore` mirrors svelte-check's comma-separated syntax
/// (e.g. `--ignore "dist,build"`) and may also be passed multiple times. Each
/// resulting piece is normalized like a tsconfig `exclude` entry so that a bare
/// directory (`src/foo`, `./src/foo`, `dist`) ignores everything beneath it,
/// while explicit globs (`**/*.test.ts`) pass through unchanged. Without the
/// comma split, the whole `"dist,build"` string was treated as a single literal
/// glob that matched nothing, so the flag appeared to do nothing (#159).
fn normalize_ignore_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .flat_map(|raw| raw.split(','))
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .map(normalize_tsconfig_pattern)
        .collect()
}

fn is_ignored_dir(ignore_set: &globset::GlobSet, relative: &Utf8Path) -> bool {
    // globset patterns use '/' as the segment separator regardless of OS.
    // `relative` comes from `WalkDir` and carries native separators on
    // Windows, so a pattern like `src/excluded/**` (normalized from the
    // user's tsconfig) would fail to match `src\excluded\foo` without this.
    let rel = to_forward_slash(relative);
    if ignore_set.is_match(&rel) {
        return true;
    }
    let mut rel_slash = String::with_capacity(rel.len() + 1);
    rel_slash.push_str(&rel);
    rel_slash.push('/');
    ignore_set.is_match(&rel_slash)
}

/// Orchestration errors.
#[derive(Debug, Error)]
pub enum OrchestratorError {
    /// Failed to read file.
    #[error("failed to read file: {0}")]
    #[allow(dead_code)] // Will be used for better error handling
    ReadFailed(String),

    /// Invalid glob pattern.
    #[error("invalid glob pattern: {0}")]
    InvalidGlob(String),

    /// Watch error.
    #[error("watch error: {0}")]
    WatchFailed(String),

    /// tsgo error.
    #[error("tsgo error: {0}")]
    TsgoError(String),

    /// bun error.
    #[error("bun error: {0}")]
    BunError(String),

    /// Compiler warnings config error.
    #[error("compiler warnings config error: {0}")]
    CompilerConfigError(String),
}

/// Lexically normalizes a path: drops `.` components and resolves `..` against
/// preceding normal components, without touching the filesystem.
///
/// `current_dir().join("./apps/foo")` yields a path with an embedded `/./`
/// segment, while the cache/`generated_path` is clean. The out-of-root import
/// rewrite (#2942) compares these paths, so an un-normalized `.`/`..` in the
/// workspace root makes in-workspace imports look "outside" and get mangled
/// (regression seen on monorepo apps invoked with `--workspace ./apps/...`).
/// Normalizing the workspace root once keeps every downstream path consistent.
fn normalize_lexical(path: &Utf8Path) -> Utf8PathBuf {
    let mut out = Utf8PathBuf::new();
    let mut normal_depth = 0usize;
    for comp in path.components() {
        match comp {
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                if normal_depth > 0 {
                    out.pop();
                    normal_depth -= 1;
                } else if out.as_str().is_empty() {
                    out.push("..");
                }
            }
            Utf8Component::Normal(segment) => {
                out.push(segment);
                normal_depth += 1;
            }
            root_or_prefix => out.push(root_or_prefix.as_str()),
        }
    }
    if out.as_str().is_empty() {
        out.push(".");
    }
    out
}

/// Resolve symlinks in a root path so every derived path (cache dir,
/// `rootDirs`, `extends`, file lists) is physical and prefix-matches the
/// paths tsgo resolves on disk. A workspace reached through a symlink (e.g.
/// macOS `/tmp` -> `/private/tmp`) otherwise breaks the `rootDirs` mapping
/// between the cache mirror and the real sources, so relative imports from
/// transformed files stop resolving and surface as false TS2307 errors.
fn canonicalize_physical(path: &Utf8Path) -> Utf8PathBuf {
    if cfg!(windows) {
        // std::fs::canonicalize yields verbatim (`\\?\C:\...`) paths on
        // Windows, which downstream tooling does not handle; keep the
        // lexically-normalized path there.
        return path.to_owned();
    }
    path.canonicalize_utf8().unwrap_or_else(|_| path.to_owned())
}

fn normalize_dependency_path(workspace: &Utf8Path, path: &Utf8Path) -> Utf8PathBuf {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        workspace.join(path)
    };
    canonicalize_with_missing_suffix(&normalize_lexical(&path))
}

fn preprocessor_error_dependency(
    component_path: &Utf8Path,
    error_file: &Utf8Path,
) -> Option<Utf8PathBuf> {
    // Valid file URLs are converted by the Bun bridge. Any URL left here is
    // not a filesystem path and cannot be watched by notify.
    if error_file.as_str().contains("://") {
        return None;
    }
    Some(if error_file.is_absolute() {
        error_file.to_owned()
    } else {
        component_path
            .parent()
            .unwrap_or_else(|| Utf8Path::new(""))
            .join(error_file)
    })
}

fn canonicalize_with_missing_suffix(path: &Utf8Path) -> Utf8PathBuf {
    let mut ancestor = path.to_owned();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        let Some(name) = ancestor.file_name() else {
            return path.to_owned();
        };
        suffix.push(name.to_string());
        if !ancestor.pop() {
            return path.to_owned();
        }
    }

    let mut canonical = ancestor.canonicalize_utf8().unwrap_or(ancestor);
    #[cfg(windows)]
    {
        let value = canonical.as_str().to_string();
        canonical = if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            Utf8PathBuf::from(format!(r"\\{rest}"))
        } else if let Some(rest) = value.strip_prefix(r"\\?\") {
            Utf8PathBuf::from(rest)
        } else {
            Utf8PathBuf::from(value)
        };
    }
    for name in suffix.into_iter().rev() {
        canonical.push(name);
    }
    canonical
}

fn paths_match(left: &Utf8Path, right: &Utf8Path) -> bool {
    left == right || (cfg!(windows) && left.as_str().eq_ignore_ascii_case(right.as_str()))
}

async fn run_bun_load_config(
    workspace: &Utf8Path,
) -> Result<(BunLoadedConfig, BunConfigSession), OrchestratorError> {
    let bun_path = BunRunner::ensure_bun(Some(workspace))
        .await
        .map_err(|error| OrchestratorError::BunError(error.to_string()))?;
    BunRunner::new(bun_path, workspace.to_owned(), 1)
        .map_err(|error| OrchestratorError::BunError(error.to_string()))?
        .load_config_session()
        .await
        .map_err(|error| OrchestratorError::BunError(error.to_string()))
}

/// Runs the check on all files.
pub async fn run(args: Args) -> Result<CheckSummary, OrchestratorError> {
    let workspace = if args.workspace.is_relative() {
        std::env::current_dir()
            .map(|p| Utf8PathBuf::try_from(p).unwrap_or_default())
            .unwrap_or_default()
            .join(&args.workspace)
    } else {
        args.workspace.clone()
    };
    let workspace = canonicalize_physical(&normalize_lexical(&workspace));

    // Execute the same effective Vite/Svelte configuration source that supplies
    // preprocessors. This prevents compiler options and preprocessors from
    // being combined from two unrelated files.
    let (loaded_config, config_session) = run_bun_load_config(&workspace).await?;
    let svelte_config = SvelteConfig {
        extensions: loaded_config.extensions.clone(),
        exclude: Vec::new(),
        kit: KitConfig {
            alias: loaded_config.kit_alias.clone(),
        },
        compiler_options: SvelteCompilerOptions {
            runes: loaded_config.runes,
            experimental_async: loaded_config.experimental_async,
        },
    };
    let extra_paths = svelte_alias_paths(&svelte_config);
    let compiler_bun_options = BunCompileOptions {
        runes: svelte_config.compiler_options.runes,
        dev: None,
        generate: None,
        experimental: svelte_config
            .compiler_options
            .experimental_async
            .map(|enabled| BunExperimentalOptions {
                async_: Some(enabled),
            }),
    };

    // Load tsconfig to detect module resolution strategy
    let ts_config_path = if let Some(ref custom_path) = args.tsconfig {
        Some(custom_path.clone())
    } else {
        TsConfig::find(&workspace).map(|(path, _)| path)
    };
    let ts_config = ts_config_path.as_ref().and_then(|p| TsConfig::load(p));
    let use_nodenext_imports = ts_config
        .as_ref()
        .map(|c| c.compiler_options.requires_explicit_extensions())
        .unwrap_or(false);

    // Handle --show-config flag
    if args.show_config {
        eprintln!("=== svelte-check-rs configuration ===");
        eprintln!("workspace: {}", workspace);
        eprintln!();
        eprintln!("=== svelte.config.js ===");
        eprintln!("file_extensions: {:?}", svelte_config.file_extensions());
        eprintln!("kit.alias: {:?}", svelte_config.kit.alias);
        eprintln!();
        eprintln!("=== tsconfig.json ===");
        if let Some(ref path) = ts_config_path {
            eprintln!("path: {}", path);
        } else {
            eprintln!("path: (not found)");
        }
        if let Some(ref config) = ts_config {
            eprintln!("module: {:?}", config.compiler_options.module);
            eprintln!(
                "moduleResolution: {:?}",
                config.compiler_options.module_resolution
            );
            eprintln!("target: {:?}", config.compiler_options.target);
            eprintln!("strict: {:?}", config.compiler_options.strict);
            eprintln!("baseUrl: {:?}", config.compiler_options.base_url);
            eprintln!("paths: {:?}", config.compiler_options.paths);
            eprintln!("exclude: {:?}", config.exclude);
            eprintln!("requires_explicit_extensions: {}", use_nodenext_imports);
        } else if ts_config_path.is_some() {
            eprintln!("(failed to parse tsconfig)");
        }
        eprintln!();
        eprintln!("=== CLI overrides ===");
        eprintln!("ignore patterns: {:?}", args.ignore);
        eprintln!("threshold: {:?}", args.threshold);
        return Ok(CheckSummary {
            file_count: 0,
            error_count: 0,
            warning_count: 0,
            fail_on_warnings: false,
        });
    }

    let timings_enabled = args.timings
        || args.timings_format == TimingFormat::Json
        || read_env_bool("SVELTE_CHECK_RS_TIMINGS").unwrap_or(false);

    // Build ignore glob set
    let mut ignore_builder = GlobSetBuilder::new();
    for normalized in normalize_ignore_patterns(&args.ignore) {
        let glob =
            Glob::new(&normalized).map_err(|e| OrchestratorError::InvalidGlob(e.to_string()))?;
        ignore_builder.add(glob);
    }

    // Add default ignores
    for pattern in [
        "**/node_modules/**",
        "**/dist/**",
        "**/.svelte-kit/**",
        "**/.svelte-check-rs/**",
        "**/node_modules/.cache/svelte-check-rs/**",
    ] {
        if let Ok(glob) = Glob::new(pattern) {
            ignore_builder.add(glob);
        }
    }

    // Add tsconfig exclude patterns (Issue #19)
    // These patterns should exclude files from both TypeScript AND Svelte diagnostics
    if let Some(ref config) = ts_config {
        for pattern in &config.exclude {
            // Convert tsconfig glob patterns to globset patterns
            // tsconfig uses patterns like "src/excluded/**" or "**/*.test.ts"
            // Make sure patterns work with both relative paths we use
            let normalized = normalize_tsconfig_pattern(pattern);
            if let Ok(glob) = Glob::new(&normalized) {
                ignore_builder.add(glob);
            }
        }
    }

    let ignore_set = ignore_builder
        .build()
        .map_err(|e| OrchestratorError::InvalidGlob(e.to_string()))?;

    // Find Svelte files
    let scan_start = Instant::now();
    let extensions = svelte_config.file_extensions();
    let files: Vec<Utf8PathBuf> = WalkDir::new(&workspace)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if !entry.file_type().is_dir() {
                return true;
            }
            let path = match Utf8Path::from_path(entry.path()) {
                Some(path) => path,
                None => return true,
            };
            let relative = path.strip_prefix(&workspace).unwrap_or(path);
            !is_ignored_dir(&ignore_set, relative)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| Utf8PathBuf::try_from(e.into_path()).ok())
        .filter(|p| {
            let file_name = p.file_name().unwrap_or("");
            extensions.iter().any(|ext| file_name.ends_with(ext))
        })
        .filter(|p| {
            let relative = p.strip_prefix(&workspace).unwrap_or(p);
            !ignore_set.is_match(to_forward_slash(relative).as_str())
        })
        .collect();
    let file_scan_time = if timings_enabled {
        Some(scan_start.elapsed())
    } else {
        None
    };

    // Split off files whose extension we don't natively understand (e.g. `.svx`
    // from mdsvex). They were registered in `svelte.config.js#extensions` so
    // they showed up in the walk, but we can't transform them — feeding them to
    // the Svelte/TS pipeline would either be silently dropped or break tsgo. We
    // warn the user once per extension and exclude them from everything below.
    let (files, unsupported_files): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|f| SvelteFileKind::from_path(f).is_some());

    if !unsupported_files.is_empty() {
        let user_extensions = svelte_config.unsupported_extensions();
        for line in format_unsupported_warnings(&unsupported_files, &user_extensions) {
            eprintln!("{}", line);
        }
    }

    // Handle --single-file flag: filter to just the specified file
    let files = if let Some(ref single_file) = args.single_file {
        let target = if single_file.is_relative() {
            workspace.join(single_file)
        } else {
            // Discovered files derive from the canonicalized workspace root,
            // so an absolute target given through a symlink must be resolved
            // the same way to match.
            canonicalize_physical(single_file)
        };
        let matched: Vec<_> = files.into_iter().filter(|f| f == &target).collect();
        if matched.is_empty() {
            eprintln!(
                "Warning: --single-file '{}' not found in discovered files. Check if path is correct.",
                single_file
            );
        }
        matched
    } else {
        files
    };

    // Handle --list-files flag: print files and exit
    if args.list_files {
        eprintln!("=== Files to check ({}) ===", files.len());
        for file in &files {
            let relative = file.strip_prefix(&workspace).unwrap_or(file);
            println!("{}", relative);
        }
        return Ok(CheckSummary {
            file_count: files.len(),
            error_count: 0,
            warning_count: 0,
            fail_on_warnings: false,
        });
    }

    let mut svelte_run_config = SvelteRunConfig {
        compiler_options: compiler_bun_options,
        config_path: loaded_config.config_file_path.clone(),
        config_dependencies: loaded_config
            .dependencies
            .iter()
            .map(|path| normalize_dependency_path(&workspace, path))
            .collect(),
        config_error: loaded_config.error.clone(),
        config_session: loaded_config.has_preprocess.then_some(config_session),
    };

    if args.watch {
        run_watch_mode(
            &args,
            &workspace,
            files,
            file_scan_time,
            use_nodenext_imports,
            svelte_run_config,
            &extra_paths,
        )
        .await
    } else {
        Ok(run_single_check(
            &args,
            &workspace,
            files,
            file_scan_time,
            use_nodenext_imports,
            &mut svelte_run_config,
            &extra_paths,
        )
        .await?
        .summary)
    }
}

struct SvelteRunConfig {
    compiler_options: BunCompileOptions,
    config_path: Option<Utf8PathBuf>,
    config_dependencies: HashSet<Utf8PathBuf>,
    config_error: Option<String>,
    config_session: Option<BunConfigSession>,
}

fn apply_loaded_config(
    workspace: &Utf8Path,
    run_config: &mut SvelteRunConfig,
    extra_paths: &mut HashMap<String, Vec<String>>,
    loaded: BunLoadedConfig,
    session: BunConfigSession,
) {
    let dependencies: HashSet<_> = loaded
        .dependencies
        .iter()
        .map(|path| normalize_dependency_path(workspace, path))
        .collect();
    if let Some(error) = loaded.error {
        // Keep last-known imported modules so restoring a broken import still
        // causes a config reload rather than only a generic source recheck.
        run_config.config_dependencies.extend(dependencies);
        run_config.config_path = loaded.config_file_path;
        run_config.config_error = Some(error);
        run_config.config_session = None;
        return;
    }

    *extra_paths = svelte_alias_paths(&SvelteConfig {
        extensions: loaded.extensions.clone(),
        exclude: Vec::new(),
        kit: KitConfig {
            alias: loaded.kit_alias.clone(),
        },
        compiler_options: SvelteCompilerOptions {
            runes: loaded.runes,
            experimental_async: loaded.experimental_async,
        },
    });
    run_config.config_path = loaded
        .has_preprocess
        .then_some(loaded.config_file_path)
        .flatten();
    run_config.config_dependencies = dependencies;
    run_config.config_error = None;
    run_config.compiler_options = BunCompileOptions {
        runes: loaded.runes,
        dev: None,
        generate: None,
        experimental: loaded
            .experimental_async
            .map(|enabled| BunExperimentalOptions {
                async_: Some(enabled),
            }),
    };
    run_config.config_session = loaded.has_preprocess.then_some(session);
}

struct CheckRun {
    summary: CheckSummary,
    dependencies: HashSet<Utf8PathBuf>,
    dependencies_complete: bool,
}

/// Runs a single check pass.
async fn run_single_check(
    args: &Args,
    workspace: &Utf8Path,
    files: Vec<Utf8PathBuf>,
    file_scan_time: Option<std::time::Duration>,
    use_nodenext_imports: bool,
    svelte_run_config: &mut SvelteRunConfig,
    extra_paths: &HashMap<String, Vec<String>>,
) -> Result<CheckRun, OrchestratorError> {
    let total_start = Instant::now();
    let timings_enabled = args.timings
        || args.timings_format == TimingFormat::Json
        || read_env_bool("SVELTE_CHECK_RS_TIMINGS").unwrap_or(false);
    let formatter = Formatter::new(args.output);
    let output_json = matches!(args.output, crate::cli::OutputFormat::Json);
    let error_count = AtomicUsize::new(0);
    let warning_count = AtomicUsize::new(0);
    let compiler_warning_settings = parse_compiler_warnings(args.compiler_warnings.as_deref())?;
    let compiler_options = svelte_run_config.compiler_options.clone();

    // Base diagnostic options (filename will be set per-file)
    let base_diag_options = DiagnosticOptions::all();

    struct FileOutput {
        text: Option<String>,
        json: Vec<FormattedDiagnostic>,
    }

    struct FileResult {
        file_path: Utf8PathBuf,
        output: Option<FileOutput>,
        additional_outputs: Vec<(Utf8PathBuf, FileOutput)>,
        transformed: Option<(Utf8PathBuf, TransformedFile)>,
        compiler_input: Option<BunInput>,
    }

    struct ComponentSource {
        file_path: Utf8PathBuf,
        original: String,
        processed: String,
        source_map: Option<PreprocessorMap>,
        preprocess_error: Option<BunPreprocessError>,
    }

    // Separate files by kind: components (.svelte) vs modules (.svelte.ts/.svelte.js)
    let (component_files, module_files): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|f| SvelteFileKind::from_path(f) == Some(SvelteFileKind::Component));

    let preprocess_start = Instant::now();
    let mut original_sources = HashMap::new();
    let mut preprocess_inputs = Vec::new();
    for file_path in &component_files {
        match fs::read_to_string(file_path) {
            Ok(source) => {
                original_sources.insert(file_path.clone(), source.clone());
                preprocess_inputs.push(BunInput {
                    filename: file_path.clone(),
                    source,
                    options: compiler_options.clone(),
                });
            }
            Err(e) => eprintln!("Failed to read {}: {}", file_path, e),
        }
    }

    let mut dependencies_complete = svelte_run_config.config_error.is_none();
    let processed_sources = if let Some(config_error) = &svelte_run_config.config_error {
        preprocess_inputs
            .into_iter()
            .map(|input| BunPreprocessed {
                filename: input.filename,
                source: input.source,
                source_map: None,
                dependencies: Vec::new(),
                error: Some(BunPreprocessError {
                    message: config_error.clone(),
                    start: None,
                    end: None,
                    phase: None,
                    fragment_offset: None,
                    file: svelte_run_config.config_path.clone(),
                }),
            })
            .collect()
    } else if let Some(session) = &mut svelte_run_config.config_session {
        session
            .preprocess_files(preprocess_inputs)
            .await
            .map_err(|error| OrchestratorError::BunError(error.to_string()))?
    } else {
        preprocess_inputs
            .into_iter()
            .map(|input| BunPreprocessed {
                filename: input.filename,
                source: input.source,
                source_map: None,
                dependencies: Vec::new(),
                error: None,
            })
            .collect()
    };

    let mut dependencies = svelte_run_config.config_dependencies.clone();
    let mut component_sources = Vec::with_capacity(processed_sources.len());
    for processed in processed_sources {
        let BunPreprocessed {
            filename,
            source,
            source_map: raw_source_map,
            dependencies: processed_dependencies,
            error,
        } = processed;
        if let Some(error_dependency) = error.as_ref().and_then(|error| {
            error
                .file
                .as_deref()
                .and_then(|file| preprocessor_error_dependency(&filename, file))
        }) {
            dependencies.insert(normalize_dependency_path(workspace, &error_dependency));
        }
        dependencies.extend(
            processed_dependencies
                .iter()
                .map(|path| normalize_dependency_path(workspace, path)),
        );
        let Some(original) = original_sources.remove(&filename) else {
            continue;
        };
        let mut preprocess_error = error;
        if preprocess_error.is_some() {
            dependencies_complete = false;
        }
        let source_map = match raw_source_map.as_deref().map(PreprocessorMap::parse) {
            Some(Ok(map)) => Some(map),
            Some(Err(message)) => {
                dependencies_complete = false;
                preprocess_error = Some(BunPreprocessError {
                    message: format!("Invalid preprocessor source map: {message}"),
                    start: None,
                    end: None,
                    phase: None,
                    fragment_offset: None,
                    file: None,
                });
                None
            }
            None if source != original => {
                dependencies_complete = false;
                preprocess_error = Some(BunPreprocessError {
                    message: "Preprocessor changed the component without returning a source map; diagnostics cannot be mapped safely".to_string(),
                    start: None,
                    end: None,
                    phase: None,
                    fragment_offset: None,
                    file: None,
                });
                None
            }
            None => None,
        };
        component_sources.push(ComponentSource {
            file_path: filename,
            original,
            processed: source,
            source_map,
            preprocess_error,
        });
    }

    let preprocessor_maps: HashMap<Utf8PathBuf, PreprocessorMap> = component_sources
        .iter()
        .filter_map(|component| {
            component
                .source_map
                .clone()
                .map(|map| (component.file_path.clone(), map))
        })
        .collect();
    let original_component_sources: HashMap<Utf8PathBuf, String> = component_sources
        .iter()
        .map(|component| (component.file_path.clone(), component.original.clone()))
        .collect();
    let preprocess_time = preprocess_start.elapsed();

    let svelte_start = Instant::now();

    // Resolve the cache root once so each transform can compute the eventual
    // generated `.svelte.ts` path. This lets the transformer rewrite relative
    // imports reaching outside the workspace so they resolve from the generated
    // cache location (issue #2942). Falls back to `None` (no rewrite) if the
    // cache root can't be resolved (e.g. node_modules not found).
    let cache_root: Option<Utf8PathBuf> = TsgoRunner::project_cache_root(workspace).ok();
    let workspace_path_str = workspace.to_string();

    // Process component files (.svelte) in parallel: parse, run Svelte diagnostics, and transform
    let component_results: Vec<FileResult> = component_sources
        .par_iter()
        .map(|component| {
            let file_path = &component.file_path;
            let source = &component.processed;
            let original_source = &component.original;

            // Parse the file
            let parse_result = parse(source);

            // If emit_ast is enabled, print parsed AST for each file
            if args.emit_ast {
                let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                eprintln!("=== AST for {} ===", relative_path);
                eprintln!("{:#?}", parse_result.document);
                if !parse_result.errors.is_empty() {
                    eprintln!("=== Parse errors ===");
                    for error in &parse_result.errors {
                        eprintln!("  {:?}", error);
                    }
                }
                eprintln!();
            }

            // Collect parse errors
            let mut all_diagnostics = Vec::new();
            let mut additional_outputs = Vec::new();

            if let Some(error) = &component.preprocess_error {
                let external = error.file.as_ref().and_then(|error_file| {
                    let path = if error_file.is_absolute() {
                        error_file.clone()
                    } else {
                        file_path.parent().unwrap_or(workspace).join(error_file)
                    };
                    let path = canonicalize_physical(&normalize_lexical(&path));
                    (path != *file_path)
                        .then(|| fs::read_to_string(&path).ok().map(|text| (path, text)))
                        .flatten()
                });
                if let Some((error_path, error_source)) = external {
                    let mut external_error = error.clone();
                    external_error.phase = None;
                    external_error.fragment_offset = None;
                    let diagnostic = svelte_diagnostics::Diagnostic::new(
                        svelte_diagnostics::DiagnosticCode::PreprocessError,
                        &error.message,
                        preprocess_error_span(&external_error, &error_source),
                    );
                    let relative_path = error_path
                        .strip_prefix(workspace)
                        .unwrap_or(&error_path)
                        .to_owned();
                    let diagnostics = [diagnostic];
                    additional_outputs.push((
                        error_path,
                        FileOutput {
                            text: if output_json {
                                None
                            } else {
                                Some(formatter.format(&diagnostics, &relative_path, &error_source))
                            },
                            json: if output_json {
                                Formatter::format_json_diagnostics(
                                    &diagnostics,
                                    &relative_path,
                                    &error_source,
                                )
                            } else {
                                Vec::new()
                            },
                        },
                    ));
                    error_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    let span = preprocess_error_span(error, source);
                    all_diagnostics.push(svelte_diagnostics::Diagnostic::new(
                        svelte_diagnostics::DiagnosticCode::PreprocessError,
                        &error.message,
                        span,
                    ));
                }
            } else {
                // Only diagnostics from a successfully preprocessed document
                // have coordinates that can be mapped safely.
                for error in &parse_result.errors {
                    all_diagnostics.push(svelte_diagnostics::Diagnostic::new(
                        svelte_diagnostics::DiagnosticCode::ParseError,
                        error.to_string(),
                        error.span,
                    ));
                }

                let file_diag_options = base_diag_options
                    .clone()
                    .with_filename(file_path.to_string());
                all_diagnostics.extend(check_svelte(&parse_result.document, file_diag_options));
            }

            all_diagnostics.retain(|diag| include_svelte_severity(diag.severity, args.threshold));

            if let Some(preprocessor_map) = &component.source_map {
                map_svelte_diagnostics(
                    &mut all_diagnostics,
                    preprocessor_map,
                    file_path,
                    source,
                    original_source,
                );
            }

            // Transform for TypeScript checking (if JS diagnostics enabled and not skipping tsgo)
            // Also transform if emit_ts or emit_source_map is enabled (for debugging)
            let mut transformed = None;
            let should_transform = component.preprocess_error.is_none()
                && (!args.skip_tsgo || args.emit_ts || args.emit_source_map);
            if should_transform {
                let virtual_path = virtual_path_for(file_path, workspace, true);
                let helpers_import = helpers_import_path_for(&virtual_path, use_nodenext_imports);
                // Absolute path the transformed file will eventually be written
                // to in the cache (matches `cache_root.join(virtual_path)` in
                // TsgoRunner::check). Used to rewrite out-of-root imports.
                let generated_path = cache_root
                    .as_ref()
                    .map(|root| root.join(&virtual_path).to_string());
                let workspace_path = generated_path.as_ref().map(|_| workspace_path_str.clone());
                let transform_options = TransformOptions {
                    filename: Some(file_path.to_string()),
                    source_maps: true,
                    use_nodenext_imports,
                    helpers_import_path: Some(helpers_import),
                    workspace_path,
                    generated_path,
                };

                let transform_result = transform(&parse_result.document, transform_options);

                // If emit_ts is enabled, print transformed TypeScript for each file.
                if args.emit_ts {
                    let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                    eprintln!(
                        "=== TypeScript for {} ===\n{}",
                        relative_path, transform_result.tsx_code
                    );
                }

                // If emit_source_map is enabled, print source map mappings
                if args.emit_source_map {
                    let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                    eprintln!(
                        "=== Source Map for {} ({} mappings) ===",
                        relative_path,
                        transform_result.source_map.len()
                    );
                    for (i, mapping) in transform_result.source_map.mappings().enumerate() {
                        eprintln!(
                            "  {}: generated {}..{} -> original {}..{}",
                            i,
                            u32::from(mapping.generated.start),
                            u32::from(mapping.generated.end),
                            u32::from(mapping.original.start),
                            u32::from(mapping.original.end)
                        );
                    }
                    eprintln!();
                }

                // Only add to transformed files collection if we're going to run tsgo
                if !args.skip_tsgo {
                    // Create the virtual path (original.svelte -> original.svelte.ts)
                    let virtual_path = virtual_path_for(file_path, workspace, true);

                    let tsx_code = transform_result.tsx_code;
                    let transformed_file = TransformedFile {
                        original_path: file_path.clone(),
                        generated_line_index: LineIndex::new_typescript(&tsx_code),
                        tsx_content: tsx_code,
                        source_map: transform_result.source_map,
                        processed_line_index: LineIndex::new(source),
                        preprocessor_map: component.source_map.clone(),
                    };

                    transformed = Some((virtual_path, transformed_file));
                }
            }

            // Count errors and warnings
            for diag in &all_diagnostics {
                match diag.severity {
                    Severity::Error => {
                        error_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Severity::Warning => {
                        warning_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Severity::Hint => {}
                }
            }

            let output = if all_diagnostics.is_empty() {
                None
            } else {
                let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                Some(FileOutput {
                    text: if output_json {
                        None
                    } else {
                        Some(formatter.format(&all_diagnostics, relative_path, original_source))
                    },
                    json: if output_json {
                        Formatter::format_json_diagnostics(
                            &all_diagnostics,
                            relative_path,
                            original_source,
                        )
                    } else {
                        Vec::new()
                    },
                })
            };

            let compiler_input = component.preprocess_error.is_none().then(|| BunInput {
                filename: file_path.clone(),
                source: source.clone(),
                options: compiler_options.clone(),
            });

            FileResult {
                file_path: file_path.clone(),
                output,
                additional_outputs,
                transformed,
                compiler_input,
            }
        })
        .collect();

    // Process module files (.svelte.ts/.svelte.js) in parallel: transform runes only
    let module_results: Vec<FileResult> = module_files
        .par_iter()
        .map(|file_path| {
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", file_path, e);
                    return FileResult {
                        file_path: file_path.clone(),
                        output: None,
                        additional_outputs: Vec::new(),
                        transformed: None,
                        compiler_input: None,
                    };
                }
            };

            // Transform module file (runes only, no template/styles)
            let virtual_path = virtual_path_for(file_path, workspace, false);
            let helpers_import = helpers_import_path_for(&virtual_path, use_nodenext_imports);
            // (workspace_path, generated_path) for out-of-root import rewriting.
            let external_imports = cache_root.as_ref().map(|root| {
                (
                    workspace_path_str.clone(),
                    root.join(&virtual_path).to_string(),
                )
            });
            let transform_result = transform_module(
                &source,
                Some(file_path.as_str()),
                Some(helpers_import),
                external_imports,
            );

            // Collect any errors from invalid rune usage (e.g., $props in module files)
            let mut all_diagnostics: Vec<svelte_diagnostics::Diagnostic> = Vec::new();
            for error in &transform_result.errors {
                // Compute byte offset from line/column
                let offset = line_column_to_offset(&source, error.line, error.column);
                let span = source_map::Span::new(offset, offset + 1);
                all_diagnostics.push(svelte_diagnostics::Diagnostic::new(
                    svelte_diagnostics::DiagnosticCode::ParseError,
                    error.message.clone(),
                    span,
                ));
            }

            // Transform for TypeScript checking (if JS diagnostics enabled)
            let mut transformed = None;
            let should_transform = !args.skip_tsgo || args.emit_ts || args.emit_source_map;
            if should_transform {
                // If emit_ts is enabled, print transformed TypeScript for each file.
                if args.emit_ts {
                    let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                    eprintln!(
                        "=== TypeScript for {} ===\n{}",
                        relative_path, transform_result.code
                    );
                }

                // For module files, we keep the same relative path (they're already .ts/.js)
                // But we need to write transformed content to the cache
                let virtual_path = virtual_path_for(file_path, workspace, false);

                let tsx_code = transform_result.code;
                let transformed_file = TransformedFile {
                    original_path: file_path.clone(),
                    generated_line_index: LineIndex::new_typescript(&tsx_code),
                    tsx_content: tsx_code,
                    source_map: transform_result.source_map,
                    processed_line_index: LineIndex::new(&source),
                    preprocessor_map: None,
                };

                if !args.skip_tsgo {
                    transformed = Some((virtual_path, transformed_file));
                }
            }

            // Count errors and warnings
            for diag in &all_diagnostics {
                match diag.severity {
                    Severity::Error => {
                        error_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Severity::Warning => {
                        warning_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Severity::Hint => {}
                }
            }

            let output = if all_diagnostics.is_empty() {
                None
            } else {
                let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                Some(FileOutput {
                    text: if output_json {
                        None
                    } else {
                        Some(formatter.format(&all_diagnostics, relative_path, &source))
                    },
                    json: if output_json {
                        Formatter::format_json_diagnostics(&all_diagnostics, relative_path, &source)
                    } else {
                        Vec::new()
                    },
                })
            };

            FileResult {
                file_path: file_path.clone(),
                output,
                additional_outputs: Vec::new(),
                transformed,
                compiler_input: None,
            }
        })
        .collect();

    // Combine outputs and transformed files from both component and module files
    let mut outputs: Vec<FileOutput> = Vec::new();
    let mut transformed_files = TransformedFiles::new();
    let mut compiler_inputs: Vec<BunInput> = Vec::new();
    let mut compiler_sources: HashMap<Utf8PathBuf, String> = HashMap::new();
    let mut files_with_diagnostics: HashSet<Utf8PathBuf> = HashSet::new();
    for result in component_results.into_iter().chain(module_results) {
        if result.output.is_some() {
            files_with_diagnostics.insert(result.file_path);
        }
        if let Some(output) = result.output {
            outputs.push(output);
        }
        for (path, output) in result.additional_outputs {
            files_with_diagnostics.insert(path);
            outputs.push(output);
        }
        if let Some((virtual_path, transformed_file)) = result.transformed {
            transformed_files.add(virtual_path, transformed_file);
        }
        if let Some(input) = result.compiler_input {
            // Only the JSON formatter (`format_compiler_diagnostics_json`) reads
            // `compiler_sources` to attach source snippets. The human formatter
            // does not, so skip the clone to keep peak memory low for large repos.
            if output_json {
                let source = original_component_sources
                    .get(&input.filename)
                    .unwrap_or(&input.source)
                    .clone();
                compiler_sources.insert(input.filename.clone(), source);
            }
            compiler_inputs.push(input);
        }
    }

    let svelte_time = svelte_start.elapsed();

    // Calculate total file count for summary
    let total_file_count = component_files.len() + module_files.len();

    let mut json_output = Vec::new();

    // Print Svelte diagnostics
    if output_json {
        for output in outputs {
            json_output.extend(output.json);
        }
    } else {
        for output in outputs {
            if let Some(text) = output.text {
                print!("{}", text);
            }
        }
    }

    let transformed_count = if args.skip_tsgo {
        0
    } else {
        transformed_files.files.len()
    };

    struct CompilerRun {
        elapsed: std::time::Duration,
        result: Result<Vec<BunDiagnostic>, OrchestratorError>,
    }

    struct TsgoRun {
        elapsed: std::time::Duration,
        sync_elapsed: std::time::Duration,
        sync_ran: bool,
        result: Result<TsgoCheckOutput, OrchestratorError>,
    }

    let compiler_future = async {
        if compiler_inputs.is_empty() {
            return None;
        }

        let bun_start = Instant::now();
        let result = run_bun_check(workspace, compiler_inputs).await;
        Some(CompilerRun {
            elapsed: bun_start.elapsed(),
            result,
        })
    };

    let tsgo_future = async {
        if args.skip_tsgo || transformed_files.files.is_empty() {
            return None;
        }

        if let Err(err) = TsgoRunner::ensure_dependency_cache(workspace) {
            eprintln!("Warning: {}", err);
        }

        let tsgo_start = Instant::now();
        let sync_start = Instant::now();
        let sync_ran = match TsgoRunner::ensure_sveltekit_sync(workspace).await {
            Ok(ran) => ran,
            Err(e) => {
                eprintln!("Warning: {}", e);
                false
            }
        };
        let sync_elapsed = sync_start.elapsed();

        let result = run_tsgo_check(
            workspace,
            &transformed_files,
            args,
            args.tsgo_diagnostics,
            extra_paths,
        )
        .await;

        Some(TsgoRun {
            elapsed: tsgo_start.elapsed(),
            sync_elapsed,
            sync_ran,
            result,
        })
    };

    let (compiler_run, tsgo_run) = tokio::join!(compiler_future, tsgo_future);

    let mut compiler_total_time = None;

    // Print Svelte compiler diagnostics first to preserve output ordering.
    if let Some(run) = compiler_run {
        compiler_total_time = Some(run.elapsed);
        match run.result {
            Ok(mut diagnostics) => {
                map_compiler_diagnostics(&mut diagnostics, &preprocessor_maps);
                apply_compiler_warning_settings(&mut diagnostics, &compiler_warning_settings);
                diagnostics.retain(|diag| include_compiler_severity(diag.severity, args.threshold));

                // Count and print compiler diagnostics
                for diag in &diagnostics {
                    files_with_diagnostics.insert(diag.file.clone());
                    match diag.severity {
                        BunDiagnosticSeverity::Error => {
                            error_count.fetch_add(1, Ordering::Relaxed);
                        }
                        BunDiagnosticSeverity::Warning => {
                            warning_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                if output_json {
                    json_output.extend(format_compiler_diagnostics_json(
                        &diagnostics,
                        workspace,
                        &compiler_sources,
                    ));
                } else {
                    let output = format_compiler_diagnostics(&diagnostics, workspace, args.output);
                    print!("{}", output);
                }
            }
            Err(e) => {
                eprintln!("Svelte compiler checking failed: {}", e);
            }
        }
    }

    let mut tsgo_stats: Option<TsgoCheckStats> = None;
    let mut tsgo_total_time = None;
    let mut sveltekit_sync_time = None;
    let mut sveltekit_sync_ran = None;

    // Then print TypeScript diagnostics, matching the previous phase order.
    if let Some(run) = tsgo_run {
        sveltekit_sync_time = Some(run.sync_elapsed);
        sveltekit_sync_ran = Some(run.sync_ran);

        match run.result {
            Ok(output) => {
                let mut ts_diagnostics = output.diagnostics;
                ts_diagnostics.retain(|diag| include_ts_severity(diag.severity, args.threshold));

                // Count and print TypeScript diagnostics
                for diag in &ts_diagnostics {
                    files_with_diagnostics.insert(diag.file.clone());
                    match diag.severity {
                        tsgo_runner::DiagnosticSeverity::Error => {
                            error_count.fetch_add(1, Ordering::Relaxed);
                        }
                        tsgo_runner::DiagnosticSeverity::Warning
                        | tsgo_runner::DiagnosticSeverity::Suggestion => {
                            warning_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                // Format and print TypeScript diagnostics
                if output_json {
                    json_output.extend(format_ts_diagnostics_json(&ts_diagnostics, workspace));
                } else {
                    let ts_output = format_ts_diagnostics(&ts_diagnostics, workspace, args.output);
                    print!("{}", ts_output);
                }

                tsgo_stats = Some(output.stats);
                tsgo_total_time = Some(run.elapsed);
            }
            Err(e) => {
                eprintln!("TypeScript checking failed: {}", e);
            }
        }

        if args.tsgo_diagnostics {
            if let Some(stats) = &tsgo_stats {
                if let Some(diag) = &stats.diagnostics {
                    eprintln!("=== tsgo diagnostics ===");
                    eprintln!("{}", diag);
                }
            }
        }
    }

    if timings_enabled {
        match args.timings_format {
            TimingFormat::Json => {
                let json = timings_json(
                    file_scan_time,
                    preprocess_time,
                    svelte_time,
                    total_file_count,
                    transformed_count,
                    compiler_total_time,
                    sveltekit_sync_time,
                    sveltekit_sync_ran,
                    tsgo_total_time,
                    tsgo_stats.as_ref(),
                    total_start.elapsed(),
                );
                eprintln!("{}", json);
            }
            TimingFormat::Text => {
                eprintln!("=== svelte-check-rs timings ===");
                if let Some(scan_time) = file_scan_time {
                    eprintln!("file scan: {:?} ({} files)", scan_time, total_file_count);
                }
                eprintln!("preprocess: {:?}", preprocess_time);
                eprintln!(
                    "svelte phase: {:?} ({} files, {} transformed)",
                    svelte_time, total_file_count, transformed_count
                );
                if let Some(compiler_time) = compiler_total_time {
                    eprintln!("svelte compiler: {:?}", compiler_time);
                }
                if let (Some(sync_time), Some(sync_ran)) = (sveltekit_sync_time, sveltekit_sync_ran)
                {
                    eprintln!(
                        "svelte-kit sync: {:?} ({})",
                        sync_time,
                        if sync_ran { "ran" } else { "skipped" }
                    );
                }
                if let Some(tsgo_time) = tsgo_total_time {
                    eprintln!("tsgo total: {:?}", tsgo_time);
                }
                if let Some(stats) = &tsgo_stats {
                    eprintln!(
                        "tsgo write cache: tsx {}/{} stubs {}/{} tsconfig {}/{}",
                        stats.cache.tsx_written,
                        stats.cache.tsx_written + stats.cache.tsx_skipped,
                        stats.cache.stub_written,
                        stats.cache.stub_written + stats.cache.stub_skipped,
                        stats.cache.tsconfig_written,
                        stats.cache.tsconfig_written + stats.cache.tsconfig_skipped
                    );
                    eprintln!(
                        "tsgo source tree: entries {} files {} dirs {} svelte_skipped {} existing_skipped {} linked {} copied {}",
                        stats.cache.source_entries,
                        stats.cache.source_files,
                        stats.cache.source_dirs,
                        stats.cache.source_svelte_skipped,
                        stats.cache.source_existing_skipped,
                        stats.cache.source_linked,
                        stats.cache.source_copied
                    );
                    eprintln!(
                        "tsgo timings: write {:?} source {:?} tsconfig {:?} tsgo {:?} parse {:?}",
                        stats.timings.write_time,
                        stats.timings.source_tree_time,
                        stats.timings.tsconfig_time,
                        stats.timings.tsgo_time,
                        stats.timings.parse_time
                    );
                }
                eprintln!("total: {:?}", total_start.elapsed());
            }
        }
    }

    // Print cache stats if requested (separate from timings)
    if args.cache_stats && !timings_enabled {
        if let Some(stats) = &tsgo_stats {
            eprintln!("=== svelte-check-rs cache stats ===");
            eprintln!(
                "TSX files:     {} written, {} skipped (unchanged)",
                stats.cache.tsx_written, stats.cache.tsx_skipped
            );
            eprintln!(
                "Stub files:    {} written, {} skipped",
                stats.cache.stub_written, stats.cache.stub_skipped
            );
            eprintln!(
                "Kit files:     {} written, {} skipped",
                stats.cache.kit_written, stats.cache.kit_skipped
            );
            eprintln!(
                "Patched files: {} written, {} skipped",
                stats.cache.patched_written, stats.cache.patched_skipped
            );
            eprintln!(
                "TSConfig:      {} written, {} skipped",
                stats.cache.tsconfig_written, stats.cache.tsconfig_skipped
            );
            eprintln!();
            eprintln!("Source tree:");
            eprintln!("  entries:          {}", stats.cache.source_entries);
            eprintln!("  files:            {}", stats.cache.source_files);
            eprintln!("  directories:      {}", stats.cache.source_dirs);
            eprintln!("  svelte skipped:   {}", stats.cache.source_svelte_skipped);
            eprintln!(
                "  existing skipped: {}",
                stats.cache.source_existing_skipped
            );
        } else if args.skip_tsgo {
            eprintln!("=== svelte-check-rs cache stats ===");
            eprintln!("(tsgo was skipped, no cache stats available)");
        } else {
            eprintln!("=== svelte-check-rs cache stats ===");
            eprintln!("(no files were transformed)");
        }
    }

    let summary = CheckSummary {
        file_count: files_with_diagnostics.len(),
        error_count: error_count.load(Ordering::Relaxed),
        warning_count: warning_count.load(Ordering::Relaxed),
        fail_on_warnings: args.fail_on_warnings,
    };

    // Print summary
    if !matches!(args.output, crate::cli::OutputFormat::Json) {
        println!("{}", summary.format());
    } else {
        let json = serde_json::to_string_pretty(&json_output).unwrap_or_else(|_| "[]".to_string());
        println!("{}", json);
    }

    Ok(CheckRun {
        summary,
        dependencies,
        dependencies_complete,
    })
}

/// Runs tsgo type-checking on transformed files.
async fn run_tsgo_check(
    workspace: &Utf8Path,
    files: &TransformedFiles,
    args: &Args,
    emit_diagnostics: bool,
    extra_paths: &HashMap<String, Vec<String>>,
) -> Result<TsgoCheckOutput, OrchestratorError> {
    // Resolve tsgo from workspace node_modules
    let tsgo_path = TsgoRunner::resolve_tsgo(workspace)
        .map_err(|e| OrchestratorError::TsgoError(e.to_string()))?;

    let runner = TsgoRunner::new(
        tsgo_path,
        workspace.to_owned(),
        args.tsconfig.clone(),
        extra_paths.clone(),
    );

    runner
        .check(files, emit_diagnostics)
        .await
        .map_err(|e| OrchestratorError::TsgoError(e.to_string()))
}

fn map_preprocessed_span(
    span: Span,
    map: &PreprocessorMap,
    original_path: &Utf8Path,
    processed_index: &LineIndex,
    original_index: &LineIndex,
) -> Option<Span> {
    let map_offset = |offset| {
        processed_index
            .utf16_line_col(offset)
            .and_then(|position| map.original_position_in(position, original_path))
            .and_then(|position| original_index.offset_utf16(position))
    };

    match (map_offset(span.start), map_offset(span.end)) {
        (Some(start), Some(end)) if end >= start => Some(Span::new(start, end)),
        (Some(start), _) => Some(Span::empty(start)),
        _ => None,
    }
}

fn preprocess_error_span(error: &BunPreprocessError, source: &str) -> Span {
    let source_len = source.len() as u32;
    let fragment_byte = match (error.phase, error.fragment_offset) {
        (
            Some(bun_runner::BunPreprocessPhase::Script | bun_runner::BunPreprocessPhase::Style),
            Some(offset),
        ) => utf16_offset_to_byte(source, offset).unwrap_or(0),
        _ => 0,
    };
    let position_source = &source[fragment_byte as usize..];
    let index = LineIndex::new(position_source);
    let position_offset = |position: Option<bun_runner::BunPreprocessPosition>| {
        position.and_then(|position| {
            position
                .line
                .zip(position.column)
                .and_then(|(line, column)| {
                    index.offset_utf16(LineCol {
                        line: line.saturating_sub(1),
                        col: column,
                    })
                })
                .map(|offset| fragment_byte + u32::from(offset))
                .or_else(|| {
                    position
                        .offset
                        .and_then(|offset| utf16_offset_to_byte(position_source, offset))
                        .map(|offset| fragment_byte + offset)
                })
        })
    };

    let start = position_offset(error.start).unwrap_or(0).min(source_len);
    let end = position_offset(error.end)
        .unwrap_or(start)
        .clamp(start, source_len);
    Span::new(start, end)
}

fn utf16_offset_to_byte(source: &str, target: u32) -> Option<u32> {
    let mut utf16_offset = 0u32;
    for (byte_offset, character) in source.char_indices() {
        if utf16_offset == target {
            return Some(byte_offset as u32);
        }
        utf16_offset += character.len_utf16() as u32;
        if utf16_offset > target {
            return None;
        }
    }

    (utf16_offset == target).then_some(source.len() as u32)
}

fn map_svelte_diagnostics(
    diagnostics: &mut [svelte_diagnostics::Diagnostic],
    map: &PreprocessorMap,
    original_path: &Utf8Path,
    processed_source: &str,
    original_source: &str,
) {
    let processed_index = LineIndex::new(processed_source);
    let original_index = LineIndex::new(original_source);

    for diagnostic in diagnostics {
        if let Some(span) = map_preprocessed_span(
            diagnostic.span,
            map,
            original_path,
            &processed_index,
            &original_index,
        ) {
            diagnostic.span = span;
        } else {
            let original_code = diagnostic.code;
            diagnostic.code = svelte_diagnostics::DiagnosticCode::PreprocessError;
            diagnostic.severity = Severity::Error;
            diagnostic.message = format!(
                "Unable to map {} diagnostic through the preprocessor source map: {}",
                original_code, diagnostic.message
            );
            diagnostic.span = Span::empty(0u32);
            diagnostic.suggestions.clear();
            continue;
        }
        let mut all_suggestions_mapped = true;
        for suggestion in &mut diagnostic.suggestions {
            if let Some(span) = map_preprocessed_span(
                suggestion.span,
                map,
                original_path,
                &processed_index,
                &original_index,
            ) {
                suggestion.span = span;
            } else {
                all_suggestions_mapped = false;
            }
        }
        if !all_suggestions_mapped {
            diagnostic.suggestions.clear();
        }
    }
}

fn map_compiler_diagnostics(
    diagnostics: &mut [BunDiagnostic],
    maps: &HashMap<Utf8PathBuf, PreprocessorMap>,
) {
    for diagnostic in diagnostics {
        let Some(map) = maps.get(&diagnostic.file) else {
            continue;
        };
        let map_position = |position: bun_runner::BunPosition| {
            map.original_position_in(
                LineCol {
                    line: position.line.saturating_sub(1),
                    col: position.column.saturating_sub(1),
                },
                &diagnostic.file,
            )
            .map(|original| bun_runner::BunPosition {
                line: original.line + 1,
                column: original.col + 1,
            })
        };
        match (map_position(diagnostic.start), map_position(diagnostic.end)) {
            (Some(start), Some(end)) => {
                diagnostic.start = start;
                diagnostic.end = if (end.line, end.column) >= (start.line, start.column) {
                    end
                } else {
                    start
                };
            }
            (Some(start), None) => {
                diagnostic.start = start;
                diagnostic.end = start;
            }
            _ => {
                diagnostic.code = "preprocess-error".to_string();
                diagnostic.message = format!(
                    "Unable to map compiler diagnostic through the preprocessor source map: {}",
                    diagnostic.message
                );
                diagnostic.severity = BunDiagnosticSeverity::Error;
                diagnostic.start = bun_runner::BunPosition { line: 1, column: 1 };
                diagnostic.end = diagnostic.start;
            }
        }
    }
}

/// Runs Svelte compiler diagnostics using bun.
async fn run_bun_check(
    workspace: &Utf8Path,
    inputs: Vec<BunInput>,
) -> Result<Vec<BunDiagnostic>, OrchestratorError> {
    let bun_path = BunRunner::ensure_bun(Some(workspace))
        .await
        .map_err(|e| OrchestratorError::BunError(e.to_string()))?;

    let worker_count = bun_worker_count();
    let runner = BunRunner::new(bun_path, workspace.to_owned(), worker_count)
        .map_err(|e| OrchestratorError::BunError(e.to_string()))?;

    runner
        .check_files(inputs)
        .await
        .map_err(|e| OrchestratorError::BunError(e.to_string()))
}

fn bun_worker_count() -> usize {
    let from_env = std::env::var("SVELTE_CHECK_RS_BUN_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0);

    if let Some(count) = from_env {
        return count;
    }

    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    std::cmp::min(available, 4)
}

/// Formats TypeScript diagnostics for output.
///
/// Positionless tsconfig diagnostics (`position_unknown`, e.g. options/global
/// errors like `error TS2318`) carry a zero line/column and an absolute
/// tsconfig path; they are intentionally printed at `line/col 0` with no source
/// snippet (this formatter never renders snippets), which is the scr analog of
/// upstream's `writers.ts` `positionUnknown` suppression. The absolute tsconfig
/// path is stripped to a workspace-relative path below (e.g. `tsconfig.json`).
fn format_ts_diagnostics(
    diagnostics: &[TsgoDiagnostic],
    workspace: &Utf8Path,
    format: crate::cli::OutputFormat,
) -> String {
    let mut output = String::new();

    for diag in diagnostics {
        let relative_file = diag
            .file
            .strip_prefix(workspace)
            .unwrap_or(&diag.file)
            .to_string();

        let severity = match diag.severity {
            tsgo_runner::DiagnosticSeverity::Error => "Error",
            tsgo_runner::DiagnosticSeverity::Warning => "Warning",
            tsgo_runner::DiagnosticSeverity::Suggestion => "Hint",
        };

        match format {
            crate::cli::OutputFormat::Human | crate::cli::OutputFormat::HumanVerbose => {
                output.push_str(&format!(
                    "{}:{}:{}\n{}: {} (ts({}))\n\n",
                    relative_file,
                    diag.start.line,
                    diag.start.column,
                    severity,
                    diag.message,
                    diag.code
                ));
            }
            crate::cli::OutputFormat::Machine => {
                output.push_str(&format!(
                    "{} {}:{}:{}:{}:{} {} (ts({}))\n",
                    severity.to_uppercase(),
                    relative_file,
                    diag.start.line,
                    diag.start.column,
                    diag.end.line,
                    diag.end.column,
                    diag.message,
                    diag.code
                ));
            }
            crate::cli::OutputFormat::Json => {
                // JSON format handled separately to produce valid JSON array
            }
        }
    }

    output
}

/// Formats TypeScript diagnostics into JSON-ready structs.
fn format_ts_diagnostics_json(
    diagnostics: &[TsgoDiagnostic],
    workspace: &Utf8Path,
) -> Vec<FormattedDiagnostic> {
    diagnostics
        .iter()
        .map(|diag| {
            let relative_file = diag
                .file
                .strip_prefix(workspace)
                .unwrap_or(&diag.file)
                .to_string();

            let severity = match diag.severity {
                tsgo_runner::DiagnosticSeverity::Error => "Error",
                tsgo_runner::DiagnosticSeverity::Warning => "Warning",
                tsgo_runner::DiagnosticSeverity::Suggestion => "Hint",
            };

            FormattedDiagnostic {
                diagnostic_type: severity.to_string(),
                filename: relative_file,
                start: Position {
                    line: diag.start.line,
                    column: diag.start.column,
                    offset: diag.start.offset,
                },
                end: Position {
                    line: diag.end.line,
                    column: diag.end.column,
                    offset: diag.end.offset,
                },
                message: diag.message.clone(),
                code: diag.code.clone(),
                source: "ts".to_string(),
            }
        })
        .collect()
}

/// Formats Svelte compiler diagnostics for output.
fn format_compiler_diagnostics(
    diagnostics: &[BunDiagnostic],
    workspace: &Utf8Path,
    format: crate::cli::OutputFormat,
) -> String {
    let mut output = String::new();

    for diag in diagnostics {
        let relative_file = diag
            .file
            .strip_prefix(workspace)
            .unwrap_or(&diag.file)
            .to_string();

        let severity = match diag.severity {
            BunDiagnosticSeverity::Error => "Error",
            BunDiagnosticSeverity::Warning => "Warning",
        };

        match format {
            crate::cli::OutputFormat::Human | crate::cli::OutputFormat::HumanVerbose => {
                output.push_str(&format!(
                    "{}:{}:{}\n{}: {} ({})\n\n",
                    relative_file,
                    diag.start.line,
                    diag.start.column,
                    severity,
                    diag.message,
                    diag.code
                ));
            }
            crate::cli::OutputFormat::Machine => {
                output.push_str(&format!(
                    "{} {}:{}:{}:{}:{} {} ({})\n",
                    severity.to_uppercase(),
                    relative_file,
                    diag.start.line,
                    diag.start.column,
                    diag.end.line,
                    diag.end.column,
                    diag.message,
                    diag.code
                ));
            }
            crate::cli::OutputFormat::Json => {
                // JSON format handled separately
            }
        }
    }

    output
}

/// Formats Svelte compiler diagnostics into JSON-ready structs.
fn format_compiler_diagnostics_json(
    diagnostics: &[BunDiagnostic],
    workspace: &Utf8Path,
    sources: &HashMap<Utf8PathBuf, String>,
) -> Vec<FormattedDiagnostic> {
    let source_indexes: HashMap<_, _> = sources
        .iter()
        .map(|(path, source)| (path, LineIndex::new(source)))
        .collect();
    diagnostics
        .iter()
        .map(|diag| {
            let relative_file = diag
                .file
                .strip_prefix(workspace)
                .unwrap_or(&diag.file)
                .to_string();

            let severity = match diag.severity {
                BunDiagnosticSeverity::Error => "Error",
                BunDiagnosticSeverity::Warning => "Warning",
            };

            let offset = source_indexes
                .get(&diag.file)
                .and_then(|index| {
                    index.offset_utf16(LineCol {
                        line: diag.start.line.saturating_sub(1),
                        col: diag.start.column.saturating_sub(1),
                    })
                })
                .map(u32::from)
                .unwrap_or(0);
            let end_offset = source_indexes
                .get(&diag.file)
                .and_then(|index| {
                    index.offset_utf16(LineCol {
                        line: diag.end.line.saturating_sub(1),
                        col: diag.end.column.saturating_sub(1),
                    })
                })
                .map(u32::from)
                .unwrap_or(offset);

            FormattedDiagnostic {
                diagnostic_type: severity.to_string(),
                filename: relative_file,
                start: Position {
                    line: diag.start.line,
                    column: diag.start.column,
                    offset,
                },
                end: Position {
                    line: diag.end.line,
                    column: diag.end.column,
                    offset: end_offset,
                },
                message: diag.message.clone(),
                code: diag.code.clone(),
                source: "svelte".to_string(),
            }
        })
        .collect()
}

fn include_svelte_severity(severity: Severity, threshold: crate::cli::Threshold) -> bool {
    match threshold {
        crate::cli::Threshold::Error => matches!(severity, Severity::Error),
        crate::cli::Threshold::Warning => true,
    }
}

fn include_compiler_severity(
    severity: BunDiagnosticSeverity,
    threshold: crate::cli::Threshold,
) -> bool {
    match threshold {
        crate::cli::Threshold::Error => matches!(severity, BunDiagnosticSeverity::Error),
        crate::cli::Threshold::Warning => true,
    }
}

fn include_ts_severity(
    severity: tsgo_runner::DiagnosticSeverity,
    threshold: crate::cli::Threshold,
) -> bool {
    match threshold {
        crate::cli::Threshold::Error => {
            matches!(severity, tsgo_runner::DiagnosticSeverity::Error)
        }
        crate::cli::Threshold::Warning => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompilerWarningLevel {
    Ignore,
    Error,
}

fn parse_compiler_warnings(
    raw: Option<&str>,
) -> Result<HashMap<String, CompilerWarningLevel>, OrchestratorError> {
    let Some(raw) = raw else {
        return Ok(HashMap::new());
    };

    let map: HashMap<String, String> = serde_json::from_str(raw)
        .map_err(|e| OrchestratorError::CompilerConfigError(e.to_string()))?;

    let mut out = HashMap::new();
    for (code, level) in map {
        match level.to_ascii_lowercase().as_str() {
            "ignore" => {
                out.insert(code, CompilerWarningLevel::Ignore);
            }
            "error" => {
                out.insert(code, CompilerWarningLevel::Error);
            }
            _ => {}
        }
    }
    Ok(out)
}

fn apply_compiler_warning_settings(
    diagnostics: &mut Vec<BunDiagnostic>,
    settings: &HashMap<String, CompilerWarningLevel>,
) {
    diagnostics.retain_mut(|diag| {
        if let Some(level) = settings.get(&diag.code) {
            match level {
                CompilerWarningLevel::Ignore => return false,
                CompilerWarningLevel::Error => {
                    diag.severity = BunDiagnosticSeverity::Error;
                }
            }
        }
        true
    });
}

fn duration_ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[allow(clippy::too_many_arguments)]
fn timings_json(
    file_scan_time: Option<std::time::Duration>,
    preprocess_time: std::time::Duration,
    svelte_time: std::time::Duration,
    file_count: usize,
    transformed_count: usize,
    compiler_total_time: Option<std::time::Duration>,
    sveltekit_sync_time: Option<std::time::Duration>,
    sveltekit_sync_ran: Option<bool>,
    tsgo_total_time: Option<std::time::Duration>,
    tsgo_stats: Option<&TsgoCheckStats>,
    total_time: std::time::Duration,
) -> String {
    let mut root = serde_json::Map::new();
    root.insert(
        "file_scan_ms".to_string(),
        file_scan_time
            .map(duration_ms)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    root.insert(
        "preprocess_ms".to_string(),
        serde_json::Value::from(duration_ms(preprocess_time)),
    );
    root.insert(
        "svelte_ms".to_string(),
        serde_json::Value::from(duration_ms(svelte_time)),
    );
    root.insert(
        "file_count".to_string(),
        serde_json::Value::from(file_count as u64),
    );
    root.insert(
        "transformed_count".to_string(),
        serde_json::Value::from(transformed_count as u64),
    );
    root.insert(
        "compiler_ms".to_string(),
        compiler_total_time
            .map(duration_ms)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    root.insert(
        "sveltekit_sync_ms".to_string(),
        sveltekit_sync_time
            .map(duration_ms)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    root.insert(
        "sveltekit_sync_ran".to_string(),
        sveltekit_sync_ran
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    root.insert(
        "tsgo_total_ms".to_string(),
        tsgo_total_time
            .map(duration_ms)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );

    if let Some(stats) = tsgo_stats {
        root.insert(
            "tsgo_cache".to_string(),
            serde_json::json!({
                "tsx_written": stats.cache.tsx_written,
                "tsx_skipped": stats.cache.tsx_skipped,
                "stub_written": stats.cache.stub_written,
                "stub_skipped": stats.cache.stub_skipped,
                "kit_written": stats.cache.kit_written,
                "kit_skipped": stats.cache.kit_skipped,
                "patched_written": stats.cache.patched_written,
                "patched_skipped": stats.cache.patched_skipped,
                "tsconfig_written": stats.cache.tsconfig_written,
                "tsconfig_skipped": stats.cache.tsconfig_skipped,
                "source_entries": stats.cache.source_entries,
                "source_files": stats.cache.source_files,
                "source_dirs": stats.cache.source_dirs,
                "source_svelte_skipped": stats.cache.source_svelte_skipped,
                "source_existing_skipped": stats.cache.source_existing_skipped,
                "source_linked": stats.cache.source_linked,
                "source_copied": stats.cache.source_copied,
                "stale_removed": stats.cache.stale_removed
            }),
        );
        root.insert(
            "tsgo_timings_ms".to_string(),
            serde_json::json!({
                "write": duration_ms(stats.timings.write_time),
                "source_tree": duration_ms(stats.timings.source_tree_time),
                "tsconfig": duration_ms(stats.timings.tsconfig_time),
                "tsgo": duration_ms(stats.timings.tsgo_time),
                "parse": duration_ms(stats.timings.parse_time)
            }),
        );
    } else {
        root.insert("tsgo_cache".to_string(), serde_json::Value::Null);
        root.insert("tsgo_timings_ms".to_string(), serde_json::Value::Null);
    }

    root.insert(
        "total_ms".to_string(),
        serde_json::Value::from(duration_ms(total_time)),
    );

    serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .unwrap_or_else(|_| "{}".to_string())
}

fn read_env_bool(name: &str) -> Option<bool> {
    let value = std::env::var(name).ok()?;
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

trait WatchBackend {
    fn watch_directory(&mut self, path: &Utf8Path) -> notify::Result<()>;
    fn unwatch_directory(&mut self, path: &Utf8Path) -> notify::Result<()>;
}

impl<T: notify::Watcher> WatchBackend for T {
    fn watch_directory(&mut self, path: &Utf8Path) -> notify::Result<()> {
        notify::Watcher::watch(
            self,
            path.as_std_path(),
            notify::RecursiveMode::NonRecursive,
        )
    }

    fn unwatch_directory(&mut self, path: &Utf8Path) -> notify::Result<()> {
        notify::Watcher::unwatch(self, path.as_std_path())
    }
}

#[derive(Debug, Default)]
struct WatchReconciler {
    logical_files: HashSet<Utf8PathBuf>,
    watched_directories: HashSet<Utf8PathBuf>,
}

impl WatchReconciler {
    fn reconcile<B: WatchBackend>(
        &mut self,
        backend: &mut B,
        logical_files: HashSet<Utf8PathBuf>,
    ) -> notify::Result<()> {
        let mut desired_directories: HashSet<_> = logical_files
            .iter()
            .filter_map(|file| nearest_existing_parent(file))
            .collect();
        if cfg!(windows) {
            // ReadDirectoryChangesW observes entries inside a watched
            // directory, not a rename of that directory in its parent.
            desired_directories.extend(
                desired_directories
                    .iter()
                    .filter_map(|directory| directory.parent().map(Utf8Path::to_owned))
                    .collect::<Vec<_>>(),
            );
        }

        let mut additions: Vec<_> = desired_directories
            .difference(&self.watched_directories)
            .cloned()
            .collect();
        let mut removals: Vec<_> = self
            .watched_directories
            .difference(&desired_directories)
            .cloned()
            .collect();
        additions.sort();
        removals.sort();

        // Add replacement coverage before removing stale directories so a
        // missing dependency becoming nested cannot create a blind window.
        for directory in additions {
            backend.watch_directory(&directory)?;
            self.watched_directories.insert(directory);
        }
        for directory in removals {
            match backend.unwatch_directory(&directory) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind,
                        notify::ErrorKind::PathNotFound | notify::ErrorKind::WatchNotFound
                    ) => {}
                Err(error) => return Err(error),
            }
            self.watched_directories.remove(&directory);
        }

        self.logical_files = logical_files;
        Ok(())
    }

    fn event_path_is_relevant(&self, changed: &Utf8Path) -> bool {
        self.logical_files.iter().any(|logical| {
            paths_match(logical, changed)
                || (logical.starts_with(changed)
                    && (!self.watched_directories.contains(changed) || !changed.exists()))
        })
    }
}

fn nearest_existing_parent(file: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut directory = file.parent();
    while let Some(candidate) = directory {
        if candidate.is_dir() {
            return Some(candidate.to_owned());
        }
        directory = candidate.parent();
    }
    None
}

fn config_candidate_paths(workspace: &Utf8Path) -> HashSet<Utf8PathBuf> {
    let mut candidates = HashSet::new();
    let mut directory = Some(workspace);
    while let Some(current) = directory {
        candidates.extend(CONFIG_FILENAMES.iter().map(|name| current.join(name)));
        directory = current.parent();
    }
    candidates
}

fn config_candidates_changed(
    workspace: &Utf8Path,
    changed_paths: &HashSet<Utf8PathBuf>,
    config_candidates: &HashSet<Utf8PathBuf>,
) -> bool {
    changed_paths.iter().any(|changed| {
        config_candidates
            .iter()
            .map(|candidate| normalize_dependency_path(workspace, candidate))
            .any(|candidate| paths_match(&candidate, changed))
    })
}

fn watch_files(
    initial_files: &[Utf8PathBuf],
    config_candidates: &HashSet<Utf8PathBuf>,
    dependencies: &HashSet<Utf8PathBuf>,
) -> HashSet<Utf8PathBuf> {
    initial_files
        .iter()
        .chain(config_candidates)
        .chain(dependencies)
        .cloned()
        .collect()
}

/// Runs in watch mode.
async fn run_watch_mode(
    args: &Args,
    workspace: &Utf8Path,
    initial_files: Vec<Utf8PathBuf>,
    file_scan_time: Option<std::time::Duration>,
    use_nodenext_imports: bool,
    mut svelte_run_config: SvelteRunConfig,
    extra_paths: &HashMap<String, Vec<String>>,
) -> Result<CheckSummary, OrchestratorError> {
    use notify::{Config, RecommendedWatcher, Watcher};
    use std::time::Duration;

    println!("Starting watch mode...\n");
    let mut active_extra_paths = extra_paths.clone();

    // Initial check
    let initial_run = run_single_check(
        args,
        workspace,
        initial_files.clone(),
        file_scan_time,
        use_nodenext_imports,
        &mut svelte_run_config,
        &active_extra_paths,
    )
    .await?;
    let mut dependencies = initial_run.dependencies;

    // Set up file watcher with tokio channel
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(1)),
    )
    .map_err(|e| OrchestratorError::WatchFailed(e.to_string()))?;

    // Parent-directory watches survive atomic saves and renames. Reconcile the
    // full logical set after every pass so removed dependencies release their
    // watcher and missing paths remain covered by their nearest existing
    // ancestor.
    let config_candidates = config_candidate_paths(workspace);
    let mut watch_reconciler = WatchReconciler::default();
    watch_reconciler
        .reconcile(
            &mut watcher,
            watch_files(&initial_files, &config_candidates, &dependencies),
        )
        .map_err(|error| OrchestratorError::WatchFailed(error.to_string()))?;

    println!("Watching for changes... (Ctrl+C to stop)\n");

    while let Some(event) = rx.recv().await {
        // Check if any Svelte files changed (.svelte, .svelte.ts, .svelte.js)
        let svelte_changed = event.paths.iter().any(|p| {
            let path_str = p.to_string_lossy();
            path_str.ends_with(".svelte")
                || path_str.ends_with(".svelte.ts")
                || path_str.ends_with(".svelte.js")
        });
        let changed_paths: HashSet<_> = event
            .paths
            .iter()
            .filter_map(|path| Utf8PathBuf::try_from(path.clone()).ok())
            .map(|path| normalize_dependency_path(workspace, &path))
            .collect();
        let dependency_changed = changed_paths.iter().any(|path| {
            dependencies
                .iter()
                .any(|dependency| paths_match(dependency, path))
                || watch_reconciler.event_path_is_relevant(path)
        });
        let config_candidates_changed =
            config_candidates_changed(workspace, &changed_paths, &config_candidates);
        let config_changed = config_candidates_changed
            || changed_paths.iter().any(|path| {
                svelte_run_config
                    .config_dependencies
                    .iter()
                    .any(|dependency| paths_match(dependency, path))
            });

        if svelte_changed || dependency_changed || config_changed {
            if !args.preserve_watch_output {
                // Clear screen
                print!("\x1B[2J\x1B[1;1H");
            }

            println!("File changed, re-checking...\n");

            if config_changed {
                match run_bun_load_config(workspace).await {
                    Ok((loaded, session)) => {
                        apply_loaded_config(
                            workspace,
                            &mut svelte_run_config,
                            &mut active_extra_paths,
                            loaded,
                            session,
                        );
                    }
                    Err(error) => eprintln!("Failed to reload Svelte config: {error}"),
                }
            }

            // Re-run check and refresh the dynamically returned dependency set.
            let mut check_result = run_single_check(
                args,
                workspace,
                initial_files.clone(),
                file_scan_time,
                use_nodenext_imports,
                &mut svelte_run_config,
                &active_extra_paths,
            )
            .await;
            if matches!(&check_result, Err(OrchestratorError::BunError(_)))
                && svelte_run_config.config_session.is_some()
            {
                if let Err(error) = &check_result {
                    eprintln!(
                        "Configured preprocessor worker failed; recreating its config session: {error}"
                    );
                }
                if let Ok((loaded, session)) = run_bun_load_config(workspace).await {
                    apply_loaded_config(
                        workspace,
                        &mut svelte_run_config,
                        &mut active_extra_paths,
                        loaded,
                        session,
                    );
                    check_result = run_single_check(
                        args,
                        workspace,
                        initial_files.clone(),
                        file_scan_time,
                        use_nodenext_imports,
                        &mut svelte_run_config,
                        &active_extra_paths,
                    )
                    .await;
                }
            }

            match check_result {
                Ok(check_run) => {
                    if check_run.dependencies_complete {
                        dependencies = check_run.dependencies;
                    } else {
                        dependencies.extend(check_run.dependencies);
                    }
                    if let Err(error) = watch_reconciler.reconcile(
                        &mut watcher,
                        watch_files(&initial_files, &config_candidates, &dependencies),
                    ) {
                        eprintln!("Failed to refresh preprocessor dependency watches: {error}");
                    }
                }
                Err(error) => eprintln!("Watch recheck failed: {error}"),
            }
        }
    }

    Err(OrchestratorError::WatchFailed(
        "watch channel closed unexpectedly".to_string(),
    ))
}

/// Converts a 1-indexed line and column to a byte offset in the source.
fn line_column_to_offset(source: &str, line: usize, column: usize) -> u32 {
    let mut current_line = 1;
    let mut current_offset = 0;

    for (i, ch) in source.char_indices() {
        if current_line == line {
            // Found the target line, now count columns
            for (col, (j, c)) in (1..).zip(source[i..].char_indices()) {
                if col == column {
                    return (i + j) as u32;
                }
                if c == '\n' {
                    break;
                }
            }
            // Column not found, return start of line
            return i as u32;
        }
        if ch == '\n' {
            current_line += 1;
        }
        current_offset = i + ch.len_utf8();
    }

    // Line not found, return end of file
    current_offset as u32
}

/// Converts a 1-indexed line and UTF-16 column to a UTF-8 byte offset.
#[cfg(test)]
fn utf16_line_column_to_offset(source: &str, line: usize, column: usize) -> u32 {
    LineIndex::new(source)
        .offset_utf16(LineCol {
            line: line.saturating_sub(1) as u32,
            col: column.saturating_sub(1) as u32,
        })
        .map(u32::from)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_workspace() {
        // Test that relative paths are resolved correctly
        let workspace = Utf8PathBuf::from(".");
        assert!(workspace.is_relative());
    }

    #[test]
    fn test_relative_import_path_uses_forward_slashes() {
        // Module specifiers must always use '/' — even on Windows, where
        // `Utf8PathBuf::push` would otherwise produce '\' that TypeScript
        // treats as an escape sequence in the import string.
        let from = Utf8PathBuf::from("src/ui/useFoo.svelte.ts");
        let to = Utf8Path::new(SHARED_HELPERS_MODULE);
        let result = relative_import_path(&from, to);
        assert_eq!(result, "../../__svelte_check_rs_helpers");
        assert!(!result.contains('\\'));
    }

    #[test]
    fn test_relative_import_path_deep_nesting() {
        let from = Utf8PathBuf::from("src/lib/components/accordion/accordion.svelte.ts");
        let to = Utf8Path::new(SHARED_HELPERS_MODULE);
        let result = relative_import_path(&from, to);
        assert_eq!(result, "../../../../__svelte_check_rs_helpers");
        assert!(!result.contains('\\'));
    }

    #[test]
    fn test_relative_import_path_same_directory() {
        let from = Utf8PathBuf::from("Foo.svelte.ts");
        let to = Utf8Path::new(SHARED_HELPERS_MODULE);
        let result = relative_import_path(&from, to);
        assert_eq!(result, "./__svelte_check_rs_helpers");
    }

    #[test]
    fn test_helpers_import_path_for_nested() {
        let virtual_path = Utf8PathBuf::from("src/ui/useFoo.svelte.ts");
        let path = helpers_import_path_for(&virtual_path, false);
        assert_eq!(path, "../../__svelte_check_rs_helpers");
        assert!(!path.contains('\\'));
    }

    #[test]
    fn test_helpers_import_path_for_nodenext() {
        let virtual_path = Utf8PathBuf::from("src/ui/useFoo.svelte.ts");
        let path = helpers_import_path_for(&virtual_path, true);
        assert_eq!(path, "../../__svelte_check_rs_helpers.js");
    }

    #[test]
    fn test_virtual_path_for_uses_forward_slashes() {
        // The parser-side back-mapping (`strip_cache_prefix` in tsgo-runner)
        // always normalizes to '/'; the insertion key must match or the
        // HashMap fast-path always misses on Windows.
        let workspace = Utf8PathBuf::from("/workspace");
        let file = workspace.join("src/lib/Component.svelte");
        let key = virtual_path_for(&file, &workspace, true);
        assert_eq!(key.as_str(), "src/lib/Component.svelte.ts");
        assert!(!key.as_str().contains('\\'));
    }

    #[test]
    fn test_virtual_path_for_module_no_ts_suffix() {
        let workspace = Utf8PathBuf::from("/workspace");
        let file = workspace.join("src/lib/Foo.svelte.ts");
        let key = virtual_path_for(&file, &workspace, false);
        assert_eq!(key.as_str(), "src/lib/Foo.svelte.ts");
    }

    #[test]
    fn test_normalize_lexical_strips_dot_and_dotdot() {
        // Regression (careswitch monorepo): `--workspace ./apps/x` becomes
        // `current_dir()/./apps/x` with an embedded `/./`. The cache path is
        // clean, so the #2942 import rewrite compared mismatched paths and
        // mangled in-workspace imports. The workspace root must be normalized.
        //
        // Compare in forward-slash form so the assertions hold on Windows too
        // (camino joins with `\` there; the `.`/`..` collapsing is what matters).
        let norm = |p: &str| {
            normalize_lexical(Utf8Path::new(p))
                .as_str()
                .replace('\\', "/")
        };
        assert_eq!(norm("/repo/./apps/x"), "/repo/apps/x");
        assert_eq!(norm("/a/b/../c"), "/a/c");
        assert_eq!(norm("/a/./b/./c"), "/a/b/c");
        assert_eq!(norm("a/../b"), "b");
        // A clean absolute path is unchanged.
        assert_eq!(norm("/Users/x/apps/web"), "/Users/x/apps/web");
    }

    #[test]
    fn test_is_ignored_dir_matches_native_separator_input() {
        // Simulate the Windows case: globset patterns use '/' but WalkDir
        // hands native-separator paths.  Build a pattern set that should
        // match `src/excluded/...` and pass a backslash-bearing relative
        // path through is_ignored_dir.
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("src/excluded/**").expect("glob"));
        let set = builder.build().expect("globset");

        // Forward-slash input always matched; the new code must also match
        // the backslash form that WalkDir produces on Windows.
        let backslash_path = Utf8PathBuf::from("src\\excluded\\nested");
        assert!(
            is_ignored_dir(&set, &backslash_path),
            "globset must match backslash-bearing input after normalization"
        );

        let forward_path = Utf8PathBuf::from("src/excluded/nested");
        assert!(is_ignored_dir(&set, &forward_path));
    }

    #[test]
    fn test_normalize_ignore_patterns_splits_and_normalizes() {
        // svelte-check's documented syntax is a single comma-separated string.
        // It must split into per-directory patterns, each expanded to match
        // everything beneath the directory (#159).
        let out = normalize_ignore_patterns(&["src/excluded,build".to_string()]);
        assert_eq!(out, vec!["src/excluded/**", "build/**"]);

        // Whitespace around pieces, leading `./`, and empty pieces (trailing
        // commas / repeated flags) are all handled; explicit globs pass through.
        let out = normalize_ignore_patterns(&[
            " ./src/excluded , ".to_string(),
            "**/*.test.ts".to_string(),
            String::new(),
        ]);
        assert_eq!(out, vec!["src/excluded/**", "**/*.test.ts"]);
    }

    #[test]
    fn test_comma_separated_ignore_excludes_dirs() {
        // End-to-end at the unit level: the normalized patterns, once compiled
        // into a globset, must actually prune the ignored directories.
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in normalize_ignore_patterns(&["src/excluded,build".to_string()]) {
            builder.add(globset::Glob::new(&pattern).expect("glob"));
        }
        let set = builder.build().expect("globset");

        assert!(is_ignored_dir(&set, Utf8Path::new("src/excluded")));
        assert!(is_ignored_dir(&set, Utf8Path::new("build")));
        assert!(set.is_match("src/excluded/Excluded.svelte"));
        assert!(set.is_match("build/Built.svelte"));
        // A sibling directory that wasn't ignored stays in.
        assert!(!is_ignored_dir(&set, Utf8Path::new("src/keep")));
        assert!(!set.is_match("src/keep/Keep.svelte"));
    }

    #[test]
    fn test_to_forward_slash_idempotent_on_unix_shape() {
        let p = Utf8PathBuf::from("src/lib/foo.ts");
        assert_eq!(to_forward_slash(&p), "src/lib/foo.ts");
    }

    #[test]
    #[should_panic(expected = "`from_file` must be workspace-relative")]
    fn test_relative_import_path_rejects_absolute_from() {
        let abs = if cfg!(windows) {
            Utf8PathBuf::from("C:\\workspace\\src\\Foo.svelte.ts")
        } else {
            Utf8PathBuf::from("/workspace/src/Foo.svelte.ts")
        };
        let _ = relative_import_path(&abs, Utf8Path::new(SHARED_HELPERS_MODULE));
    }

    #[test]
    #[should_panic(expected = "`to` must be workspace-relative")]
    fn test_relative_import_path_rejects_absolute_to() {
        let rel = Utf8PathBuf::from("src/Foo.svelte.ts");
        let abs = if cfg!(windows) {
            Utf8PathBuf::from("C:\\workspace\\helpers")
        } else {
            Utf8PathBuf::from("/workspace/helpers")
        };
        let _ = relative_import_path(&rel, &abs);
    }

    #[test]
    fn test_line_column_to_offset() {
        let source = "line1\nline2\nline3";
        // Line 1, column 1 = offset 0
        assert_eq!(line_column_to_offset(source, 1, 1), 0);
        // Line 1, column 3 = offset 2 ('n')
        assert_eq!(line_column_to_offset(source, 1, 3), 2);
        // Line 2, column 1 = offset 6 ('l')
        assert_eq!(line_column_to_offset(source, 2, 1), 6);
        // Line 3, column 1 = offset 12 ('l')
        assert_eq!(line_column_to_offset(source, 3, 1), 12);
    }

    #[test]
    fn test_utf16_line_column_to_offset_after_non_bmp_character() {
        assert_eq!(utf16_line_column_to_offset("😀value", 1, 3), 4);
        assert_eq!(utf16_line_column_to_offset("x\n😀value", 2, 3), 6);
    }

    #[test]
    fn compiler_low_resolution_map_never_emits_column_zero() {
        let file = Utf8PathBuf::from("/workspace/input.svelte");
        let map = PreprocessorMap::parse(
            r#"{"version":3,"sources":["input.svelte"],"names":[],"mappings":"AAAA"}"#,
        )
        .expect("source map");
        let maps = HashMap::from([(file.clone(), map)]);
        let mut diagnostics = vec![BunDiagnostic {
            file,
            code: "fixture".to_string(),
            message: "fixture".to_string(),
            severity: BunDiagnosticSeverity::Error,
            start: bun_runner::BunPosition { line: 1, column: 6 },
            end: bun_runner::BunPosition { line: 1, column: 7 },
        }];
        map_compiler_diagnostics(&mut diagnostics, &maps);
        assert_eq!(diagnostics[0].start.column, 1);
        assert_eq!(diagnostics[0].end.column, 1);
    }

    #[test]
    fn test_preprocess_error_span_prefers_component_line_column() {
        let source = "<script>\n  const broken = ;\n</script>";
        let error = BunPreprocessError {
            message: "Expression expected.".to_string(),
            start: Some(bun_runner::BunPreprocessPosition {
                line: Some(2),
                column: Some(17),
                // Script preprocessors may supply a fragment-relative offset.
                offset: Some(0),
            }),
            end: None,
            phase: None,
            fragment_offset: None,
            file: None,
        };

        let span = preprocess_error_span(&error, source);
        let position = LineIndex::new(source)
            .utf16_line_col(span.start)
            .expect("mapped position");
        assert_eq!(position, LineCol::new(1, 17));
    }

    #[test]
    fn test_preprocess_error_span_converts_utf16_offset() {
        let source = "😀 const broken = ;";
        let error = BunPreprocessError {
            message: "Expression expected.".to_string(),
            start: Some(bun_runner::BunPreprocessPosition {
                line: None,
                column: None,
                offset: Some(18),
            }),
            end: None,
            phase: None,
            fragment_offset: None,
            file: None,
        };

        let span = preprocess_error_span(&error, source);
        assert_eq!(u32::from(span.start), 20);
    }

    #[test]
    fn test_preprocess_error_span_applies_script_fragment_offset() {
        let source = "<p>before</p>\n<p>again</p>\n<script>\nfirst\nsecond\n</script>";
        let fragment = source.find("\nfirst").expect("script content") as u32;
        let error = BunPreprocessError {
            message: "script failure".to_string(),
            start: Some(bun_runner::BunPreprocessPosition {
                line: Some(2),
                column: Some(1),
                offset: None,
            }),
            end: None,
            phase: Some(bun_runner::BunPreprocessPhase::Script),
            fragment_offset: Some(fragment),
            file: None,
        };
        let span = preprocess_error_span(&error, source);
        assert_eq!(
            LineIndex::new(source).utf16_line_col(span.start),
            Some(LineCol::new(3, 1))
        );
    }

    #[test]
    fn test_normalize_tsconfig_pattern() {
        // Issue #19: tsconfig exclude patterns should be properly normalized

        // Directory pattern without glob should get /** appended
        assert_eq!(
            normalize_tsconfig_pattern("src/excluded"),
            "src/excluded/**"
        );

        // Pattern already with ** should be unchanged
        assert_eq!(
            normalize_tsconfig_pattern("src/excluded/**"),
            "src/excluded/**"
        );

        // Leading ./ should be stripped
        assert_eq!(
            normalize_tsconfig_pattern("./src/excluded"),
            "src/excluded/**"
        );

        // Patterns starting with ** should be unchanged
        assert_eq!(normalize_tsconfig_pattern("**/*.test.ts"), "**/*.test.ts");

        // Patterns with * but no ** should be unchanged
        assert_eq!(normalize_tsconfig_pattern("src/*.test.ts"), "src/*.test.ts");
    }

    #[test]
    fn test_unsupported_extension_label_prefers_longest_user_extension() {
        let label = unsupported_extension_label("page.svx", &[".svx"]);
        assert_eq!(label, ".svx");

        // Longer configured extension should win over a shorter one.
        let label = unsupported_extension_label("page.svelte.md", &[".md", ".svelte.md"]);
        assert_eq!(label, ".svelte.md");
    }

    #[test]
    fn test_unsupported_extension_label_falls_back_to_path_extension() {
        // No user extension matches → fall back to the trailing dotted suffix.
        let label = unsupported_extension_label("notes.txt", &[]);
        assert_eq!(label, ".txt");
    }

    #[test]
    fn test_format_unsupported_warnings_groups_by_extension() {
        let files = vec![
            Utf8PathBuf::from("src/a.svx"),
            Utf8PathBuf::from("src/b.svx"),
            Utf8PathBuf::from("src/c.mdx"),
        ];
        let lines = format_unsupported_warnings(&files, &[".svx", ".mdx"]);
        assert_eq!(
            lines,
            vec![
                "warning: 1 file with unregistered extension (.mdx) skipped".to_string(),
                "warning: 2 files with unregistered extension (.svx) skipped".to_string(),
            ]
        );
    }

    #[test]
    fn test_format_unsupported_warnings_empty() {
        assert!(format_unsupported_warnings(&[], &[]).is_empty());
    }

    #[test]
    fn test_tsconfig_pattern_matching() {
        // Test that normalized patterns work with globset
        use globset::GlobBuilder;

        // Simulate the exclude pattern "src/excluded/**" matching
        let pattern = normalize_tsconfig_pattern("src/excluded");
        let glob = GlobBuilder::new(&pattern)
            .literal_separator(false)
            .build()
            .unwrap()
            .compile_matcher();

        // Should match files in the excluded directory
        assert!(glob.is_match("src/excluded/Test.svelte"));
        assert!(glob.is_match("src/excluded/nested/File.svelte"));

        // Should not match files outside the excluded directory
        assert!(!glob.is_match("src/routes/Page.svelte"));
        assert!(!glob.is_match("src/lib/Component.svelte"));
    }

    #[derive(Default)]
    struct FakeWatchBackend {
        watched: HashSet<Utf8PathBuf>,
        operations: Vec<(bool, Utf8PathBuf)>,
        missing_on_unwatch: bool,
    }

    impl WatchBackend for FakeWatchBackend {
        fn watch_directory(&mut self, path: &Utf8Path) -> notify::Result<()> {
            self.watched.insert(path.to_owned());
            self.operations.push((true, path.to_owned()));
            Ok(())
        }

        fn unwatch_directory(&mut self, path: &Utf8Path) -> notify::Result<()> {
            self.watched.remove(path);
            self.operations.push((false, path.to_owned()));
            if self.missing_on_unwatch {
                Err(notify::Error::new(notify::ErrorKind::WatchNotFound))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn watch_reconciler_removes_stale_dependency_directories() {
        let temp = tempfile::tempdir().expect("temp directory");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path");
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        let mut backend = FakeWatchBackend::default();
        let mut reconciler = WatchReconciler::default();
        reconciler
            .reconcile(
                &mut backend,
                HashSet::from([a.join("dependency.txt"), b.join("dependency.txt")]),
            )
            .unwrap();
        reconciler
            .reconcile(
                &mut backend,
                HashSet::from([b.join("renamed-dependency.txt")]),
            )
            .unwrap();

        let mut expected = HashSet::from([b]);
        if cfg!(windows) {
            expected.insert(root);
        }
        assert_eq!(backend.watched, expected);
        assert!(
            backend.operations.contains(&(false, a)),
            "stale directory was not unwatched"
        );
    }

    #[test]
    fn watch_reconciler_narrows_missing_path_coverage_without_a_gap() {
        let temp = tempfile::tempdir().expect("temp directory");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path");
        let dependency = root.join("new/nested/dependency.txt");
        let mut backend = FakeWatchBackend::default();
        let mut reconciler = WatchReconciler::default();
        let logical = HashSet::from([dependency.clone()]);

        reconciler.reconcile(&mut backend, logical.clone()).unwrap();
        let mut expected = HashSet::from([root.clone()]);
        if cfg!(windows) {
            expected.insert(root.parent().unwrap().to_owned());
        }
        assert_eq!(backend.watched, expected);
        assert!(reconciler.event_path_is_relevant(&root.join("new")));

        fs::create_dir_all(root.join("new/nested")).unwrap();
        let operation_start = backend.operations.len();
        reconciler.reconcile(&mut backend, logical).unwrap();
        let refresh = &backend.operations[operation_start..];
        let replacement_added = refresh
            .iter()
            .position(|operation| operation == &(true, root.join("new/nested")))
            .expect("replacement directory was not watched");
        let stale_removed = refresh
            .iter()
            .position(|operation| operation == &(false, root.clone()))
            .expect("stale directory was not unwatched");
        assert!(
            replacement_added < stale_removed,
            "replacement coverage must be added before stale coverage is removed"
        );
    }

    #[test]
    fn watch_reconciler_accepts_already_removed_backend_watches() {
        let temp = tempfile::tempdir().expect("temp directory");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        let mut backend = FakeWatchBackend::default();
        let mut reconciler = WatchReconciler::default();
        reconciler
            .reconcile(&mut backend, HashSet::from([child.join("dependency")]))
            .unwrap();
        backend.missing_on_unwatch = true;
        reconciler.reconcile(&mut backend, HashSet::new()).unwrap();
        assert!(reconciler.watched_directories.is_empty());
    }

    #[test]
    fn config_candidate_changes_require_an_exact_candidate_path() {
        let temp = tempfile::tempdir().expect("temp directory");
        let root = canonicalize_physical(
            &Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path"),
        );
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let workspace = normalize_dependency_path(&root, &workspace);
        let candidates = config_candidate_paths(&workspace);

        assert!(config_candidates_changed(
            &workspace,
            &HashSet::from([workspace.join("svelte.config.js")]),
            &candidates,
        ));
        assert!(!config_candidates_changed(
            &workspace,
            &HashSet::from([root.join("dependency/svelte.config.js")]),
            &candidates,
        ));
    }

    #[test]
    #[cfg(unix)]
    fn config_candidate_changes_normalize_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp directory");
        let root = canonicalize_physical(
            &Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path"),
        );
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let target = root.join("shared-config.js");
        fs::write(&target, "export default {};").unwrap();
        symlink(&target, workspace.join("svelte.config.js")).unwrap();

        assert!(config_candidates_changed(
            &workspace,
            &HashSet::from([target]),
            &config_candidate_paths(&workspace),
        ));
    }

    #[test]
    #[cfg(unix)]
    fn missing_dependency_paths_canonicalize_their_existing_ancestor() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp directory");
        let root = canonicalize_physical(
            &Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path"),
        );
        let physical = root.join("physical");
        fs::create_dir_all(&physical).unwrap();
        let alias = root.join("alias");
        symlink(&physical, &alias).unwrap();

        let normalized = normalize_dependency_path(&root, &alias.join("missing/file.scss"));
        assert_eq!(normalized, physical.join("missing/file.scss"));
    }

    #[test]
    #[cfg(windows)]
    fn windows_watches_dependency_directories_and_their_parents() {
        let temp = tempfile::tempdir().expect("temp directory");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).expect("utf-8 temp path");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        let mut backend = FakeWatchBackend::default();
        let mut reconciler = WatchReconciler::default();
        reconciler
            .reconcile(&mut backend, HashSet::from([child.join("dependency")]))
            .unwrap();

        assert!(backend.watched.contains(&child));
        assert!(backend.watched.contains(&root));
        assert!(paths_match(
            Utf8Path::new(r"C:\Project\Config.js"),
            Utf8Path::new(r"c:\project\config.js")
        ));
    }
}
