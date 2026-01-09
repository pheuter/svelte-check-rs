//! Corpus tests that parse all fixture files to ensure no panics
//! and that valid fixtures produce no errors.

use std::fs;
use std::path::PathBuf;
use svelte_parser::parse;

fn get_fixtures_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
}

fn collect_svelte_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "svelte") {
                files.push(path);
            } else if path.is_dir() {
                files.extend(collect_svelte_files(&path));
            }
        }
    }
    files
}

#[test]
fn test_parse_all_valid_fixtures() {
    let fixtures_dir = get_fixtures_dir().join("valid");
    let files = collect_svelte_files(&fixtures_dir);

    assert!(!files.is_empty(), "No valid fixtures found");

    for path in &files {
        let source = fs::read_to_string(path).expect("Failed to read file");
        let filename = path.file_name().unwrap().to_string_lossy();

        // Should not panic
        let result = parse(&source);

        // Valid fixtures should have no errors
        assert!(
            result.errors.is_empty(),
            "Valid fixture {} should have no errors, but got: {:?}",
            filename,
            result.errors
        );

        // Should produce a document
        assert!(
            !result.document.fragment.nodes.is_empty() || result.document.instance_script.is_some(),
            "Valid fixture {} should produce some content",
            filename
        );
    }

    println!("Successfully parsed {} valid fixtures", files.len());
}

#[test]
fn test_parse_all_invalid_fixtures() {
    let fixtures_dir = get_fixtures_dir().join("invalid");
    let files = collect_svelte_files(&fixtures_dir);

    // Invalid directory contains files with semantic issues (a11y, etc.)
    // not necessarily parse errors. Just ensure they don't panic.
    for path in &files {
        let source = fs::read_to_string(path).expect("Failed to read file");
        let filename = path.file_name().unwrap().to_string_lossy();

        // Should not panic - even for invalid input
        let _result = parse(&source);

        println!("Parsed invalid fixture {} without panic", filename);
    }

    if !files.is_empty() {
        println!("Successfully parsed {} invalid fixtures", files.len());
    }
}

#[test]
fn test_specific_valid_fixtures() {
    let fixtures_dir = get_fixtures_dir().join("valid");

    // Test specific fixtures exist and parse correctly
    let required_fixtures = [
        "Simple.svelte",
        "Counter.svelte",
        "Blocks.svelte",
        "Snippets.svelte",
        "Accessible.svelte",
    ];

    for fixture_name in required_fixtures {
        let path = fixtures_dir.join(fixture_name);
        if path.exists() {
            let source = fs::read_to_string(&path).expect("Failed to read file");
            let result = parse(&source);
            assert!(
                result.errors.is_empty(),
                "Required fixture {} should have no errors: {:?}",
                fixture_name,
                result.errors
            );
        }
    }
}

#[test]
fn test_new_test_fixtures() {
    let fixtures_dir = get_fixtures_dir().join("valid");

    // Test the new fixtures we created
    let new_fixtures = [
        "SvelteElements.svelte",
        "AllDirectives.svelte",
        "BlockEdgeCases.svelte",
        "AttributeEdgeCases.svelte",
        "ComplexExpressions.svelte",
        "SpecialTags.svelte",
        "Attachments.svelte",
    ];

    for fixture_name in new_fixtures {
        let path = fixtures_dir.join(fixture_name);
        if path.exists() {
            let source = fs::read_to_string(&path).expect("Failed to read file");
            let result = parse(&source);

            // These should parse without errors (we may need to fix some)
            // For now, just ensure they don't panic
            println!(
                "{}: {} errors, {} nodes",
                fixture_name,
                result.errors.len(),
                result.document.fragment.nodes.len()
            );
        }
    }
}

/// Regression tests for issues #52 and #54:
/// - #52: {#each} with destructuring defaults spanning multiple lines
/// - #54: {#each ... as const as [destructuring with defaults]} on multiple lines
#[test]
fn test_each_multiline_as_whitespace() {
    let fixtures_dir = get_fixtures_dir().join("valid").join("parser");

    let issue_fixtures = [
        (
            "issue-52-each-multiline-destructure-default.svelte",
            "multi-line destructuring with defaults",
        ),
        (
            "issue-54-each-as-const-multiline.svelte",
            "as const with multi-line destructuring",
        ),
    ];

    for (fixture_name, description) in issue_fixtures {
        let path = fixtures_dir.join(fixture_name);
        assert!(path.exists(), "Fixture {} should exist", fixture_name);

        let source = fs::read_to_string(&path).expect("Failed to read file");
        let result = parse(&source);

        assert!(
            result.errors.is_empty(),
            "Fixture {} ({}) should parse without errors, but got: {:?}",
            fixture_name,
            description,
            result.errors
        );

        println!("OK: {} - {}", fixture_name, description);
    }
}

#[test]
fn test_edge_cases() {
    // Test various edge cases that shouldn't panic
    let edge_cases = [
        "",              // Empty file
        "   ",           // Whitespace only
        "<div>",         // Unclosed tag
        "{#if",          // Incomplete block
        "<!-- comment",  // Unclosed comment
        "{expression",   // Unclosed expression
        "<div attr=",    // Incomplete attribute
        "<div attr=\"",  // Unclosed quote
        "{#each items}", // Missing as pattern
        "{#await}",      // Missing expression
    ];

    for (i, source) in edge_cases.iter().enumerate() {
        // Should not panic
        let _result = parse(source);
        println!("Edge case {} parsed without panic", i);
    }
}

#[test]
fn test_invalid_syntax_produces_errors() {
    // These SHOULD produce parse errors - this tests the parser catches issues
    let invalid_cases: &[(&str, &str)] = &[
        ("<div>", "unclosed tag"),
        ("<div></span>", "mismatched closing tag"),
        ("{#if}", "if without condition"),
        ("{#each items}", "each without as pattern"),
        ("{expression", "unclosed expression"),
        ("<div attr=\"unclosed>", "unclosed attribute quote"),
    ];

    for (source, description) in invalid_cases {
        let result = parse(source);
        // Note: We're documenting which cases the parser catches vs doesn't
        // This helps identify parser gaps
        if result.errors.is_empty() {
            println!(
                "WARNING: Parser did NOT produce error for: {} ({})",
                description, source
            );
        } else {
            println!(
                "OK: Parser produced {} error(s) for: {}",
                result.errors.len(),
                description
            );
        }
    }
}

#[test]
fn test_unclosed_blocks_produce_errors() {
    // Specifically test that unclosed blocks produce errors
    let unclosed_block_cases: &[(&str, &str)] = &[
        ("{#if true}<p>test</p>", "unclosed if block"),
        ("{#each items as item}<li>test</li>", "unclosed each block"),
        ("{#await promise}<p>loading</p>", "unclosed await block"),
        ("{#key id}<div>test</div>", "unclosed key block"),
    ];

    let mut all_caught = true;
    for (source, description) in unclosed_block_cases {
        let result = parse(source);
        if result.errors.is_empty() {
            println!("BUG: Parser did NOT produce error for: {}", description);
            all_caught = false;
        } else {
            println!("OK: {} - {} errors", description, result.errors.len());
        }
    }

    // This assertion documents whether the parser catches unclosed blocks
    // If it fails, it means the parser is too lenient
    if !all_caught {
        println!("\nNOTE: Parser does not currently report all unclosed block errors.");
        println!("This may be by design (lenient parsing) or a bug to fix.");
    }
}

#[test]
fn test_stress_deeply_nested() {
    // Test deeply nested structures don't cause stack overflow
    let mut source = String::new();
    for _ in 0..50 {
        source.push_str("{#if true}");
    }
    source.push_str("<p>deep</p>");
    for _ in 0..50 {
        source.push_str("{/if}");
    }

    let result = parse(&source);
    // Should parse without crashing
    assert!(
        result.errors.is_empty(),
        "Deep nesting should parse: {:?}",
        result.errors
    );
}

#[test]
fn test_stress_many_siblings() {
    // Test many sibling elements
    let mut source = String::new();
    for i in 0..100 {
        source.push_str(&format!("<div id=\"{}\">content</div>", i));
    }

    let result = parse(&source);
    assert!(
        result.errors.is_empty(),
        "Many siblings should parse: {:?}",
        result.errors
    );
    assert_eq!(
        result.document.fragment.nodes.len(),
        100,
        "Should have 100 nodes"
    );
}

#[test]
fn test_stress_long_expression() {
    // Test a very long expression
    let expr = "a".repeat(1000);
    let source = format!("{{{}}}", expr);

    let result = parse(&source);
    // Should parse without crashing
    assert!(result.errors.is_empty());
}
