use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

fn compiler_codes_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("compiler-warning-codes.txt")
}

fn diagnostics_rs_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("diagnostic.rs")
}

fn coverage_doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docs")
        .join("diagnostics-coverage.md")
}

fn load_compiler_codes() -> BTreeSet<String> {
    let content = fs::read_to_string(compiler_codes_path())
        .expect("Failed to read compiler warning codes list");
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect()
}

fn normalize_code(code: &str) -> String {
    code.replace('-', "_")
}

fn load_internal_codes() -> Vec<String> {
    let content =
        fs::read_to_string(diagnostics_rs_path()).expect("Failed to read diagnostics source file");
    let mut in_as_str = false;
    let mut brace_depth: i32 = 0;
    let mut codes = Vec::new();

    for line in content.lines() {
        if !in_as_str {
            if line.contains("fn as_str") {
                in_as_str = true;
                brace_depth += count_braces(line);
            }
            continue;
        }

        if let Some(code) = extract_code_from_line(line) {
            codes.push(code);
        }

        brace_depth += count_braces(line);
        if in_as_str && brace_depth == 0 {
            break;
        }
    }

    codes
}

fn count_braces(line: &str) -> i32 {
    let mut count = 0;
    for ch in line.chars() {
        match ch {
            '{' => count += 1,
            '}' => count -= 1,
            _ => {}
        }
    }
    count
}

fn extract_code_from_line(line: &str) -> Option<String> {
    if !line.contains("DiagnosticCode::") || !line.contains("=>") {
        return None;
    }

    let quote_start = line.find('"')?;
    let rest = &line[quote_start + 1..];
    let quote_end = rest.find('"')?;
    Some(rest[..quote_end].to_string())
}

fn render_section(title: &str, items: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!("## {}\n", title));
    if items.is_empty() {
        out.push_str("- (none)\n");
        return out;
    }

    for item in items {
        out.push_str("- ");
        out.push_str(item);
        out.push('\n');
    }

    out
}

fn render_coverage_report(compiler_codes: &BTreeSet<String>, internal_codes: &[String]) -> String {
    let mut internal_map: BTreeMap<String, String> = BTreeMap::new();
    for code in internal_codes {
        let normalized = normalize_code(code);
        if internal_map.contains_key(&normalized) {
            panic!("Duplicate internal code normalization for {}", code);
        }
        internal_map.insert(normalized, code.clone());
    }

    let internal_normalized: BTreeSet<String> = internal_map.keys().cloned().collect();

    let overlap: Vec<String> = compiler_codes
        .intersection(&internal_normalized)
        .map(|code| {
            let internal = internal_map.get(code).expect("Missing internal mapping");
            if internal == code {
                code.clone()
            } else {
                format!("{} (internal: {})", code, internal)
            }
        })
        .collect();

    let compiler_only: Vec<String> = compiler_codes
        .difference(&internal_normalized)
        .cloned()
        .collect();

    let mut internal_only: Vec<String> = internal_map
        .iter()
        .filter(|(code, _)| !compiler_codes.contains(*code))
        .map(|(_, internal)| internal.clone())
        .collect();
    internal_only.sort();

    let mut out = String::new();
    out.push_str("## Summary\n");
    out.push_str(&format!("- Compiler warnings: {}\n", compiler_codes.len()));
    out.push_str(&format!(
        "- Internal diagnostics: {}\n",
        internal_codes.len()
    ));
    out.push_str(&format!("- Overlap: {}\n", overlap.len()));
    out.push_str(&format!("- Compiler-only: {}\n", compiler_only.len()));
    out.push_str(&format!("- Internal-only: {}\n\n", internal_only.len()));

    out.push_str(&render_section("Overlap", &overlap));
    out.push('\n');
    out.push_str(&render_section("Compiler-only", &compiler_only));
    out.push('\n');
    out.push_str(&render_section("Internal-only", &internal_only));

    out
}

fn extract_between_markers(content: &str, start: &str, end: &str) -> Option<String> {
    let start_idx = content.find(start)? + start.len();
    let end_idx = content[start_idx..].find(end)? + start_idx;
    Some(content[start_idx..end_idx].trim().to_string())
}

#[test]
fn compiler_warning_coverage_is_up_to_date() {
    let compiler_codes = load_compiler_codes();
    let internal_codes = load_internal_codes();

    let expected = render_coverage_report(&compiler_codes, &internal_codes);
    let doc =
        fs::read_to_string(coverage_doc_path()).expect("Failed to read diagnostics coverage doc");

    let start_marker = "<!-- COVERAGE:START -->";
    let end_marker = "<!-- COVERAGE:END -->";
    let actual = extract_between_markers(&doc, start_marker, end_marker)
        .expect("Coverage markers not found in diagnostics-coverage.md");

    assert_eq!(
        actual,
        expected.trim(),
        "Diagnostics coverage report is out of date. Update docs/diagnostics-coverage.md."
    );
}
