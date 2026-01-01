//! Svelte-Check-RS: High-performance Svelte type-checker and linter.

mod cli;
mod config;
mod orchestrator;
mod output;

use clap::Parser;
use cli::Args;
use miette::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let result = orchestrator::run(args).await;

    match result {
        Ok(summary) => {
            if summary.error_count > 0 || (summary.warning_count > 0 && summary.fail_on_warnings) {
                std::process::exit(1);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
