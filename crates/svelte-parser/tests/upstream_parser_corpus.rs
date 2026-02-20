//! Optional parity test against upstream Svelte parser suites.
//!
//! This test is intentionally `ignored` because it requires a local checkout
//! of the Svelte repository and currently serves as a parity-gap detector.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use svelte_parser::parse;

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

    let include_loose = env::var("SVELTE_INCLUDE_LOOSE").is_ok_and(|v| v == "1");
    let samples = collect_samples(&svelte_repo);
    assert!(
        !samples.is_empty(),
        "No parser samples found under {}",
        svelte_repo.display()
    );

    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut skipped_loose = 0usize;

    for sample in samples {
        if sample.loose && !include_loose {
            skipped_loose += 1;
            continue;
        }

        let source = fs::read_to_string(&sample.input_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", sample.input_path.display()));
        let result = parse(&normalize_input(source));
        checked += 1;

        if !result.errors.is_empty() {
            failures.push(format!(
                "{}:{} ({} errors)",
                sample.suite,
                sample.name,
                result.errors.len()
            ));
        }
    }

    eprintln!(
        "Upstream parser corpus: checked {}, skipped_loose {}, failures {}",
        checked,
        skipped_loose,
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
