//! CLI argument parsing.

use camino::Utf8PathBuf;
use clap::{Parser, ValueEnum};

/// High-performance Svelte type-checker and linter.
#[derive(Debug, Parser)]
#[command(name = "svelte-check-rs")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Working directory for the check
    #[arg(long, default_value = ".")]
    pub workspace: Utf8PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value = "human")]
    pub output: OutputFormat,

    /// Path to tsconfig.json
    #[arg(long)]
    pub tsconfig: Option<Utf8PathBuf>,

    /// Minimum severity threshold
    #[arg(long, value_enum, default_value = "warning")]
    pub threshold: Threshold,

    /// Watch mode
    #[arg(long)]
    pub watch: bool,

    /// Preserve watch output (don't clear screen)
    #[arg(long = "preserveWatchOutput")]
    pub preserve_watch_output: bool,

    /// Exit with error on warnings
    #[arg(long = "fail-on-warnings")]
    pub fail_on_warnings: bool,

    /// Compiler warning configuration (JSON)
    #[arg(long = "compiler-warnings")]
    pub compiler_warnings: Option<String>,

    /// Diagnostic sources to include (comma-separated: js,svelte,css)
    #[arg(long = "diagnostic-sources")]
    pub diagnostic_sources: Option<String>,

    /// Glob patterns to ignore
    #[arg(long)]
    pub ignore: Vec<String>,

    /// Output transformed TypeScript to stdout (for debugging)
    #[arg(long = "emit-ts")]
    pub emit_ts: bool,

    /// Print tsgo compiler diagnostics (performance stats)
    #[arg(long = "tsgo-diagnostics")]
    pub tsgo_diagnostics: bool,

    /// Print timing breakdowns
    #[arg(long)]
    pub timings: bool,

    /// Timing output format
    #[arg(long, value_enum, default_value = "text")]
    pub timings_format: TimingFormat,

    /// Disable caching of .svelte-kit (fallback to direct symlink)
    #[arg(long = "disable-sveltekit-cache")]
    pub disable_sveltekit_cache: bool,

    /// Show tsgo version and installation path
    #[arg(long = "tsgo-version")]
    pub tsgo_version: bool,

    /// Update tsgo to latest or specified version (e.g., --tsgo-update or --tsgo-update=7.0.0-dev.20260101.1)
    #[arg(long = "tsgo-update")]
    pub tsgo_update: Option<Option<String>>,

    /// Show resolved paths for package manager, tsgo, and svelte-kit binaries
    #[arg(long = "debug-paths")]
    pub debug_paths: bool,

    // === Debug flags for development ===
    /// List files that would be checked, then exit (useful for debugging file discovery)
    #[arg(long = "list-files")]
    pub list_files: bool,

    /// Skip TypeScript type-checking (tsgo), only run Svelte diagnostics (a11y, CSS, component)
    #[arg(long = "skip-tsgo")]
    pub skip_tsgo: bool,

    /// Process only a single file (useful for isolating issues)
    #[arg(long = "single-file")]
    pub single_file: Option<Utf8PathBuf>,

    /// Output parsed AST as JSON (for debugging parser issues)
    #[arg(long = "emit-ast")]
    pub emit_ast: bool,

    /// Show resolved configuration (tsconfig, svelte.config.js, excludes)
    #[arg(long = "show-config")]
    pub show_config: bool,

    /// Show source map mappings when using --emit-ts (for debugging position mapping)
    #[arg(long = "emit-source-map")]
    pub emit_source_map: bool,

    /// Show cache statistics (files written/skipped to node_modules/.cache/svelte-check-rs/)
    #[arg(long = "cache-stats")]
    pub cache_stats: bool,
}

/// Output format options.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable output (default)
    #[default]
    Human,
    /// Human-readable with code snippets
    HumanVerbose,
    /// JSON output
    Json,
    /// Machine-readable (one line per diagnostic)
    Machine,
}

/// Severity threshold.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum Threshold {
    /// Only show errors
    Error,
    /// Show errors and warnings (default)
    #[default]
    Warning,
}

/// Timing output format.
#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq)]
pub enum TimingFormat {
    /// Human-readable output
    #[default]
    Text,
    /// JSON output (machine-readable)
    Json,
}

impl Args {
    /// Returns whether JS diagnostics should be included.
    #[allow(dead_code)] // Will be used when tsgo integration is complete
    pub fn include_js(&self) -> bool {
        self.diagnostic_sources
            .as_ref()
            .map(|s| s.contains("js"))
            .unwrap_or(true)
    }

    /// Returns whether Svelte diagnostics should be included.
    pub fn include_svelte(&self) -> bool {
        self.diagnostic_sources
            .as_ref()
            .map(|s| s.contains("svelte"))
            .unwrap_or(true)
    }

    /// Returns whether CSS diagnostics should be included.
    pub fn include_css(&self) -> bool {
        self.diagnostic_sources
            .as_ref()
            .map(|s| s.contains("css"))
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_args() {
        let args = Args::parse_from(["svelte-check-rs"]);
        assert_eq!(args.workspace.as_str(), ".");
        assert!(matches!(args.output, OutputFormat::Human));
        assert!(!args.watch);
    }

    #[test]
    fn test_custom_workspace() {
        let args = Args::parse_from(["svelte-check-rs", "--workspace", "/path/to/project"]);
        assert_eq!(args.workspace.as_str(), "/path/to/project");
    }

    #[test]
    fn test_watch_mode() {
        let args = Args::parse_from(["svelte-check-rs", "--watch"]);
        assert!(args.watch);
    }

    #[test]
    fn test_output_formats() {
        let args = Args::parse_from(["svelte-check-rs", "--output", "json"]);
        assert!(matches!(args.output, OutputFormat::Json));

        let args = Args::parse_from(["svelte-check-rs", "--output", "machine"]);
        assert!(matches!(args.output, OutputFormat::Machine));
    }

    #[test]
    fn test_diagnostic_sources() {
        let args = Args::parse_from(["svelte-check-rs", "--diagnostic-sources", "js,svelte"]);
        assert!(args.include_js());
        assert!(args.include_svelte());
        assert!(!args.include_css());
    }

    #[test]
    fn test_debug_flags() {
        // Test --list-files
        let args = Args::parse_from(["svelte-check-rs", "--list-files"]);
        assert!(args.list_files);

        // Test --skip-tsgo
        let args = Args::parse_from(["svelte-check-rs", "--skip-tsgo"]);
        assert!(args.skip_tsgo);

        // Test --single-file
        let args = Args::parse_from(["svelte-check-rs", "--single-file", "src/App.svelte"]);
        assert_eq!(
            args.single_file.as_ref().map(|p| p.as_str()),
            Some("src/App.svelte")
        );

        // Test --emit-ast
        let args = Args::parse_from(["svelte-check-rs", "--emit-ast"]);
        assert!(args.emit_ast);

        // Test --show-config
        let args = Args::parse_from(["svelte-check-rs", "--show-config"]);
        assert!(args.show_config);

        // Test --emit-source-map
        let args = Args::parse_from(["svelte-check-rs", "--emit-source-map"]);
        assert!(args.emit_source_map);

        // Test --cache-stats
        let args = Args::parse_from(["svelte-check-rs", "--cache-stats"]);
        assert!(args.cache_stats);
    }

    #[test]
    fn test_combined_debug_flags() {
        // Test combining multiple debug flags
        let args = Args::parse_from([
            "svelte-check-rs",
            "--emit-ts",
            "--emit-source-map",
            "--skip-tsgo",
            "--single-file",
            "test.svelte",
        ]);
        assert!(args.emit_ts);
        assert!(args.emit_source_map);
        assert!(args.skip_tsgo);
        assert_eq!(
            args.single_file.as_ref().map(|p| p.as_str()),
            Some("test.svelte")
        );
    }
}
