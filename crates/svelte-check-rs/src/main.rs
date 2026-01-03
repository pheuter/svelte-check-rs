//! svelte-check-rs: High-performance Svelte type-checker and linter.

mod cli;
mod config;
mod orchestrator;
mod output;

use clap::Parser;
use cli::Args;
use miette::Result;
use tsgo_runner::TsgoRunner;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle tsgo version command
    if args.tsgo_version {
        match TsgoRunner::get_tsgo_version().await {
            Ok((version, path)) => {
                println!("tsgo {}", version);
                println!("path: {}", path);
                if let Some(cache_dir) = TsgoRunner::get_cache_dir() {
                    println!("cache: {}", cache_dir);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Handle tsgo update command
    if let Some(version_opt) = &args.tsgo_update {
        let version = version_opt.as_deref();
        match TsgoRunner::update_tsgo(version).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

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
