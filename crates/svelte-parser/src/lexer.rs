//! Svelte lexer using logos.
//!
//! The lexer handles tokenization of Svelte syntax including:
//! - HTML tokens (tags, attributes, text)
//! - Svelte-specific tokens (blocks, expressions)
//! - Script and style tag content

use logos::Logos;
use source_map::Span;
use text_size::TextSize;

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,
    /// The span of the token in the source.
    pub span: Span,
}

/// Token kinds for Svelte syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Logos, Default)]
#[logos(skip r"[ \t\r]+")]
pub enum TokenKind {
    // === HTML Tokens ===
    /// `<`
    #[token("<", priority = 10)]
    LAngle,

    /// `>`
    #[token(">", priority = 10)]
    RAngle,

    /// `/>`
    #[token("/>", priority = 10)]
    SlashRAngle,

    /// `</`
    #[token("</", priority = 10)]
    LAngleSlash,

    /// `=`
    #[token("=", priority = 10)]
    Eq,

    /// `"`
    #[token("\"", priority = 10)]
    DoubleQuote,

    /// `'`
    #[token("'", priority = 10)]
    SingleQuote,

    // === Svelte Block Tokens ===
    /// `{`
    #[token("{", priority = 10)]
    LBrace,

    /// `}`
    #[token("}", priority = 10)]
    RBrace,

    /// `{#`
    #[token("{#", priority = 11)]
    LBraceHash,

    /// `{/`
    #[token("{/", priority = 11)]
    LBraceSlash,

    /// `{:`
    #[token("{:", priority = 11)]
    LBraceColon,

    /// `{@`
    #[token("{@", priority = 11)]
    LBraceAt,

    // === Block Keywords ===
    /// `if`
    #[token("if", priority = 5)]
    If,

    /// `else`
    #[token("else", priority = 5)]
    Else,

    /// `each`
    #[token("each", priority = 5)]
    Each,

    /// `await`
    #[token("await", priority = 5)]
    Await,

    /// `then`
    #[token("then", priority = 5)]
    Then,

    /// `catch`
    #[token("catch", priority = 5)]
    Catch,

    /// `key`
    #[token("key", priority = 5)]
    Key,

    /// `snippet`
    #[token("snippet", priority = 5)]
    Snippet,

    // === Tag Keywords ===
    /// `html`
    #[token("html", priority = 5)]
    Html,

    /// `const`
    #[token("const", priority = 5)]
    Const,

    /// `debug`
    #[token("debug", priority = 5)]
    Debug,

    /// `render`
    #[token("render", priority = 5)]
    Render,

    // === Special Tokens ===
    /// `script`
    #[token("script", priority = 5)]
    Script,

    /// `style`
    #[token("style", priority = 5)]
    Style,

    /// `as`
    #[token("as", priority = 5)]
    As,

    /// `,`
    #[token(",", priority = 10)]
    Comma,

    /// `(`
    #[token("(", priority = 10)]
    LParen,

    /// `)`
    #[token(")", priority = 10)]
    RParen,

    /// `:`
    #[token(":", priority = 10)]
    Colon,

    /// `|`
    #[token("|", priority = 10)]
    Pipe,

    /// Newline
    #[token("\n", priority = 10)]
    Newline,

    /// An identifier (tag name, attribute name, etc.)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_\-]*", priority = 4)]
    Ident,

    /// A namespace prefix (e.g., `svelte:`)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*:", priority = 5)]
    NamespacedIdent,

    /// A number
    #[regex(r"[0-9]+", priority = 4)]
    Number,

    /// Slash character (for closing tags without space like `</div>`)
    #[token("/", priority = 10)]
    Slash,

    /// Text content - used sparingly, most text is handled by read_until
    /// Only matches specific punctuation that appears in text content
    #[regex(r"[.!?;#@$%^&*\[\]~`]+", priority = 1)]
    Text,

    /// End of file
    Eof,

    /// Invalid/unknown token
    #[default]
    Error,
}

impl TokenKind {
    /// Returns true if this token can start an expression.
    pub fn can_start_expression(&self) -> bool {
        matches!(self, TokenKind::LBrace)
    }

    /// Returns a human-readable name for this token kind.
    pub fn name(&self) -> &'static str {
        match self {
            TokenKind::LAngle => "'<'",
            TokenKind::RAngle => "'>'",
            TokenKind::SlashRAngle => "'/>'",
            TokenKind::LAngleSlash => "'</'",
            TokenKind::Eq => "'='",
            TokenKind::DoubleQuote => "'\"'",
            TokenKind::SingleQuote => "'''",
            TokenKind::LBrace => "'{'",
            TokenKind::RBrace => "'}'",
            TokenKind::LBraceHash => "'{#'",
            TokenKind::LBraceSlash => "'{/'",
            TokenKind::LBraceColon => "'{:'",
            TokenKind::LBraceAt => "'{@'",
            TokenKind::If => "'if'",
            TokenKind::Else => "'else'",
            TokenKind::Each => "'each'",
            TokenKind::Await => "'await'",
            TokenKind::Then => "'then'",
            TokenKind::Catch => "'catch'",
            TokenKind::Key => "'key'",
            TokenKind::Snippet => "'snippet'",
            TokenKind::Html => "'html'",
            TokenKind::Const => "'const'",
            TokenKind::Debug => "'debug'",
            TokenKind::Render => "'render'",
            TokenKind::Script => "'script'",
            TokenKind::Style => "'style'",
            TokenKind::As => "'as'",
            TokenKind::Comma => "','",
            TokenKind::LParen => "'('",
            TokenKind::RParen => "')'",
            TokenKind::Colon => "':'",
            TokenKind::Pipe => "'|'",
            TokenKind::Newline => "newline",
            TokenKind::Ident => "identifier",
            TokenKind::NamespacedIdent => "namespaced identifier",
            TokenKind::Number => "number",
            TokenKind::Slash => "'/'",
            TokenKind::Text => "text",
            TokenKind::Eof => "end of file",
            TokenKind::Error => "invalid token",
        }
    }
}

/// A lexer for Svelte source code.
pub struct Lexer<'src> {
    inner: logos::Lexer<'src, TokenKind>,
    source: &'src str,
    finished: bool,
}

impl<'src> Lexer<'src> {
    /// Creates a new lexer for the given source.
    pub fn new(source: &'src str) -> Self {
        Self {
            inner: TokenKind::lexer(source),
            source,
            finished: false,
        }
    }

    /// Returns the source string being lexed.
    pub fn source(&self) -> &'src str {
        self.source
    }

    /// Returns the text of the current token.
    pub fn slice(&self) -> &'src str {
        self.inner.slice()
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        match self.inner.next() {
            Some(Ok(kind)) => {
                let span = self.inner.span();
                Some(Token {
                    kind,
                    span: Span::new(
                        TextSize::from(span.start as u32),
                        TextSize::from(span.end as u32),
                    ),
                })
            }
            Some(Err(())) => {
                let span = self.inner.span();
                Some(Token {
                    kind: TokenKind::Error,
                    span: Span::new(
                        TextSize::from(span.start as u32),
                        TextSize::from(span.end as u32),
                    ),
                })
            }
            None => {
                self.finished = true;
                let end = TextSize::from(self.source.len() as u32);
                Some(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(end, end),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<TokenKind> {
        Lexer::new(source)
            .map(|t| t.kind)
            .filter(|k| *k != TokenKind::Eof)
            .collect()
    }

    #[test]
    fn test_simple_tag() {
        let tokens = tokenize("<div>");
        assert_eq!(
            tokens,
            vec![TokenKind::LAngle, TokenKind::Ident, TokenKind::RAngle]
        );
    }

    #[test]
    fn test_self_closing_tag() {
        let tokens = tokenize("<br/>");
        assert_eq!(
            tokens,
            vec![TokenKind::LAngle, TokenKind::Ident, TokenKind::SlashRAngle]
        );
    }

    #[test]
    fn test_closing_tag() {
        let tokens = tokenize("</div>");
        assert_eq!(
            tokens,
            vec![TokenKind::LAngleSlash, TokenKind::Ident, TokenKind::RAngle]
        );
    }

    #[test]
    fn test_if_block() {
        let tokens = tokenize("{#if condition}");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBraceHash,
                TokenKind::If,
                TokenKind::Ident,
                TokenKind::RBrace
            ]
        );
    }

    #[test]
    fn test_expression() {
        let tokens = tokenize("{value}");
        assert_eq!(
            tokens,
            vec![TokenKind::LBrace, TokenKind::Ident, TokenKind::RBrace]
        );
    }

    #[test]
    fn test_snippet_block() {
        let tokens = tokenize("{#snippet name()}");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBraceHash,
                TokenKind::Snippet,
                TokenKind::Ident,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::RBrace
            ]
        );
    }

    #[test]
    fn test_render_tag() {
        let tokens = tokenize("{@render name()}");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBraceAt,
                TokenKind::Render,
                TokenKind::Ident,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::RBrace
            ]
        );
    }
}
