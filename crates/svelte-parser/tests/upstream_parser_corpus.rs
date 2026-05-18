//! Optional parity test against upstream Svelte parser suites.
//!
//! This test is intentionally `ignored` because it requires a local checkout
//! of the Svelte repository. It runs every sample under `parser-modern` and
//! `parser-legacy` through `svelte-parser`, enabling loose mode for samples
//! whose directory name starts with `loose-` (mirroring upstream's runner).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use svelte_parser::{parse_with_options, ParseOptions};

const SUITES: &[&str] = &["parser-modern", "parser-legacy"];

#[derive(Debug, Clone)]
struct ParserSample {
    suite: String,
    name: String,
    input_path: PathBuf,
    loose: bool,
}

fn collect_samples(svelte_repo: &Path) -> Vec<ParserSample> {
    let mut samples = Vec::new();

    for suite in SUITES {
        let samples_dir = svelte_repo
            .join("packages")
            .join("svelte")
            .join("tests")
            .join(suite)
            .join("samples");

        let Ok(entries) = fs::read_dir(&samples_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let sample_dir = entry.path();
            if !sample_dir.is_dir() {
                continue;
            }

            let Some(name_os) = sample_dir.file_name() else {
                continue;
            };
            let name = name_os.to_string_lossy().into_owned();
            let input_path = sample_dir.join("input.svelte");
            if !input_path.is_file() {
                continue;
            }

            samples.push(ParserSample {
                suite: (*suite).to_string(),
                loose: name.starts_with("loose-"),
                name,
                input_path,
            });
        }
    }

    samples.sort_by(|a, b| {
        a.suite
            .cmp(&b.suite)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.input_path.cmp(&b.input_path))
    });
    samples
}

fn normalize_input(source: String) -> String {
    source.trim_end().replace('\r', "")
}

#[test]
#[ignore = "requires SVELTE_REPO to a local sveltejs/svelte checkout"]
fn test_upstream_svelte_parser_samples() {
    let Ok(svelte_repo) = env::var("SVELTE_REPO") else {
        eprintln!("Skipping upstream parser corpus: set SVELTE_REPO to a sveltejs/svelte checkout");
        return;
    };

    let svelte_repo = PathBuf::from(svelte_repo);
    if !svelte_repo.exists() {
        panic!("SVELTE_REPO does not exist: {}", svelte_repo.display());
    }

    let samples = collect_samples(&svelte_repo);
    assert!(
        !samples.is_empty(),
        "No parser samples found under {}",
        svelte_repo.display()
    );

    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut loose_checked = 0usize;

    for sample in samples {
        let source = fs::read_to_string(&sample.input_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", sample.input_path.display()));
        let options = ParseOptions {
            loose: sample.loose,
            ..ParseOptions::default()
        };
        let result = parse_with_options(&normalize_input(source), options);
        checked += 1;
        if sample.loose {
            loose_checked += 1;
        }

        if !result.errors.is_empty() {
            failures.push(format!(
                "{}:{} ({} errors{})",
                sample.suite,
                sample.name,
                result.errors.len(),
                if sample.loose { ", loose" } else { "" },
            ));
        }
    }

    eprintln!(
        "Upstream parser corpus: checked {}, loose {}, failures {}",
        checked,
        loose_checked,
        failures.len()
    );

    if !failures.is_empty() {
        let preview = failures
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "Found {} parser parity gaps.\n{}\n{}",
            failures.len(),
            preview,
            if failures.len() > 50 {
                "\n(truncated to first 50 failures)"
            } else {
                ""
            }
        );
    }
}
