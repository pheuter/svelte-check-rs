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
