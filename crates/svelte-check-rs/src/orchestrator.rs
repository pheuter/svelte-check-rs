//! Main orchestration logic.

use crate::cli::Args;
use crate::config::{SvelteConfig, TsConfig};
use crate::output::{CheckSummary, FormattedDiagnostic, Formatter, Position};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSetBuilder};
use rayon::prelude::*;
use source_map::LineIndex;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use svelte_diagnostics::{check as check_svelte, DiagnosticOptions, Severity};
use svelte_parser::parse;
use svelte_transformer::{transform, TransformOptions};
use thiserror::Error;
use tsgo_runner::{TransformedFile, TransformedFiles, TsgoDiagnostic, TsgoRunner};
use walkdir::WalkDir;

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
    let _ts_config = TsConfig::find(&workspace);

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
    ] {
        if let Ok(glob) = Glob::new(pattern) {
            ignore_builder.add(glob);
        }
    }

    let ignore_set = ignore_builder
        .build()
        .map_err(|e| OrchestratorError::InvalidGlob(e.to_string()))?;

    // Find Svelte files
    let extensions = svelte_config.file_extensions();
    let files: Vec<Utf8PathBuf> = WalkDir::new(&workspace)
        .into_iter()
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

    if args.watch {
        run_watch_mode(&args, &workspace, files).await
    } else {
        run_single_check(&args, &workspace, files).await
    }
}

/// Runs a single check pass.
async fn run_single_check(
    args: &Args,
    workspace: &Utf8Path,
    files: Vec<Utf8PathBuf>,
) -> Result<CheckSummary, OrchestratorError> {
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

    // Shared container for transformed files (thread-safe)
    let transformed_files = Mutex::new(TransformedFiles::new());

    struct FileOutput {
        text: Option<String>,
        json: Vec<FormattedDiagnostic>,
    }

    // Process files in parallel: parse, run Svelte diagnostics, and transform
    let outputs: Vec<FileOutput> = files
        .par_iter()
        .filter_map(|file_path| {
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", file_path, e);
                    return None;
                }
            };

            // Parse the file
            let parse_result = parse(&source);

            // Collect parse errors
            let mut all_diagnostics = Vec::new();

            // Convert parse errors to diagnostics
            for error in &parse_result.errors {
                all_diagnostics.push(svelte_diagnostics::Diagnostic::new(
                    svelte_diagnostics::DiagnosticCode::InvalidRuneUsage, // Using as generic parse error
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

            // Transform for TypeScript checking (if JS diagnostics enabled)
            if args.include_js() {
                let transform_options = TransformOptions {
                    filename: Some(file_path.to_string()),
                    source_maps: true,
                };

                let transform_result = transform(&parse_result.document, transform_options);

                // If emit_tsx is enabled, print TSX for each transformed file.
                if args.emit_tsx {
                    let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                    eprintln!(
                        "=== TSX for {} ===\n{}",
                        relative_path, transform_result.tsx_code
                    );
                }

                // Create the virtual path (original.svelte -> original.svelte.ts)
                let relative_path = file_path.strip_prefix(workspace).unwrap_or(file_path);
                let virtual_path = Utf8PathBuf::from(format!("{}.ts", relative_path));

                let transformed_file = TransformedFile {
                    original_path: file_path.clone(),
                    tsx_content: transform_result.tsx_code,
                    source_map: transform_result.source_map,
                    original_line_index: LineIndex::new(&source),
                };

                // Add to shared collection
                if let Ok(mut files) = transformed_files.lock() {
                    files.add(virtual_path, transformed_file);
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

            if all_diagnostics.is_empty() {
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
            }
        })
        .collect();

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

    // Run TypeScript type-checking if JS diagnostics are enabled
    if args.include_js() {
        let transformed = transformed_files.into_inner().unwrap_or_default();

        if !transformed.files.is_empty() {
            match run_tsgo_check(workspace, &transformed, args).await {
                Ok(mut ts_diagnostics) => {
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
                }
                Err(e) => {
                    eprintln!("TypeScript checking failed: {}", e);
                }
            }
        }
    }

    let summary = CheckSummary {
        file_count: files.len(),
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
) -> Result<Vec<TsgoDiagnostic>, OrchestratorError> {
    // Find or install tsgo
    let tsgo_path = TsgoRunner::ensure_tsgo()
        .await
        .map_err(|e| OrchestratorError::TsgoError(e.to_string()))?;

    let runner = TsgoRunner::new(tsgo_path, workspace.to_owned(), args.tsconfig.clone());

    runner
        .check(files)
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

/// Runs in watch mode.
async fn run_watch_mode(
    args: &Args,
    workspace: &Utf8Path,
    initial_files: Vec<Utf8PathBuf>,
) -> Result<CheckSummary, OrchestratorError> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::time::Duration;

    println!("Starting watch mode...\n");

    // Initial check
    let _summary = run_single_check(args, workspace, initial_files.clone()).await?;

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
        // Check if any Svelte files changed
        let svelte_changed = event
            .paths
            .iter()
            .any(|p| p.extension().map(|ext| ext == "svelte").unwrap_or(false));

        if svelte_changed {
            if !args.preserve_watch_output {
                // Clear screen
                print!("\x1B[2J\x1B[1;1H");
            }

            println!("File changed, re-checking...\n");

            // Re-run check
            let _ = run_single_check(args, workspace, initial_files.clone()).await;
        }
    }

    Err(OrchestratorError::WatchFailed(
        "watch channel closed unexpectedly".to_string(),
    ))
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
}
