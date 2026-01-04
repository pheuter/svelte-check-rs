//! Svelte 5 parser for svelte-check-rs.
//!
//! This crate provides a complete parser for Svelte 5 syntax including:
//! - Lexer (tokenizer) using `logos`
//! - Recursive descent parser
//! - AST types for all Svelte constructs
//! - Error recovery for partial parsing
//!
//! # Example
//!
//! ```
//! use svelte_parser::parse;
//!
//! let source = r#"
//! <script>
//!     let count = $state(0);
//! </script>
//!
//! <button onclick={() => count++}>
//!     Count: {count}
//! </button>
//! "#;
//!
//! let result = parse(source);
//! if result.errors.is_empty() {
//!     println!("Parsed successfully!");
//! }
//! ```

mod ast;
mod error;
mod lexer;
mod parser;

pub use ast::*;
pub use error::{ParseError, ParseErrorKind};
pub use lexer::{Lexer, Token};
pub use source_map::Span;

/// Options for parsing Svelte files.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// Whether to enable tracing for debugging.
    pub trace: bool,
}

/// The result of parsing a Svelte file.
#[derive(Debug)]
pub struct ParseResult {
    /// The parsed document.
    pub document: SvelteDocument,
    /// Any errors encountered during parsing.
    pub errors: Vec<ParseError>,
}

/// Parses a Svelte source file into an AST.
///
/// This function will attempt to parse the entire file and recover from errors
/// where possible, returning both the AST and any errors encountered.
pub fn parse(source: &str) -> ParseResult {
    parse_with_options(source, ParseOptions::default())
}

/// Parses a Svelte source file with custom options.
pub fn parse_with_options(source: &str, options: ParseOptions) -> ParseResult {
    parser::Parser::new(source, options).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let result = parse("");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_parse_simple_element() {
        let result = parse("<div>hello</div>");
        assert!(result.errors.is_empty());
        assert_eq!(result.document.fragment.nodes.len(), 1);
    }

    #[test]
    fn test_parse_with_script() {
        let source = r#"<script>let x = 1;</script><div>{x}</div>"#;
        let result = parse(source);
        assert!(result.errors.is_empty());
        assert!(result.document.instance_script.is_some());
    }
}

#[test]
fn test_comment_with_apostrophe_in_expression() {
    // Regression test: apostrophes in JS comments inside expressions
    // were being treated as string delimiters
    let source = r#"<script></script>
<div onclick={() => {
    // When enabling times per day, pre-populate each day's start and end times with
    // the values from startTime and endTime, if any. Then clear startTime and endTime,
    // as those fields will be removed.
    console.log('clicked');
}}></div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_multiline_comment_in_expression() {
    // Ensure /* */ comments are also handled
    let source = r#"<script></script>
<div onclick={() => {
    /* This is a multi-line comment
       with an apostrophe: it's great
       and some "quotes" too */
    console.log('clicked');
}}></div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

// === Style Directive Tests ===
// Tests for issue #9 and comprehensive style directive support

#[test]
fn test_style_directive_css_custom_property() {
    // Issue #9: CSS custom properties (variables) starting with --
    let source = r#"<svg style:--icon-compensate={compensate}><path d=""/></svg>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors for style:--icon-compensate, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_css_custom_property_complex() {
    // Complex CSS custom property with ternary expression
    let source =
        r#"<svg style:--icon-compensate={compensate === 0 ? null : `${compensate}px`}></svg>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_string_value() {
    // style:color="red" - string value
    let source = r#"<div style:color="red">text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_expression_value() {
    // style:color={myColor} - expression value
    let source = r#"<div style:color={myColor}>text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_shorthand() {
    // style:color - shorthand form (uses variable named 'color')
    let source = r#"<div style:color>text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_multiple() {
    // Multiple style directives on single element
    let source = r#"<div style:color style:width="12rem" style:background-color={darkMode ? 'black' : 'white'}>text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_important_modifier() {
    // style:color|important="red" - important modifier
    let source = r#"<div style:color|important="red">text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_with_style_attribute() {
    // style: directive combined with style attribute
    let source = r#"<div style:color="red" style="font-size: 16px;">text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_kebab_case() {
    // style:background-color - kebab-case property name
    let source = r#"<div style:background-color="blue">text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_multiple_css_custom_properties() {
    // Multiple CSS custom properties
    let source =
        r#"<div style:--primary={primary} style:--secondary="blue" style:--spacing>text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_css_custom_property_important() {
    // CSS custom property with important modifier
    let source = r#"<div style:--my-color|important={color}>text</div>"#;
    let result = parse(source);
    assert!(
        result.errors.is_empty(),
        "Expected no errors, got: {:?}",
        result.errors
    );
}

// === Style Directive Error Cases ===

#[test]
fn test_style_directive_unclosed_brace() {
    // Unclosed brace in style directive expression
    let source = r#"<div style:color={myColor>text</div>"#;
    let result = parse(source);
    assert!(
        !result.errors.is_empty(),
        "Expected parse error for unclosed brace in style directive"
    );
}

#[test]
fn test_style_directive_unclosed_brace_complex() {
    // More complex unclosed brace case
    let source =
        r#"<div style:--icon-compensate={compensate === 0 ? null : `${compensate}px`>text</div>"#;
    let result = parse(source);
    assert!(
        !result.errors.is_empty(),
        "Expected parse error for unclosed brace in CSS custom property style directive"
    );
}

#[test]
fn test_style_directive_empty_name() {
    // Empty style directive name should produce InvalidDirective error
    let source = r#"<div style:>text</div>"#;
    let result = parse(source);
    assert!(
        !result.errors.is_empty(),
        "Expected parse error for empty style directive name"
    );
    // Check that the error message mentions "name cannot be empty"
    let has_empty_name_error = result
        .errors
        .iter()
        .any(|e| e.to_string().contains("name cannot be empty"));
    assert!(
        has_empty_name_error,
        "Expected 'name cannot be empty' error, got: {:?}",
        result.errors
    );
}

#[test]
fn test_style_directive_missing_value() {
    // Missing value after = should be handled
    let source = r#"<div style:color=>text</div>"#;
    let result = parse(source);
    // This may or may not error depending on parser behavior
    // The important thing is it doesn't crash
    let _ = result;
}
