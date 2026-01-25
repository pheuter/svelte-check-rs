//! svelte-check-rs: High-performance Svelte type-checker and linter.

mod cli;
mod config;
mod orchestrator;
mod output;

use bun_runner::BunRunner;
use camino::Utf8Path;
use clap::Parser;
use cli::Args;
use miette::Result;
use tsgo_runner::{PackageManager, TsgoRunner};

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

    // Handle bun version command
    if args.bun_version {
        match BunRunner::get_bun_version().await {
            Ok((version, path)) => {
                println!("bun {}", version);
                println!("path: {}", path);
                if let Some(cache_dir) = BunRunner::get_cache_dir() {
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

    // Handle bun update command
    if let Some(version_opt) = &args.bun_update {
        let version = version_opt.as_deref();
        match BunRunner::update_bun(version).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Handle debug-paths command
    if args.debug_paths {
        print_debug_paths(&args.workspace);
        return Ok(());
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

/// Prints debug information about resolved paths and package manager.
fn print_debug_paths(workspace: &Utf8Path) {
    let workspace = if workspace.as_str() == "." {
        std::env::current_dir()
            .ok()
            .and_then(|p| camino::Utf8PathBuf::try_from(p).ok())
            .unwrap_or_else(|| workspace.to_owned())
    } else {
        workspace.to_owned()
    };

    println!("Workspace: {}", workspace);
    println!();

    // Package manager detection
    println!("Package Manager:");
    if let Some(pm) = PackageManager::detect_from_workspace(&workspace) {
        println!(
            "  from workspace: {} (detected from lockfile)",
            pm.command_name()
        );
    } else {
        println!("  from workspace: (none detected)");
    }
    if let Some(pm) = PackageManager::detect_from_path() {
        println!("  from PATH:      {}", pm.command_name());
    } else {
        println!("  from PATH:      (none found)");
    }
    println!();

    // tsgo binary
    println!("tsgo:");
    if let Some(path) = TsgoRunner::find_tsgo(Some(&workspace)) {
        println!("  resolved: {}", path);
    } else {
        println!("  resolved: (not found - will be auto-installed on first run)");
        if let Some(cache_dir) = TsgoRunner::get_cache_dir() {
            println!("  cache:    {}/node_modules/.bin/tsgo", cache_dir);
        }
    }
    println!();

    // svelte-kit binary
    println!("svelte-kit:");
    match TsgoRunner::find_sveltekit_binary(&workspace) {
        Ok(path) => println!("  resolved: {}", path),
        Err(_) => println!("  resolved: (not found - not a SvelteKit project or not installed)"),
    }

    println!();

    // bun binary
    println!("bun:");
    if let Some(path) = BunRunner::find_bun(Some(&workspace)) {
        println!("  resolved: {}", path);
    } else {
        println!("  resolved: (not found - will be auto-installed on first run)");
        if let Some(cache_dir) = BunRunner::get_cache_dir() {
            println!("  cache:    {}/node_modules/.bin/bun", cache_dir);
        }
    }
}
