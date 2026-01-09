//! Main orchestration logic.

use crate::cli::{Args, TimingFormat};
use crate::config::{SvelteConfig, SvelteFileKind, TsConfig};
use crate::output::{CheckSummary, FormattedDiagnostic, Formatter, Position};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSetBuilder};
use rayon::prelude::*;
use source_map::LineIndex;
use std::collections::HashMap;
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
    if suffix_ts {
        Utf8PathBuf::from(format!("{}.ts", relative))
    } else {
        relative
    }
}

fn relative_import_path(from_file: &Utf8Path, to: &Utf8Path) -> String {
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

    let mut rel = Utf8PathBuf::new();
    for _ in common..from_components.len() {
        rel.push("..");
    }
    for comp in &to_components[common..] {
        rel.push(comp);
    }

    let mut rel_str = rel.as_str().to_string();
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

fn is_ignored_dir(ignore_set: &globset::GlobSet, relative: &Utf8Path) -> bool {
    let rel = relative.as_str();
    if ignore_set.is_match(rel) {
        return true;
    }
    let mut rel_slash = String::with_capacity(rel.len() + 1);
    rel_slash.push_str(rel);
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

    // Load configuration
    let svelte_config = SvelteConfig::load(&workspace);
    let extra_paths = svelte_alias_paths(&svelte_config);

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
        eprintln!("diagnostic_sources: {:?}", args.diagnostic_sources);
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
    for pattern in &args.ignore {
        let glob = Glob::new(pattern).map_err(|e| OrchestratorError::InvalidGlob(e.to_string()))?;
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
            !ignore_set.is_match(relative.as_str())
        })
        .collect();
    let file_scan_time = if timings_enabled {
        Some(scan_start.elapsed())
    } else {
        None
    };

    // Handle --single-file flag: filter to just the specified file
    let files = if let Some(ref single_file) = args.single_file {
        let target = if single_file.is_relative() {
            workspace.join(single_file)
        } else {
            single_file.clone()
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

    if args.watch {
        run_watch_mode(
            &args,
            &workspace,
            files,
            file_scan_time,
            use_nodenext_imports,
            &extra_paths,
        )
        .await
    } else {
        run_single_check(
            &args,
            &workspace,
            files,
            file_scan_time,
            use_nodenext_imports,
            &extra_paths,
        )
        .await
    }
}

/// Runs a single check pass.
async fn run_single_check(
    args: &Args,
    workspace: &Utf8Path,
    files: Vec<Utf8PathBuf>,
    file_scan_time: Option<std::time::Duration>,
    use_nodenext_imports: bool,
    extra_paths: &HashMap<String, Vec<String>>,
) -> Result<CheckSummary, OrchestratorError> {
    let total_start = Instant::now();
    let timings_enabled = args.timings
        || args.timings_format == TimingFormat::Json
        || read_env_bool("SVELTE_CHECK_RS_TIMINGS").unwrap_or(false);
    let formatter = Formatter::new(args.output);
    let output_json = matches!(args.output, crate::cli::OutputFormat::Json);
    let error_count = AtomicUsize::new(0);
    let warning_count = AtomicUsize::new(0);

    // Base diagnostic options (filename will be set per-file)
    let base_diag_options = DiagnosticOptions {
        a11y: args.include_svelte(),
        css: args.include_css(),
        component: args.include_svelte(),
        filename: None,
    };

    struct FileOutput {
        text: Option<String>,
        json: Vec<FormattedDiagnostic>,
    }

    struct FileResult {
        output: Option<FileOutput>,
        transformed: Option<(Utf8PathBuf, TransformedFile)>,
    }

    // Separate files by kind: components (.svelte) vs modules (.svelte.ts/.svelte.js)
    let (component_files, module_files): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|f| SvelteFileKind::from_path(f) == Some(SvelteFileKind::Component));

    let svelte_start = Instant::now();

    // Process component files (.svelte) in parallel: parse, run Svelte diagnostics, and transform
    let component_results: Vec<FileResult> = component_files
        .par_iter()
        .map(|file_path| {
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", file_path, e);
                    return FileResult {
                        output: None,
                        transformed: None,
                    };
                }
            };

            // Parse the file
            let parse_result = parse(&source);

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

            // Convert parse errors to diagnostics
            for error in &parse_result.errors {
                all_diagnostics.push(svelte_diagnostics::Diagnostic::new(
                    svelte_diagnostics::DiagnosticCode::ParseError,
                    error.to_string(),
                    error.span,
                ));
            }

            // Run Svelte diagnostics with filename for component checks
            let file_diag_options = base_diag_options
                .clone()
                .with_filename(file_path.to_string());
            let svelte_diags = check_svelte(&parse_result.document, file_diag_options);
            all_diagnostics.extend(svelte_diags);

            all_diagnostics.retain(|diag| include_svelte_severity(diag.severity, args.threshold));

            // Transform for TypeScript checking (if JS diagnostics enabled and not skipping tsgo)
            // Also transform if emit_ts or emit_source_map is enabled (for debugging)
            let mut transformed = None;
            let should_transform =
                (args.include_js() && !args.skip_tsgo) || args.emit_ts || args.emit_source_map;
            if should_transform {
                let virtual_path = virtual_path_for(file_path, workspace, true);
                let helpers_import = helpers_import_path_for(&virtual_path, use_nodenext_imports);
                let transform_options = TransformOptions {
                    filename: Some(file_path.to_string()),
                    source_maps: true,
                    use_nodenext_imports,
                    helpers_import_path: Some(helpers_import),
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
                if args.include_js() && !args.skip_tsgo {
                    // Create the virtual path (original.svelte -> original.svelte.ts)
                    let virtual_path = virtual_path_for(file_path, workspace, true);

                    let tsx_code = transform_result.tsx_code;
                    let transformed_file = TransformedFile {
                        original_path: file_path.clone(),
                        generated_line_index: LineIndex::new(&tsx_code),
                        tsx_content: tsx_code,
                        source_map: transform_result.source_map,
                        original_line_index: LineIndex::new(&source),
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
                output,
                transformed,
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
                        output: None,
                        transformed: None,
                    };
                }
            };

            // Transform module file (runes only, no template/styles)
            let virtual_path = virtual_path_for(file_path, workspace, false);
            let helpers_import = helpers_import_path_for(&virtual_path, use_nodenext_imports);
            let transform_result =
                transform_module(&source, Some(file_path.as_str()), Some(helpers_import));

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
            if args.include_js() {
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
                    generated_line_index: LineIndex::new(&tsx_code),
                    tsx_content: tsx_code,
                    source_map: transform_result.source_map,
                    original_line_index: LineIndex::new(&source),
                };

                transformed = Some((virtual_path, transformed_file));
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
                output,
                transformed,
            }
        })
        .collect();

    // Combine outputs and transformed files from both component and module files
    let mut outputs: Vec<FileOutput> = Vec::new();
    let mut transformed_files = TransformedFiles::new();
    for result in component_results
        .into_iter()
        .chain(module_results.into_iter())
    {
        if let Some(output) = result.output {
            outputs.push(output);
        }
        if let Some((virtual_path, transformed_file)) = result.transformed {
            transformed_files.add(virtual_path, transformed_file);
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

    let mut transformed_count = 0usize;
    let mut tsgo_stats: Option<TsgoCheckStats> = None;
    let mut tsgo_total_time = None;
    let mut sveltekit_sync_time = None;
    let mut sveltekit_sync_ran = None;

    // Run TypeScript type-checking if JS diagnostics are enabled and not skipping tsgo
    if args.include_js() && !args.skip_tsgo {
        let transformed = transformed_files;
        transformed_count = transformed.files.len();

        if !transformed.files.is_empty() {
            // Ensure SvelteKit types are generated before running tsgo
            let tsgo_start = Instant::now();
            let sync_start = Instant::now();
            let sync_ran = match TsgoRunner::ensure_sveltekit_sync(workspace).await {
                Ok(ran) => ran,
                Err(e) => {
                    eprintln!("Warning: {}", e);
                    false
                }
            };
            sveltekit_sync_time = Some(sync_start.elapsed());
            sveltekit_sync_ran = Some(sync_ran);

            match run_tsgo_check(
                workspace,
                &transformed,
                args,
                args.tsgo_diagnostics,
                extra_paths,
            )
            .await
            {
                Ok(output) => {
                    let mut ts_diagnostics = output.diagnostics;
                    ts_diagnostics
                        .retain(|diag| include_ts_severity(diag.severity, args.threshold));

                    // Count and print TypeScript diagnostics
                    for diag in &ts_diagnostics {
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
                        let ts_output =
                            format_ts_diagnostics(&ts_diagnostics, workspace, args.output);
                        print!("{}", ts_output);
                    }

                    tsgo_stats = Some(output.stats);
                    tsgo_total_time = Some(tsgo_start.elapsed());
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
    }

    if timings_enabled {
        match args.timings_format {
            TimingFormat::Json => {
                let json = timings_json(
                    file_scan_time,
                    svelte_time,
                    total_file_count,
                    transformed_count,
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
                eprintln!(
                    "svelte phase: {:?} ({} files, {} transformed)",
                    svelte_time, total_file_count, transformed_count
                );
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
        file_count: total_file_count,
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

    Ok(summary)
}

/// Runs tsgo type-checking on transformed files.
async fn run_tsgo_check(
    workspace: &Utf8Path,
    files: &TransformedFiles,
    args: &Args,
    emit_diagnostics: bool,
    extra_paths: &HashMap<String, Vec<String>>,
) -> Result<TsgoCheckOutput, OrchestratorError> {
    // Find or install tsgo
    let tsgo_path = TsgoRunner::ensure_tsgo(Some(workspace))
        .await
        .map_err(|e| OrchestratorError::TsgoError(e.to_string()))?;

    let runner = TsgoRunner::new(
        tsgo_path,
        workspace.to_owned(),
        args.tsconfig.clone(),
        extra_paths.clone(),
        !args.disable_sveltekit_cache,
    );

    runner
        .check(files, emit_diagnostics)
        .await
        .map_err(|e| OrchestratorError::TsgoError(e.to_string()))
}

/// Formats TypeScript diagnostics for output.
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

fn include_svelte_severity(severity: Severity, threshold: crate::cli::Threshold) -> bool {
    match threshold {
        crate::cli::Threshold::Error => matches!(severity, Severity::Error),
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

fn duration_ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[allow(clippy::too_many_arguments)]
fn timings_json(
    file_scan_time: Option<std::time::Duration>,
    svelte_time: std::time::Duration,
    file_count: usize,
    transformed_count: usize,
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

/// Runs in watch mode.
async fn run_watch_mode(
    args: &Args,
    workspace: &Utf8Path,
    initial_files: Vec<Utf8PathBuf>,
    file_scan_time: Option<std::time::Duration>,
    use_nodenext_imports: bool,
    extra_paths: &HashMap<String, Vec<String>>,
) -> Result<CheckSummary, OrchestratorError> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::time::Duration;

    println!("Starting watch mode...\n");

    // Initial check
    let _summary = run_single_check(
        args,
        workspace,
        initial_files.clone(),
        file_scan_time,
        use_nodenext_imports,
        extra_paths,
    )
    .await?;

    // Set up file watcher with tokio channel
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(1)),
    )
    .map_err(|e| OrchestratorError::WatchFailed(e.to_string()))?;

    watcher
        .watch(workspace.as_std_path(), RecursiveMode::Recursive)
        .map_err(|e| OrchestratorError::WatchFailed(e.to_string()))?;

    println!("Watching for changes... (Ctrl+C to stop)\n");

    while let Some(event) = rx.recv().await {
        // Check if any Svelte files changed (.svelte, .svelte.ts, .svelte.js)
        let svelte_changed = event.paths.iter().any(|p| {
            let path_str = p.to_string_lossy();
            path_str.ends_with(".svelte")
                || path_str.ends_with(".svelte.ts")
                || path_str.ends_with(".svelte.js")
        });

        if svelte_changed {
            if !args.preserve_watch_output {
                // Clear screen
                print!("\x1B[2J\x1B[1;1H");
            }

            println!("File changed, re-checking...\n");

            // Re-run check
            let _ = run_single_check(
                args,
                workspace,
                initial_files.clone(),
                file_scan_time,
                use_nodenext_imports,
                extra_paths,
            )
            .await;
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
            let mut col = 1;
            for (j, c) in source[i..].char_indices() {
                if col == column {
                    return (i + j) as u32;
                }
                if c == '\n' {
                    break;
                }
                col += 1;
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
}
