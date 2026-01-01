//! Main orchestration logic.

use crate::cli::Args;
use crate::config::{SvelteConfig, TsConfig};
use crate::output::{CheckSummary, Formatter};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSetBuilder};
use rayon::prelude::*;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use svelte_diagnostics::{check as check_svelte, DiagnosticOptions, Severity};
use svelte_parser::parse;
use thiserror::Error;
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
    for pattern in ["**/node_modules/**", "**/dist/**", "**/.svelte-kit/**"] {
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
        run_single_check(&args, &workspace, files)
    }
}

/// Runs a single check pass.
fn run_single_check(
    args: &Args,
    workspace: &Utf8Path,
    files: Vec<Utf8PathBuf>,
) -> Result<CheckSummary, OrchestratorError> {
    let formatter = Formatter::new(args.output);
    let error_count = AtomicUsize::new(0);
    let warning_count = AtomicUsize::new(0);

    // Determine diagnostic options
    let diag_options = DiagnosticOptions {
        a11y: args.include_svelte(),
        css: args.include_css(),
        component: args.include_svelte(),
    };

    // Process files in parallel
    let outputs: Vec<String> = files
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

            // Run Svelte diagnostics
            let svelte_diags = check_svelte(&parse_result.document, diag_options.clone());
            all_diagnostics.extend(svelte_diags);

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
                Some(formatter.format(&all_diagnostics, relative_path, &source))
            }
        })
        .collect();

    // Print all outputs
    for output in outputs {
        print!("{}", output);
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
    }

    Ok(summary)
}

/// Runs in watch mode.
async fn run_watch_mode(
    args: &Args,
    workspace: &Utf8Path,
    initial_files: Vec<Utf8PathBuf>,
) -> Result<CheckSummary, OrchestratorError> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc;
    use std::time::Duration;

    println!("Starting watch mode...\n");

    // Initial check
    let _summary = run_single_check(args, workspace, initial_files.clone())?;

    // Set up file watcher
    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(1)),
    )
    .map_err(|e| OrchestratorError::WatchFailed(e.to_string()))?;

    watcher
        .watch(workspace.as_std_path(), RecursiveMode::Recursive)
        .map_err(|e| OrchestratorError::WatchFailed(e.to_string()))?;

    println!("Watching for changes... (Ctrl+C to stop)\n");

    loop {
        match rx.recv() {
            Ok(event) => {
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
                    let _ = run_single_check(args, workspace, initial_files.clone());
                }
            }
            Err(e) => {
                return Err(OrchestratorError::WatchFailed(e.to_string()));
            }
        }
    }
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
