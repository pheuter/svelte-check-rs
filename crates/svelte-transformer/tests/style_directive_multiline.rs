use svelte_parser::parse;
use svelte_transformer::{transform, TransformOptions};

#[test]
fn multiline_style_directive_string_is_escaped() {
    let source = r#"<span
  style:transform="rotate({angle}deg)
  translate({dx}px, {dy}px)"
></span>"#;

    let parsed = parse(source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some("Test.svelte".to_string()),
            source_maps: true,
            ..Default::default()
        },
    );

    let expected = "\"rotate({angle}deg)\\n  translate({dx}px, {dy}px)\"";
    assert!(
        result.tsx_code.contains(expected),
        "expected escaped newline in quoted style directive, got:\n{}",
        result.tsx_code
    );
}
