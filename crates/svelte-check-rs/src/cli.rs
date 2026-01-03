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

    /// Output transformed TSX to stdout (for debugging)
    #[arg(long = "emit-tsx")]
    pub emit_tsx: bool,

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
}
