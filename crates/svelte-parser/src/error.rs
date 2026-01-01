//! Parse error types.

use source_map::Span;
use thiserror::Error;

/// An error that occurred during parsing.
#[derive(Debug, Clone, Error)]
#[error("{kind}")]
pub struct ParseError {
    /// The kind of error.
    pub kind: ParseErrorKind,
    /// The location in the source where the error occurred.
    pub span: Span,
}

impl ParseError {
    /// Creates a new parse error.
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// The kind of parse error.
#[derive(Debug, Clone, Error)]
pub enum ParseErrorKind {
    /// An unexpected token was encountered.
    #[error("unexpected token: expected {expected}, found {found}")]
    UnexpectedToken {
        /// What was expected.
        expected: String,
        /// What was found.
        found: String,
    },

    /// An unexpected end of file was encountered.
    #[error("unexpected end of file: expected {expected}")]
    UnexpectedEof {
        /// What was expected.
        expected: String,
    },

    /// An unclosed tag was found.
    #[error("unclosed tag: <{tag_name}>")]
    UnclosedTag {
        /// The name of the unclosed tag.
        tag_name: String,
    },

    /// A mismatched closing tag was found.
    #[error("mismatched closing tag: expected </{expected}>, found </{found}>")]
    MismatchedClosingTag {
        /// The expected tag name.
        expected: String,
        /// The found tag name.
        found: String,
    },

    /// An unclosed block was found.
    #[error("unclosed block: {{#{block_type}}}")]
    UnclosedBlock {
        /// The type of block (if, each, await, key, snippet).
        block_type: String,
    },

    /// An invalid attribute was found.
    #[error("invalid attribute: {message}")]
    InvalidAttribute {
        /// A description of the problem.
        message: String,
    },

    /// An invalid expression was found.
    #[error("invalid expression: {message}")]
    InvalidExpression {
        /// A description of the problem.
        message: String,
    },

    /// An invalid directive was found.
    #[error("invalid directive: {message}")]
    InvalidDirective {
        /// A description of the problem.
        message: String,
    },

    /// A duplicate attribute was found.
    #[error("duplicate attribute: {name}")]
    DuplicateAttribute {
        /// The name of the duplicated attribute.
        name: String,
    },

    /// An invalid tag name was found.
    #[error("invalid tag name: {name}")]
    InvalidTagName {
        /// The invalid tag name.
        name: String,
    },

    /// An invalid block syntax was found.
    #[error("invalid block syntax: {message}")]
    InvalidBlockSyntax {
        /// A description of the problem.
        message: String,
    },

    /// A generic syntax error.
    #[error("{message}")]
    SyntaxError {
        /// A description of the error.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use text_size::TextSize;

    #[test]
    fn test_error_display() {
        let error = ParseError::new(
            ParseErrorKind::UnexpectedToken {
                expected: "tag name".to_string(),
                found: "}".to_string(),
            },
            Span::new(TextSize::from(0), TextSize::from(1)),
        );
        assert_eq!(
            error.to_string(),
            "unexpected token: expected tag name, found }"
        );
    }
}
