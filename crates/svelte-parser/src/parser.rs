//! Recursive descent parser for Svelte 5.

use crate::ast::*;
use crate::error::{ParseError, ParseErrorKind};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::{ParseOptions, ParseResult};
use smol_str::SmolStr;
use source_map::Span;
use text_size::TextSize;

/// HTML void elements that are self-closing and should not have closing tags.
/// See: https://developer.mozilla.org/en-US/docs/Glossary/Void_element
const HTML_VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Returns true if the given element name is an HTML void element.
fn is_void_element(name: &str) -> bool {
    HTML_VOID_ELEMENTS.contains(&name.to_lowercase().as_str())
}

/// The Svelte parser.
pub struct Parser<'src> {
    /// The source being parsed.
    source: &'src str,
    /// The lexer.
    tokens: Vec<Token>,
    /// Current position in the token stream.
    pos: usize,
    /// Parse errors collected during parsing.
    errors: Vec<ParseError>,
    /// Parser options.
    #[allow(dead_code)]
    options: ParseOptions,
    /// EOF token for when we're past the end
    eof_token: Token,
}

impl<'src> Parser<'src> {
    /// Creates a new parser.
    pub fn new(source: &'src str, options: ParseOptions) -> Self {
        let tokens: Vec<Token> = Lexer::new(source).collect();
        let eof_token = Token {
            kind: TokenKind::Eof,
            span: Span::empty(TextSize::from(source.len() as u32)),
        };
        Self {
            source,
            tokens,
            pos: 0,
            errors: Vec::new(),
            options,
            eof_token,
        }
    }

    /// Parses the source into a Svelte document.
    pub fn parse(mut self) -> ParseResult {
        let document = self.parse_document();
        ParseResult {
            document,
            errors: self.errors,
        }
    }

    // === Token helpers ===

    /// Returns the current token.
    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }

    /// Returns the current token kind.
    fn current_kind(&self) -> TokenKind {
        self.current().kind
    }

    /// Returns the text of the current token.
    fn current_text(&self) -> &str {
        let span = self.current().span;
        &self.source[u32::from(span.start) as usize..u32::from(span.end) as usize]
    }

    /// Advances to the next token.
    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    /// Checks if the current token matches the given kind.
    fn check(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    /// Advances if the current token matches, returns true if matched.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Expects the current token to be the given kind, reports error if not.
    fn expect(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            self.error(ParseErrorKind::UnexpectedToken {
                expected: kind.name().to_string(),
                found: self.current_kind().name().to_string(),
            });
            false
        }
    }

    /// Reports an error at the current position.
    fn error(&mut self, kind: ParseErrorKind) {
        self.errors.push(ParseError::new(kind, self.current().span));
    }

    /// Skips whitespace and newlines.
    fn skip_whitespace(&mut self) {
        while self.check(TokenKind::Newline) || self.check(TokenKind::Text) {
            let text = self.current_text();
            if text.chars().all(|c| c.is_whitespace()) {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Reads text until the given delimiter, returning the text and its span.
    fn read_until(&mut self, delimiters: &[&str]) -> (String, Span) {
        let start = self.current().span.start;
        let mut text = String::new();
        let start_offset = u32::from(start) as usize;

        // Find the position of the nearest delimiter
        let remaining = &self.source[start_offset..];
        let end_pos = delimiters
            .iter()
            .filter_map(|d| remaining.find(d))
            .min()
            .unwrap_or(remaining.len());

        text.push_str(&remaining[..end_pos]);
        let end = TextSize::from((start_offset + end_pos) as u32);

        // Advance past the tokens we consumed
        while self.current().span.start < end && !self.check(TokenKind::Eof) {
            self.advance();
        }

        (text, Span::new(start, end))
    }

    /// Reads an expression until the given closing delimiter, respecting nested braces.
    /// This handles cases like `{items.map(x => { return x; })}` correctly.
    /// Also properly handles template literals with embedded expressions like `${expr}`.
    fn read_expression_until(&mut self, close_char: char) -> (String, Span) {
        let start = self.current().span.start;
        let start_offset = u32::from(start) as usize;

        let mut depth = 0;
        let mut in_string = false;
        let mut in_template_literal = false;
        let mut template_expr_depth = 0; // Track depth within template expressions
        let mut string_char = ' ';
        let mut pos = start_offset;
        let bytes = self.source.as_bytes();
        // Track previous non-whitespace char to determine if / starts a regex
        let mut prev_non_ws: char = '='; // Start with '=' so first / is treated as regex

        let mut chars = self.source[start_offset..].char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let absolute_i = start_offset + i;
            // Count consecutive preceding backslashes. An even count means the backslashes
            // are paired (escaped backslashes) and the current character is NOT escaped.
            // A naive `bytes[pos-1] == '\\'` check fails for `\\'` because both backslashes
            // form an escaped backslash, yet the quote following them appears to have a `\`
            // predecessor.
            let preceding_backslashes = (0..absolute_i)
                .rev()
                .take_while(|&j| bytes.get(j) == Some(&b'\\'))
                .count();
            let is_escaped = preceding_backslashes % 2 == 1;

            if in_string && !in_template_literal {
                // Regular string - just look for end quote
                if c == string_char && !is_escaped {
                    in_string = false;
                    prev_non_ws = c;
                }
                pos = start_offset + i + c.len_utf8();
                continue;
            }

            if in_template_literal {
                if c == '`' && !is_escaped && template_expr_depth == 0 {
                    // End of template literal (not inside a ${...})
                    in_template_literal = false;
                    in_string = false;
                    prev_non_ws = c;
                } else if c == '$' && template_expr_depth == 0 {
                    // Check for ${ to enter expression
                    if let Some(&(_, '{')) = chars.peek() {
                        chars.next(); // consume '{'
                        template_expr_depth = 1;
                        pos = start_offset + i + 2;
                        prev_non_ws = '{';
                        continue;
                    }
                } else if template_expr_depth > 0 {
                    // Inside a ${...} expression
                    match c {
                        '`' => {
                            // Nested template literal - need to skip it entirely
                            // Track nested template depth (including ${} inside nested templates)
                            let mut nested_depth = 1;
                            let mut nested_expr_depth = 0;
                            while nested_depth > 0 {
                                if let Some((ni, nc)) = chars.next() {
                                    let nested_abs_i = start_offset + ni;
                                    let nc_escaped = nested_abs_i > 0
                                        && bytes.get(nested_abs_i - 1) == Some(&b'\\');

                                    if nested_expr_depth == 0 {
                                        // In template literal text
                                        if nc == '`' && !nc_escaped {
                                            nested_depth -= 1;
                                        } else if nc == '$' {
                                            if let Some(&(_, '{')) = chars.peek() {
                                                chars.next();
                                                nested_expr_depth += 1;
                                            }
                                        }
                                    } else {
                                        // In ${...} expression inside nested template
                                        match nc {
                                            '`' => nested_depth += 1, // Even deeper nested template
                                            '{' => nested_expr_depth += 1,
                                            '}' => nested_expr_depth -= 1,
                                            '"' | '\'' => {
                                                // Skip string inside nested expression
                                                let quote = nc;
                                                for (si, sc) in chars.by_ref() {
                                                    let string_abs_i = start_offset + si;
                                                    let sc_escaped = string_abs_i > 0
                                                        && bytes.get(string_abs_i - 1)
                                                            == Some(&b'\\');
                                                    if sc == quote && !sc_escaped {
                                                        break;
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }
                            prev_non_ws = '`';
                        }
                        '/' => {
                            // Check for JavaScript comments inside template expression
                            if let Some(&(_, next_c)) = chars.peek() {
                                if next_c == '/' {
                                    // Single-line comment - skip until end of line
                                    chars.next();
                                    for (_, sc) in chars.by_ref() {
                                        if sc == '\n' {
                                            break;
                                        }
                                    }
                                } else if next_c == '*' {
                                    // Multi-line comment - skip until */
                                    chars.next();
                                    let mut prev_star = false;
                                    for (_, sc) in chars.by_ref() {
                                        if prev_star && sc == '/' {
                                            break;
                                        }
                                        prev_star = sc == '*';
                                    }
                                } else if Self::could_start_regex(prev_non_ws) {
                                    // Regex literal inside template expression - skip it
                                    Self::skip_regex_literal(&mut chars, bytes, start_offset);
                                }
                            } else if Self::could_start_regex(prev_non_ws) {
                                // Regex at end - skip it
                                Self::skip_regex_literal(&mut chars, bytes, start_offset);
                            }
                            prev_non_ws = '/';
                        }
                        '"' | '\'' => {
                            // String inside template expression
                            let quote = c;
                            for (si, sc) in chars.by_ref() {
                                let string_abs_i = start_offset + si;
                                let preceding_backslashes = (0..string_abs_i)
                                    .rev()
                                    .take_while(|&j| bytes.get(j) == Some(&b'\\'))
                                    .count();
                                let sc_escaped = preceding_backslashes % 2 == 1;
                                if sc == quote && !sc_escaped {
                                    break;
                                }
                            }
                            prev_non_ws = c;
                        }
                        '{' => {
                            template_expr_depth += 1;
                            prev_non_ws = c;
                        }
                        '}' => {
                            template_expr_depth -= 1;
                            prev_non_ws = c;
                            // When template_expr_depth becomes 0, we're back in template string
                        }
                        _ => {
                            if !c.is_whitespace() {
                                prev_non_ws = c;
                            }
                        }
                    }
                }
                pos = start_offset + i + c.len_utf8();
                continue;
            }

            match c {
                '/' => {
                    // Check for JavaScript comments or regex literals
                    if let Some(&(_, next_c)) = chars.peek() {
                        if next_c == '/' {
                            // Single-line comment - skip until end of line
                            chars.next(); // consume second '/'
                            for (_, sc) in chars.by_ref() {
                                if sc == '\n' {
                                    break;
                                }
                            }
                        } else if next_c == '*' {
                            // Multi-line comment - skip until */
                            chars.next(); // consume '*'
                            let mut prev_star = false;
                            for (_, sc) in chars.by_ref() {
                                if prev_star && sc == '/' {
                                    break;
                                }
                                prev_star = sc == '*';
                            }
                        } else if Self::could_start_regex(prev_non_ws) {
                            // Regex literal - skip the entire regex
                            Self::skip_regex_literal(&mut chars, bytes, start_offset);
                        }
                    } else if Self::could_start_regex(prev_non_ws) {
                        // Regex at end - skip it
                        Self::skip_regex_literal(&mut chars, bytes, start_offset);
                    }
                    prev_non_ws = '/';
                }
                '"' | '\'' => {
                    in_string = true;
                    string_char = c;
                    prev_non_ws = c;
                }
                '`' => {
                    in_string = true;
                    in_template_literal = true;
                    prev_non_ws = c;
                }
                '{' | '(' | '[' => {
                    depth += 1;
                    prev_non_ws = c;
                }
                '}' if close_char == '}' => {
                    if depth == 0 {
                        pos = start_offset + i;
                        break;
                    }
                    depth -= 1;
                    prev_non_ws = c;
                }
                ')' if close_char == ')' => {
                    if depth == 0 {
                        pos = start_offset + i;
                        break;
                    }
                    depth -= 1;
                    prev_non_ws = c;
                }
                ']' if close_char == ']' => {
                    if depth == 0 {
                        pos = start_offset + i;
                        break;
                    }
                    depth -= 1;
                    prev_non_ws = c;
                }
                '}' => {
                    depth -= 1;
                    prev_non_ws = c;
                }
                ')' => {
                    depth -= 1;
                    prev_non_ws = c;
                }
                ']' => {
                    depth -= 1;
                    prev_non_ws = c;
                }
                _ => {
                    if !c.is_whitespace() {
                        prev_non_ws = c;
                    }
                }
            }
            pos = start_offset + i + c.len_utf8();
        }

        let text = self.source[start_offset..pos].to_string();
        let end = TextSize::from(pos as u32);

        // Advance past the tokens we consumed
        while self.current().span.start < end && !self.check(TokenKind::Eof) {
            self.advance();
        }

        (text, Span::new(start, end))
    }

    /// Determines if a `/` following `prev_char` could start a regex literal.
    /// Returns true if regex is likely, false if division is likely.
    fn could_start_regex(prev_char: char) -> bool {
        // `/` starts a regex when preceded by:
        // - Operators: = ! + - * % < > & | ^ ~ ? :
        // - Open brackets: ( [ {
        // - Comma, semicolon: , ;
        // - Start of expression (we use '=' as initial value)
        //
        // `/` is division when preceded by:
        // - Close brackets: ) ] }
        // - Identifiers (letters, digits, underscore, $)
        // - Quotes (end of string literal)
        matches!(
            prev_char,
            '=' | '!'
                | '+'
                | '-'
                | '*'
                | '%'
                | '<'
                | '>'
                | '&'
                | '|'
                | '^'
                | '~'
                | '?'
                | ':'
                | '('
                | '['
                | '{'
                | ','
                | ';'
                | '\n'
        )
    }

    /// Skips a regex literal, handling escape sequences and character classes.
    /// Assumes the opening `/` has already been consumed.
    fn skip_regex_literal(
        chars: &mut std::iter::Peekable<std::str::CharIndices>,
        bytes: &[u8],
        start_offset: usize,
    ) {
        let mut in_char_class = false;

        while let Some((ri, rc)) = chars.next() {
            let regex_abs_i = start_offset + ri;
            let rc_escaped = regex_abs_i > 0 && bytes.get(regex_abs_i - 1) == Some(&b'\\');

            if rc_escaped {
                // Escaped character - skip it
                continue;
            }

            match rc {
                '/' if !in_char_class => {
                    // End of regex - now skip flags (g, i, m, s, u, y, d, v)
                    while let Some(&(_, fc)) = chars.peek() {
                        if fc.is_ascii_alphabetic() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    return;
                }
                '[' if !in_char_class => {
                    in_char_class = true;
                }
                ']' if in_char_class => {
                    in_char_class = false;
                }
                '\n' => {
                    // Unterminated regex - stop here
                    return;
                }
                _ => {}
            }
        }
    }

    /// Reads an expression from a specific source offset until the closing character.
    /// Unlike `read_expression_until`, this doesn't advance tokens - caller must handle that.
    /// Used when the lexer has already tokenized past the opening brace (e.g., LBraceSlash for `{/`).
    fn read_expression_from_offset(&self, start_offset: usize, close_char: char) -> (String, Span) {
        let start = TextSize::from(start_offset as u32);
        let mut depth = 0;
        let mut in_string = false;
        let mut in_template_literal = false;
        let mut template_expr_depth = 0;
        let mut string_char = ' ';
        let mut pos = start_offset;
        let bytes = self.source.as_bytes();
        // Track previous non-whitespace char to determine if / starts a regex
        let mut prev_non_ws: char = '='; // Start with '=' so first / is treated as regex

        let mut chars = self.source[start_offset..].char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let absolute_i = start_offset + i;
            let preceding_backslashes = (0..absolute_i)
                .rev()
                .take_while(|&j| bytes.get(j) == Some(&b'\\'))
                .count();
            let is_escaped = preceding_backslashes % 2 == 1;

            if in_string && !in_template_literal {
                if c == string_char && !is_escaped {
                    in_string = false;
                    prev_non_ws = c;
                }
                pos = start_offset + i + c.len_utf8();
                continue;
            }

            if in_template_literal {
                if c == '`' && !is_escaped && template_expr_depth == 0 {
                    in_template_literal = false;
                    in_string = false;
                    prev_non_ws = c;
                } else if c == '$' && template_expr_depth == 0 {
                    if let Some(&(_, '{')) = chars.peek() {
                        chars.next();
                        template_expr_depth = 1;
                        pos = start_offset + i + 2;
                        prev_non_ws = '{';
                        continue;
                    }
                } else if template_expr_depth > 0 {
                    match c {
                        '/' => {
                            if let Some(&(_, next_c)) = chars.peek() {
                                if next_c == '/' {
                                    chars.next();
                                    for (_, sc) in chars.by_ref() {
                                        if sc == '\n' {
                                            break;
                                        }
                                    }
                                } else if next_c == '*' {
                                    chars.next();
                                    let mut prev_star = false;
                                    for (_, sc) in chars.by_ref() {
                                        if prev_star && sc == '/' {
                                            break;
                                        }
                                        prev_star = sc == '*';
                                    }
                                } else if Self::could_start_regex(prev_non_ws) {
                                    Self::skip_regex_literal(&mut chars, bytes, start_offset);
                                }
                            } else if Self::could_start_regex(prev_non_ws) {
                                Self::skip_regex_literal(&mut chars, bytes, start_offset);
                            }
                            prev_non_ws = '/';
                        }
                        '"' | '\'' => {
                            let quote = c;
                            for (si, sc) in chars.by_ref() {
                                let string_abs_i = start_offset + si;
                                let preceding_bs = (0..string_abs_i)
                                    .rev()
                                    .take_while(|&j| bytes.get(j) == Some(&b'\\'))
                                    .count();
                                let sc_escaped = preceding_bs % 2 == 1;
                                if sc == quote && !sc_escaped {
                                    break;
                                }
                            }
                            prev_non_ws = c;
                        }
                        '{' => {
                            template_expr_depth += 1;
                            prev_non_ws = c;
                        }
                        '}' => {
                            template_expr_depth -= 1;
                            prev_non_ws = c;
                        }
                        _ => {
                            if !c.is_whitespace() {
                                prev_non_ws = c;
                            }
                        }
                    }
                }
                pos = start_offset + i + c.len_utf8();
                continue;
            }

            match c {
                '/' => {
                    if let Some(&(_, next_c)) = chars.peek() {
                        if next_c == '/' {
                            chars.next();
                            for (_, sc) in chars.by_ref() {
                                if sc == '\n' {
                                    break;
                                }
                            }
                        } else if next_c == '*' {
                            chars.next();
                            let mut prev_star = false;
                            for (_, sc) in chars.by_ref() {
                                if prev_star && sc == '/' {
                                    break;
                                }
                                prev_star = sc == '*';
                            }
                        } else if Self::could_start_regex(prev_non_ws) {
                            Self::skip_regex_literal(&mut chars, bytes, start_offset);
                        }
                    } else if Self::could_start_regex(prev_non_ws) {
                        Self::skip_regex_literal(&mut chars, bytes, start_offset);
                    }
                    prev_non_ws = '/';
                }
                '"' | '\'' => {
                    in_string = true;
                    string_char = c;
                    prev_non_ws = c;
                }
                '`' => {
                    in_string = true;
                    in_template_literal = true;
                    prev_non_ws = c;
                }
                '{' | '(' | '[' => {
                    depth += 1;
                    prev_non_ws = c;
                }
                '}' if close_char == '}' => {
                    if depth == 0 {
                        pos = start_offset + i;
                        break;
                    }
                    depth -= 1;
                    prev_non_ws = c;
                }
                ')' if close_char == ')' => {
                    if depth == 0 {
                        pos = start_offset + i;
                        break;
                    }
                    depth -= 1;
                    prev_non_ws = c;
                }
                ']' if close_char == ']' => {
                    if depth == 0 {
                        pos = start_offset + i;
                        break;
                    }
                    depth -= 1;
                    prev_non_ws = c;
                }
                '}' => {
                    depth -= 1;
                    prev_non_ws = c;
                }
                ')' => {
                    depth -= 1;
                    prev_non_ws = c;
                }
                ']' => {
                    depth -= 1;
                    prev_non_ws = c;
                }
                _ => {
                    if !c.is_whitespace() {
                        prev_non_ws = c;
                    }
                }
            }
            pos = start_offset + i + c.len_utf8();
        }

        let text = self.source[start_offset..pos].to_string();
        let end = TextSize::from(pos as u32);
        (text, Span::new(start, end))
    }

    /// Finds a keyword in an expression, respecting nesting and string boundaries.
    /// Returns the position of the keyword if found at depth 0.
    /// Note: For keywords with spaces like " as " or " then ", word boundaries are
    /// already handled by the spaces in the keyword itself.
    fn find_keyword_in_expr(expr: &str, keyword: &str) -> Option<usize> {
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut string_char = ' ';
        let bytes = expr.as_bytes();

        let mut i = 0;
        while i < expr.len() {
            let c = expr[i..].chars().next().unwrap();

            if in_string {
                let preceding_bs = (0..i).rev().take_while(|&j| bytes.get(j) == Some(&b'\\')).count();
                let is_escaped = preceding_bs % 2 == 1;
                if c == string_char && !is_escaped {
                    in_string = false;
                }
                i += c.len_utf8();
                continue;
            }

            match c {
                '"' | '\'' | '`' => {
                    in_string = true;
                    string_char = c;
                }
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth = depth.saturating_sub(1),
                _ if depth == 0 && expr[i..].starts_with(keyword) => {
                    return Some(i);
                }
                _ => {}
            }
            i += c.len_utf8();
        }
        None
    }

    /// Finds " as" followed by any whitespace in an expression at depth 0.
    /// Returns (position, match_length) where match_length includes the trailing whitespace.
    /// This handles multi-line expressions where " as " may be " as\n" or " as\t".
    ///
    /// Note: This is a specialized variant of `find_keyword_in_expr` that handles
    /// flexible trailing whitespace for the " as" keyword in {#each} blocks.
    fn find_as_keyword_in_expr(expr: &str) -> Option<(usize, usize)> {
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut string_char = ' ';
        let bytes = expr.as_bytes();

        let mut i = 0;
        while i < expr.len() {
            let c = expr[i..].chars().next().unwrap();

            if in_string {
                let preceding_bs = (0..i).rev().take_while(|&j| bytes.get(j) == Some(&b'\\')).count();
                let is_escaped = preceding_bs % 2 == 1;
                if c == string_char && !is_escaped {
                    in_string = false;
                }
                i += c.len_utf8();
                continue;
            }

            match c {
                '"' | '\'' | '`' => {
                    in_string = true;
                    string_char = c;
                }
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth = depth.saturating_sub(1),
                ' ' if depth == 0 && expr[i..].starts_with(" as") => {
                    // Check if followed by whitespace (space, tab, newline, etc.)
                    if let Some(ws_char) = expr.get(i + 3..).and_then(|s| s.chars().next()) {
                        if ws_char.is_whitespace() {
                            return Some((i, 3 + ws_char.len_utf8()));
                        }
                    }
                }
                _ => {}
            }
            i += c.len_utf8();
        }
        None
    }

    /// Finds a character in an expression at depth 0, respecting nesting.
    fn find_char_in_expr(expr: &str, target: char) -> Option<usize> {
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut string_char = ' ';
        let bytes = expr.as_bytes();

        for (i, c) in expr.char_indices() {
            if in_string {
                let preceding_bs = (0..i).rev().take_while(|&j| bytes.get(j) == Some(&b'\\')).count();
                let is_escaped = preceding_bs % 2 == 1;
                if c == string_char && !is_escaped {
                    in_string = false;
                }
                continue;
            }

            // Check for target at depth 0 BEFORE updating depth
            if depth == 0 && c == target {
                return Some(i);
            }

            match c {
                '"' | '\'' | '`' => {
                    in_string = true;
                    string_char = c;
                }
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        None
    }

    // === Parsing methods ===

    /// Parses a complete Svelte document.
    fn parse_document(&mut self) -> SvelteDocument {
        let start = self.current().span.start;
        let mut doc = SvelteDocument::default();
        let mut nodes = Vec::new();

        while !self.check(TokenKind::Eof) {
            self.skip_whitespace();

            if self.check(TokenKind::Eof) {
                break;
            }

            // Check for script/style tags
            if self.check(TokenKind::LAngle) {
                let lookahead = self.peek_tag_name();

                if lookahead == "script" {
                    if let Some(script) = self.parse_script() {
                        if script.context == ScriptContext::Module {
                            doc.module_script = Some(script);
                        } else {
                            doc.instance_script = Some(script);
                        }
                        continue;
                    }
                } else if lookahead == "style" {
                    if let Some(style) = self.parse_style() {
                        doc.style = Some(style);
                        continue;
                    }
                }
            }

            // Parse template nodes
            if let Some(node) = self.parse_template_node() {
                nodes.push(node);
            } else {
                // Skip invalid token to avoid infinite loop
                self.advance();
            }
        }

        let end = if nodes.is_empty() {
            start
        } else {
            nodes.last().map(|n| n.span().end).unwrap_or(start)
        };

        doc.fragment = Fragment {
            nodes,
            span: Span::new(start, end),
        };
        doc.span = Span::new(start, TextSize::from(self.source.len() as u32));

        doc
    }

    /// Peeks at the tag name following a `<`.
    fn peek_tag_name(&self) -> &str {
        // Save position
        let mut peek_pos = self.pos + 1;

        // Skip whitespace
        while peek_pos < self.tokens.len() {
            let token = &self.tokens[peek_pos];
            if token.kind == TokenKind::Newline {
                peek_pos += 1;
            } else {
                break;
            }
        }

        // Get the identifier
        if peek_pos < self.tokens.len() {
            let token = &self.tokens[peek_pos];
            if token.kind == TokenKind::Ident
                || token.kind == TokenKind::Script
                || token.kind == TokenKind::Style
            {
                return &self.source
                    [u32::from(token.span.start) as usize..u32::from(token.span.end) as usize];
            }
        }

        ""
    }

    /// Parses a script block.
    fn parse_script(&mut self) -> Option<Script> {
        let start = self.current().span.start;

        // Expect `<`
        if !self.eat(TokenKind::LAngle) {
            return None;
        }

        // Expect `script`
        if !self.check(TokenKind::Script) && !self.check(TokenKind::Ident) {
            return None;
        }
        self.advance();

        // Parse attributes
        let mut attributes = Vec::new();
        let mut lang = ScriptLang::JavaScript;
        let mut context = ScriptContext::Default;

        loop {
            self.skip_whitespace();

            if self.check(TokenKind::RAngle) || self.check(TokenKind::SlashRAngle) {
                break;
            }

            if self.check(TokenKind::Ident) {
                let name = SmolStr::new(self.current_text());
                let attr_start = self.current().span.start;
                self.advance();

                let value = if self.eat(TokenKind::Eq) {
                    if self.eat(TokenKind::DoubleQuote) {
                        let (text, span) = self.read_until(&["\""]);
                        self.eat(TokenKind::DoubleQuote);
                        AttributeValue::Text(TextValue { span, value: text })
                    } else {
                        AttributeValue::True
                    }
                } else {
                    AttributeValue::True
                };

                // Check for special attributes
                if name == "lang" || name == "type" {
                    if let AttributeValue::Text(ref t) = value {
                        if t.value == "ts"
                            || t.value == "typescript"
                            || t.value.contains("typescript")
                        {
                            lang = ScriptLang::TypeScript;
                        }
                    }
                } else if name == "context" {
                    if let AttributeValue::Text(ref t) = value {
                        if t.value == "module" {
                            context = ScriptContext::Module;
                        }
                    }
                } else if name == "module" {
                    // Svelte 5 uses bare `module` attribute: <script module>
                    context = ScriptContext::Module;
                }

                let attr_end = self
                    .tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| t.span.end)
                    .unwrap_or(attr_start);

                attributes.push(Attribute::Normal(NormalAttribute {
                    span: Span::new(attr_start, attr_end),
                    name,
                    value,
                }));
            } else {
                break;
            }
        }

        // Expect `>`
        if !self.eat(TokenKind::RAngle) {
            return None;
        }

        // Read content until </script>
        let content_start = self.current().span.start;
        let (content, content_span) = self.read_until(&["</script>"]);

        // Expect `</script>`
        if self.check(TokenKind::LAngleSlash) {
            self.advance();
            // Skip `script`
            if self.check(TokenKind::Script) || self.check(TokenKind::Ident) {
                self.advance();
            }
            self.eat(TokenKind::RAngle);
        }

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(content_start);

        Some(Script {
            span: Span::new(start, end),
            content_span,
            content,
            lang,
            context,
            attributes,
        })
    }

    /// Parses a style block.
    fn parse_style(&mut self) -> Option<Style> {
        let start = self.current().span.start;

        // Expect `<`
        if !self.eat(TokenKind::LAngle) {
            return None;
        }

        // Expect `style`
        if !self.check(TokenKind::Style) && !self.check(TokenKind::Ident) {
            return None;
        }
        self.advance();

        // Parse attributes
        let mut attributes = Vec::new();
        let mut global = false;

        loop {
            self.skip_whitespace();

            if self.check(TokenKind::RAngle) || self.check(TokenKind::SlashRAngle) {
                break;
            }

            if self.check(TokenKind::Ident) {
                let name = SmolStr::new(self.current_text());
                let attr_start = self.current().span.start;
                self.advance();

                let value = if self.eat(TokenKind::Eq) {
                    if self.eat(TokenKind::DoubleQuote) {
                        let (text, span) = self.read_until(&["\""]);
                        self.eat(TokenKind::DoubleQuote);
                        AttributeValue::Text(TextValue { span, value: text })
                    } else {
                        AttributeValue::True
                    }
                } else {
                    AttributeValue::True
                };

                if name == "global" {
                    global = true;
                }

                let attr_end = self
                    .tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| t.span.end)
                    .unwrap_or(attr_start);

                attributes.push(Attribute::Normal(NormalAttribute {
                    span: Span::new(attr_start, attr_end),
                    name,
                    value,
                }));
            } else {
                break;
            }
        }

        // Expect `>`
        if !self.eat(TokenKind::RAngle) {
            return None;
        }

        // Read content until </style>
        let content_start = self.current().span.start;
        let (content, content_span) = self.read_until(&["</style>"]);

        // Expect `</style>`
        if self.check(TokenKind::LAngleSlash) {
            self.advance();
            if self.check(TokenKind::Style) || self.check(TokenKind::Ident) {
                self.advance();
            }
            self.eat(TokenKind::RAngle);
        }

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(content_start);

        Some(Style {
            span: Span::new(start, end),
            content_span,
            content,
            global,
            attributes,
        })
    }

    /// Parses a template node.
    fn parse_template_node(&mut self) -> Option<TemplateNode> {
        match self.current_kind() {
            TokenKind::LAngle => {
                // Check if this is a comment
                if self.check_source("<!--") {
                    self.parse_comment()
                } else {
                    self.parse_element_or_component()
                }
            }
            TokenKind::LBraceHash => self.parse_block(),
            TokenKind::LBraceAt => self.parse_special_tag(),
            TokenKind::LBrace => self.parse_expression_tag(),
            TokenKind::LBraceSlash => {
                // {/ could be a closing block tag ({/if}, {/each}, etc.) or a regex literal ({/pattern/})
                // If followed by a closing block keyword, return None to let parent block handle it
                // Otherwise, parse as expression containing regex
                if self.is_closing_block_keyword() {
                    None
                } else {
                    self.parse_expression_tag_from_lbrace_slash()
                }
            }
            TokenKind::LBraceColon => {
                // {:...} outside of a valid block context is an error
                // Valid continuation tags are {:else}, {:else if}, {:then}, {:catch}
                self.parse_invalid_block_continuation()
            }
            TokenKind::Text | TokenKind::Ident | TokenKind::Number | TokenKind::Error => {
                self.parse_text()
            }
            // Newline returns None without advancing - caller handles it
            TokenKind::Newline => None,
            _ => None,
        }
    }

    /// Checks if the next token after LBraceSlash is a block closing keyword.
    fn is_closing_block_keyword(&self) -> bool {
        if let Some(next) = self.tokens.get(self.pos + 1) {
            matches!(
                next.kind,
                TokenKind::If
                    | TokenKind::Each
                    | TokenKind::Key
                    | TokenKind::Await
                    | TokenKind::Snippet
            )
        } else {
            false
        }
    }

    /// Parses an expression tag that was tokenized as LBraceSlash (e.g., {/regex/.test(x)}).
    fn parse_expression_tag_from_lbrace_slash(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;
        let start_offset = u32::from(start) as usize;

        // LBraceSlash consumed "{/", so expression starts after "{"
        // Read expression from offset + 1 (skip the "{", include the "/")
        let (expr, expr_span) = self.read_expression_from_offset(start_offset + 1, '}');

        // Advance past all tokens until we reach or pass the closing brace
        let expr_end = expr_span.end;
        while !self.check(TokenKind::Eof) && self.current().span.end <= expr_end {
            self.advance();
        }
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::Expression(ExpressionTag {
            span: Span::new(start, end),
            expression_span: expr_span,
            expression: expr,
        }))
    }

    /// Parses an HTML comment.
    fn parse_comment(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;
        let start_offset = u32::from(start) as usize;

        // Skip past "<!--"
        let content_start = start_offset + 4;

        // Find "-->"
        let remaining = &self.source[content_start..];
        let end_pos = remaining.find("-->").unwrap_or(remaining.len());
        let content = remaining[..end_pos].to_string();

        let end_offset = content_start + end_pos + 3; // Include "-->"
        let end = TextSize::from(end_offset as u32);

        // Advance past the tokens we consumed
        while self.current().span.start < end && !self.check(TokenKind::Eof) {
            self.advance();
        }

        Some(TemplateNode::Comment(Comment {
            span: Span::new(start, end),
            data: content,
        }))
    }

    /// Parses an element or component.
    fn parse_element_or_component(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;

        // Expect `<`
        if !self.eat(TokenKind::LAngle) {
            return None;
        }

        // Get tag name - handles:
        // - Simple elements: div, span
        // - Svelte elements: svelte:head, svelte:window
        // - Components: Button, MyComponent
        // - Namespaced components: Module.Component, Tooltip.Root
        // Check for valid tag name token - could be Ident, NamespacedIdent, or
        // keyword tokens like Script/Style when used as elements in template
        let is_element_name = self.check(TokenKind::Ident)
            || self.check(TokenKind::NamespacedIdent)
            || self.check(TokenKind::Script)
            || self.check(TokenKind::Style);

        let name = if self.check(TokenKind::NamespacedIdent) {
            let mut full_name = self.current_text().to_string();
            self.advance();
            // Check for following identifier (e.g., "head" in "svelte:head")
            if self.check(TokenKind::Ident) {
                full_name.push_str(self.current_text());
                self.advance();
            }
            SmolStr::new(full_name)
        } else if is_element_name {
            let mut full_name = self.current_text().to_string();
            self.advance();

            // Handle namespaced components like Module.Component or Tooltip.Root
            // Keep consuming .Ident patterns
            while self.check(TokenKind::Dot) {
                self.advance(); // consume the dot
                if self.check(TokenKind::Ident) {
                    full_name.push('.');
                    full_name.push_str(self.current_text());
                    self.advance();
                } else {
                    // Dot not followed by identifier - stop
                    break;
                }
            }

            SmolStr::new(full_name)
        } else {
            return None;
        };

        // Check if this is a component (PascalCase) or svelte: element
        let is_component = name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);
        let is_svelte_element = name.starts_with("svelte:");

        // Parse attributes
        let attributes = self.parse_attributes();

        // Check for self-closing syntax (/>) or HTML void element
        let explicit_self_closing = self.eat(TokenKind::SlashRAngle);
        let is_void = is_void_element(&name);
        let self_closing = explicit_self_closing || is_void;

        if !explicit_self_closing {
            self.expect(TokenKind::RAngle);
        }

        // Parse children if not self-closing
        let children = if self_closing {
            Vec::new()
        } else {
            self.parse_children(&name)
        };

        // Expect closing tag if not self-closing
        if !self_closing {
            self.parse_closing_tag(&name);
        }

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        if is_svelte_element {
            let kind = match name.strip_prefix("svelte:") {
                Some("self") => SvelteElementKind::Self_,
                Some("component") => SvelteElementKind::Component,
                Some("element") => SvelteElementKind::Element,
                Some("window") => SvelteElementKind::Window,
                Some("document") => SvelteElementKind::Document,
                Some("body") => SvelteElementKind::Body,
                Some("head") => SvelteElementKind::Head,
                Some("options") => SvelteElementKind::Options,
                Some("fragment") => SvelteElementKind::Fragment,
                Some("boundary") => SvelteElementKind::Boundary,
                _ => SvelteElementKind::Element,
            };

            Some(TemplateNode::SvelteElement(SvelteElement {
                span: Span::new(start, end),
                kind,
                attributes,
                children,
            }))
        } else if is_component {
            Some(TemplateNode::Component(Component {
                span: Span::new(start, end),
                name,
                attributes,
                children,
                self_closing,
            }))
        } else {
            Some(TemplateNode::Element(Element {
                span: Span::new(start, end),
                name,
                attributes,
                children,
                self_closing,
            }))
        }
    }

    /// Parses element attributes.
    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attributes = Vec::new();

        loop {
            self.skip_whitespace();

            if self.check(TokenKind::RAngle)
                || self.check(TokenKind::SlashRAngle)
                || self.check(TokenKind::Eof)
            {
                break;
            }

            if let Some(attr) = self.parse_attribute() {
                attributes.push(attr);
            } else {
                break;
            }
        }

        attributes
    }

    /// Parses a single attribute.
    fn parse_attribute(&mut self) -> Option<Attribute> {
        let start = self.current().span.start;

        // Check for spread: {...expr}
        if self.check(TokenKind::LBrace) {
            return self.parse_spread_or_shorthand();
        }

        // Check for attach: {@attach expr}
        if self.check(TokenKind::LBraceAt) {
            return self.parse_attach_attribute();
        }

        // Check for CSS custom property attribute (--var-name="value")
        // These start with two minus signs followed by an identifier
        if self.check(TokenKind::Minus) {
            let first_minus_end = self.current().span.end;
            self.advance();

            // Check for second minus immediately after
            if self.current().span.start == first_minus_end && self.check(TokenKind::Minus) {
                let second_minus_end = self.current().span.end;
                self.advance();

                // Check for identifier immediately after
                if self.current().span.start == second_minus_end && self.check(TokenKind::Ident) {
                    let mut full_name = String::from("--");
                    full_name.push_str(self.current_text());
                    let mut prev_end = self.current().span.end;
                    self.advance();

                    // Continue reading hyphenated parts (--primary-color-dark)
                    while self.pos < self.tokens.len()
                        && self.current().span.start == prev_end
                        && self.check(TokenKind::Minus)
                    {
                        full_name.push('-');
                        prev_end = self.current().span.end;
                        self.advance();

                        if self.pos < self.tokens.len()
                            && self.current().span.start == prev_end
                            && self.check(TokenKind::Ident)
                        {
                            full_name.push_str(self.current_text());
                            prev_end = self.current().span.end;
                            self.advance();
                        }
                    }

                    // Parse value
                    let value = if self.eat(TokenKind::Eq) {
                        Some(self.parse_attribute_value())
                    } else {
                        None
                    };

                    let end = self
                        .tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| t.span.end)
                        .unwrap_or(start);
                    return Some(Attribute::CssCustomProperty {
                        name: SmolStr::new(&full_name[2..]), // Strip the leading --
                        value,
                        span: Span::new(start, end),
                    });
                }
            }

            // Not a valid CSS custom property, backtrack is not possible
            // so we return None and let the caller handle it
            return None;
        }

        // Check for identifier (normal attribute or directive)
        // Also accept keyword tokens that can be valid HTML attribute names
        // This includes block keywords (if, else, each, etc.) and tag keywords (html, const, etc.)
        let is_keyword_as_attr = matches!(
            self.current_kind(),
            TokenKind::Style
                | TokenKind::Script
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::Each
                | TokenKind::Key
                | TokenKind::Await
                | TokenKind::Then
                | TokenKind::Catch
                | TokenKind::As
                | TokenKind::Snippet
                | TokenKind::Html
                | TokenKind::Const
                | TokenKind::Debug
                | TokenKind::Render
        );

        if !self.check(TokenKind::Ident)
            && !self.check(TokenKind::NamespacedIdent)
            && !is_keyword_as_attr
        {
            return None;
        }

        let mut full_name = self.current_text().to_string();
        let is_namespaced = self.check(TokenKind::NamespacedIdent);
        let mut prev_end = self.current().span.end;
        self.advance();

        // Handle dot notation in attribute names (e.g., rotation.x={...} used by threlte)
        // Only continue if tokens are adjacent (no whitespace between)
        while self.pos < self.tokens.len()
            && self.current().span.start == prev_end
            && self.check(TokenKind::Dot)
        {
            full_name.push_str(self.current_text());
            prev_end = self.current().span.end;
            self.advance();

            // After dot, expect an identifier
            if self.pos < self.tokens.len()
                && self.current().span.start == prev_end
                && self.check(TokenKind::Ident)
            {
                full_name.push_str(self.current_text());
                prev_end = self.current().span.end;
                self.advance();
            }
        }

        // For namespaced identifiers (directives), continue reading tokens
        // to build the full name including modifiers: on:click|preventDefault|stopPropagation
        // Also handle Tailwind-style class names:
        // - class:hover:underline (nested pseudo-classes)
        // - class:!items-start (important modifier)
        // - class:sm:grid-cols-[auto,1fr,1fr] (arbitrary values with brackets and commas)
        if is_namespaced {
            // Read the argument name and all its modifiers/content
            // Only continue if tokens are adjacent (no whitespace between)
            // Include keywords that can be valid directive argument names (e.g., bind:key, on:if)
            while self.pos < self.tokens.len()
                && self.current().span.start == prev_end
                && (self.check(TokenKind::Ident)
                    || self.check(TokenKind::Pipe)
                    || self.check(TokenKind::Colon)
                    || self.check(TokenKind::NamespacedIdent)
                    || self.check(TokenKind::Dot) // For member access: use:form.enhance
                    || self.check(TokenKind::Text) // For !, [, ], etc.
                    || self.check(TokenKind::Comma) // For Tailwind bracket values: [auto,1fr]
                    || self.check(TokenKind::Number) // For sizes: [100px], grid-cols-2
                    || self.check(TokenKind::Minus) // For CSS custom properties: style:--my-var
                    // Keywords that can be valid directive argument names
                    || self.check(TokenKind::Key) // bind:key
                    || self.check(TokenKind::If) // on:if (custom events)
                    || self.check(TokenKind::Else)
                    || self.check(TokenKind::Each)
                    || self.check(TokenKind::Await)
                    || self.check(TokenKind::Then)
                    || self.check(TokenKind::Catch)
                    || self.check(TokenKind::As)
                    || self.check(TokenKind::Snippet)
                    || self.check(TokenKind::Html)
                    || self.check(TokenKind::Const)
                    || self.check(TokenKind::Debug)
                    || self.check(TokenKind::Render)
                    || self.check(TokenKind::Style)
                    || self.check(TokenKind::Script))
            {
                full_name.push_str(self.current_text());
                prev_end = self.current().span.end;
                self.advance();
            }
        }

        // Check for directive (name:arg)
        // Only recognize known Svelte directive prefixes; unknown prefixes like xmlns: or xlink:
        // should be treated as normal attributes (valid in SVG/XML contexts)
        let directive_info = full_name.find(':').and_then(|colon_pos| {
            let directive_name = &full_name[..colon_pos];
            let arg_name = &full_name[colon_pos + 1..];
            let kind = match directive_name {
                "on" => Some(DirectiveKind::On),
                "bind" => Some(DirectiveKind::Bind),
                "class" => Some(DirectiveKind::Class),
                "style" => Some(DirectiveKind::StyleDirective),
                "use" => Some(DirectiveKind::Use),
                "transition" => Some(DirectiveKind::Transition),
                "in" => Some(DirectiveKind::In),
                "out" => Some(DirectiveKind::Out),
                "animate" => Some(DirectiveKind::Animate),
                "let" => Some(DirectiveKind::Let),
                _ => None,
            }?;
            Some((kind, directive_name.to_string(), arg_name.to_string()))
        });

        if let Some((kind, directive_name, arg_name)) = directive_info {
            // Parse modifiers (|modifier)
            // Split on '|' - first part is directive name, rest are modifiers
            let parts: Vec<&str> = arg_name.split('|').collect();
            let remaining = parts.first().unwrap_or(&"").to_string();
            let modifiers: Vec<SmolStr> = parts[1..]
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| SmolStr::new(*s))
                .collect();

            // Error if directive name is empty (e.g., style:, on:, bind:)
            if remaining.is_empty() {
                self.errors.push(ParseError::new(
                    ParseErrorKind::InvalidDirective {
                        message: format!("`{}:` name cannot be empty", directive_name),
                    },
                    Span::new(start, self.current().span.end),
                ));
            }

            // Parse value - directives can have expression or quoted string values
            let expression = if self.eat(TokenKind::Eq) {
                if self.check(TokenKind::LBrace) {
                    self.advance();
                    let (expr, expr_span) = self.read_expression_until('}');
                    self.eat(TokenKind::RBrace);
                    let end = self
                        .tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| t.span.end)
                        .unwrap_or(start);
                    Some(ExpressionValue {
                        span: Span::new(start, end),
                        expression_span: expr_span,
                        expression: expr,
                        is_quoted: false,
                    })
                } else if self.check(TokenKind::DoubleQuote) || self.check(TokenKind::SingleQuote) {
                    // Handle quoted string values like style:color="red"
                    let quote = if self.check(TokenKind::DoubleQuote) {
                        "\""
                    } else {
                        "'"
                    };
                    let quote_start = self.current().span.start;
                    self.advance(); // consume opening quote
                    let (text, text_span) = self.read_until(&[quote]);
                    if self.check(TokenKind::DoubleQuote) || self.check(TokenKind::SingleQuote) {
                        self.advance(); // consume closing quote
                    }
                    let end = self
                        .tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| t.span.end)
                        .unwrap_or(quote_start);
                    Some(ExpressionValue {
                        span: Span::new(quote_start, end),
                        expression_span: text_span,
                        expression: text,
                        is_quoted: true,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map(|t| t.span.end)
                .unwrap_or(start);

            return Some(Attribute::Directive(Directive {
                span: Span::new(start, end),
                kind,
                name: SmolStr::new(remaining),
                modifiers,
                expression,
            }));
        }

        // Normal attribute
        let name = SmolStr::new(&full_name);
        let value = if self.eat(TokenKind::Eq) {
            self.parse_attribute_value()
        } else {
            AttributeValue::True
        };

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(Attribute::Normal(NormalAttribute {
            span: Span::new(start, end),
            name,
            value,
        }))
    }

    /// Parses an attribute value.
    fn parse_attribute_value(&mut self) -> AttributeValue {
        if self.check(TokenKind::DoubleQuote) {
            self.advance();
            self.parse_quoted_attribute_value('"')
        } else if self.check(TokenKind::SingleQuote) {
            self.advance();
            self.parse_quoted_attribute_value('\'')
        } else if self.check(TokenKind::LBrace) {
            let start = self.current().span.start;
            self.advance();
            let (expr, expr_span) = self.read_expression_until('}');
            self.eat(TokenKind::RBrace);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map(|t| t.span.end)
                .unwrap_or(start);
            AttributeValue::Expression(ExpressionValue {
                span: Span::new(start, end),
                expression_span: expr_span,
                expression: expr,
                is_quoted: false,
            })
        } else if self.check(TokenKind::LBraceSlash) {
            // Handle case where lexer tokenized {/* as LBraceSlash + * (comment in expression)
            // or {// as LBraceSlash + / (single-line comment in expression)
            // This happens because LBraceSlash has higher priority than LBrace
            let start = self.current().span.start;
            // Read from source directly, skipping the "{" that was consumed as part of LBraceSlash
            // The source starting at `start` contains the full `{...}` expression
            let start_offset = u32::from(start) as usize;
            // Skip the opening brace and read the expression
            let (expr, expr_span) = self.read_expression_from_offset(start_offset + 1, '}');
            // Advance past all tokens until we reach or pass the closing brace
            let expr_end = expr_span.end;
            while !self.check(TokenKind::Eof) && self.current().span.end <= expr_end {
                self.advance();
            }
            self.eat(TokenKind::RBrace);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map(|t| t.span.end)
                .unwrap_or(start);
            AttributeValue::Expression(ExpressionValue {
                span: Span::new(start, end),
                expression_span: expr_span,
                expression: expr,
                is_quoted: false,
            })
        } else {
            AttributeValue::True
        }
    }

    /// Parses a quoted attribute value that may contain expressions.
    fn parse_quoted_attribute_value(&mut self, quote: char) -> AttributeValue {
        let start = self.current().span.start;
        let start_offset = u32::from(start) as usize;
        let quote_token = if quote == '"' {
            TokenKind::DoubleQuote
        } else {
            TokenKind::SingleQuote
        };

        // Read the content between quotes
        let quote_str = if quote == '"' { "\"" } else { "'" };
        let (full_text, full_span) = self.read_until(&[quote_str]);
        self.eat(quote_token);

        // Check if it contains any expressions
        if !full_text.contains('{') {
            // Simple text value
            return AttributeValue::Text(TextValue {
                span: full_span,
                value: full_text,
            });
        }

        // Parse concatenated parts
        let mut parts = Vec::new();
        let mut pos = 0;
        let bytes = full_text.as_bytes();

        while pos < full_text.len() {
            if bytes[pos] == b'{' {
                // Find matching closing brace
                let expr_start = pos + 1;
                let mut depth: i32 = 1;
                let mut end = expr_start;
                let mut in_string = false;
                let mut string_char = ' ';

                while end < full_text.len() && depth > 0 {
                    let c = full_text[end..].chars().next().unwrap();
                    if in_string {
                        let preceding_bs = (0..end).rev().take_while(|&j| bytes.get(j) == Some(&b'\\')).count();
                        let is_escaped = preceding_bs % 2 == 1;
                        if c == string_char && !is_escaped {
                            in_string = false;
                        }
                    } else {
                        match c {
                            '"' | '\'' | '`' => {
                                in_string = true;
                                string_char = c;
                            }
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                    }
                    end += c.len_utf8();
                }

                // The expression content (without braces)
                let expr_end = if depth == 0 { end - 1 } else { end };
                let expression = full_text[expr_start..expr_end].to_string();
                let expr_span = Span::new(
                    TextSize::from((start_offset + expr_start) as u32),
                    TextSize::from((start_offset + expr_end) as u32),
                );
                let full_expr_span = Span::new(
                    TextSize::from((start_offset + pos) as u32),
                    TextSize::from((start_offset + end) as u32),
                );

                parts.push(AttributeValuePart::Expression(ExpressionValue {
                    span: full_expr_span,
                    expression_span: expr_span,
                    expression,
                    is_quoted: false,
                }));

                pos = end;
            } else {
                // Find the next '{' or end of string
                let text_start = pos;
                while pos < full_text.len() && bytes[pos] != b'{' {
                    pos += 1;
                }

                if pos > text_start {
                    let text = full_text[text_start..pos].to_string();
                    let text_span = Span::new(
                        TextSize::from((start_offset + text_start) as u32),
                        TextSize::from((start_offset + pos) as u32),
                    );
                    parts.push(AttributeValuePart::Text(TextValue {
                        span: text_span,
                        value: text,
                    }));
                }
            }
        }

        // If only one part, simplify to the appropriate type
        if parts.len() == 1 {
            match parts.remove(0) {
                AttributeValuePart::Text(text) => return AttributeValue::Text(text),
                AttributeValuePart::Expression(expr) => return AttributeValue::Expression(expr),
            }
        }

        AttributeValue::Concat(parts)
    }

    /// Parses a spread attribute or shorthand.
    fn parse_spread_or_shorthand(&mut self) -> Option<Attribute> {
        let start = self.current().span.start;

        if !self.eat(TokenKind::LBrace) {
            return None;
        }

        let (expr, expr_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        if let Some(spread_expr) = expr.strip_prefix("...") {
            Some(Attribute::Spread(SpreadAttribute {
                span: Span::new(start, end),
                expression_span: expr_span,
                expression: spread_expr.to_string(),
            }))
        } else {
            Some(Attribute::Shorthand(ShorthandAttribute {
                span: Span::new(start, end),
                name: SmolStr::new(expr.trim()),
            }))
        }
    }

    /// Parses an attach attribute `{@attach expr}`.
    fn parse_attach_attribute(&mut self) -> Option<Attribute> {
        let start = self.current().span.start;

        if !self.eat(TokenKind::LBraceAt) {
            return None;
        }

        // Check for 'attach' keyword (will be an Ident token)
        let tag_text = self.current_text();
        if tag_text != "attach" {
            self.error(ParseErrorKind::InvalidBlockSyntax {
                message: format!("expected 'attach' after '{{@', found '{}'", tag_text),
            });
            return None;
        }
        self.advance();

        self.skip_whitespace();

        // Read the expression until closing brace
        let (expr, expr_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(Attribute::Attach(AttachAttribute {
            span: Span::new(start, end),
            expression_span: expr_span,
            expression: expr,
        }))
    }

    /// Parses children until a closing tag.
    fn parse_children(&mut self, parent_tag: &str) -> Vec<TemplateNode> {
        let mut children = Vec::new();
        let close_tag = format!("</{}", parent_tag);

        while !self.check(TokenKind::Eof) {
            // Check for closing tag
            if self.check(TokenKind::LAngleSlash) {
                break;
            }

            // Check in source for closing tag
            let current_offset = u32::from(self.current().span.start) as usize;
            if self.source[current_offset..].starts_with(&close_tag) {
                break;
            }

            if let Some(node) = self.parse_template_node() {
                children.push(node);
            } else if !self.check(TokenKind::Eof) && !self.check(TokenKind::LAngleSlash) {
                self.advance();
            } else {
                break;
            }
        }

        children
    }

    /// Parses a closing tag.
    fn parse_closing_tag(&mut self, expected_name: &str) {
        if !self.eat(TokenKind::LAngleSlash) {
            self.error(ParseErrorKind::UnclosedTag {
                tag_name: expected_name.to_string(),
            });
            return;
        }

        // Parse tag name - handle:
        // - Svelte elements: svelte:head
        // - Namespaced components: Module.Component, Tooltip.Root
        // - Keyword elements: script, style (when used in templates)
        let is_element_name = self.check(TokenKind::Ident)
            || self.check(TokenKind::Script)
            || self.check(TokenKind::Style);

        let found_name = if self.check(TokenKind::NamespacedIdent) {
            let mut full_name = self.current_text().to_string();
            self.advance();
            // Check for following identifier (e.g., "head" in "svelte:head")
            if self.check(TokenKind::Ident) {
                full_name.push_str(self.current_text());
                self.advance();
            }
            full_name
        } else if is_element_name {
            let mut full_name = self.current_text().to_string();
            self.advance();

            // Handle namespaced components like Module.Component
            while self.check(TokenKind::Dot) {
                self.advance(); // consume the dot
                if self.check(TokenKind::Ident) {
                    full_name.push('.');
                    full_name.push_str(self.current_text());
                    self.advance();
                } else {
                    break;
                }
            }

            full_name
        } else {
            String::new()
        };

        if found_name != expected_name {
            self.error(ParseErrorKind::MismatchedClosingTag {
                expected: expected_name.to_string(),
                found: found_name,
            });
        }

        self.eat(TokenKind::RAngle);
    }

    /// Parses a block ({#if}, {#each}, etc.).
    fn parse_block(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;

        if !self.eat(TokenKind::LBraceHash) {
            return None;
        }

        let block_type = self.current_text().to_string();
        self.advance();

        match block_type.as_str() {
            "if" => self.parse_if_block(start),
            "each" => self.parse_each_block(start),
            "await" => self.parse_await_block(start),
            "key" => self.parse_key_block(start),
            "snippet" => self.parse_snippet_block(start),
            _ => {
                self.error(ParseErrorKind::InvalidBlockSyntax {
                    message: format!("unknown block type: {}", block_type),
                });
                None
            }
        }
    }

    /// Parses an if block.
    fn parse_if_block(&mut self, start: TextSize) -> Option<TemplateNode> {
        // Parse condition
        let (condition, condition_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        // Parse consequent
        let consequent = self.parse_block_children(&["{:else", "{/if"]);

        // Check for else
        let alternate = if self.check_source("{:else if") {
            self.eat(TokenKind::LBraceColon);
            self.eat(TokenKind::Else);
            self.eat(TokenKind::If); // Eat the 'if' keyword before parsing condition
                                     // Continue as if block
            let else_if_start = self.current().span.start;
            self.parse_if_block(else_if_start).map(|node| {
                if let TemplateNode::IfBlock(block) = node {
                    ElseBranch::ElseIf(Box::new(block))
                } else {
                    unreachable!()
                }
            })
        } else if self.check_source("{:else}") {
            self.eat(TokenKind::LBraceColon); // {:
            self.eat(TokenKind::Else); // else
            self.eat(TokenKind::RBrace); // }
            let else_body = self.parse_block_children(&["{/if"]);
            Some(ElseBranch::Else(else_body))
        } else {
            None
        };

        // Expect {/if}
        self.eat(TokenKind::LBraceSlash);
        self.eat(TokenKind::If);
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::IfBlock(IfBlock {
            span: Span::new(start, end),
            condition_span,
            condition,
            consequent,
            alternate,
        }))
    }

    /// Parses an each block.
    fn parse_each_block(&mut self, start: TextSize) -> Option<TemplateNode> {
        // Parse expression and pattern: {#each items as item, index (key)}
        let expr_start = self.current().span.start;
        let (full_expr, full_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        // Use brace-aware parsing to find " as " - we need to find the LAST " as "
        // that separates the expression from the pattern, since the expression itself
        // may contain TypeScript " as " casts.
        // Note: " as" may be followed by any whitespace (space, newline, tab).
        let (expression, expression_span, rest, rest_offset) = {
            // Find all occurrences of " as" followed by whitespace and use the last one
            let mut last_as_match: Option<(usize, usize)> = None;
            let mut search_start = 0;
            while let Some((pos, match_len)) =
                Self::find_as_keyword_in_expr(&full_expr[search_start..])
            {
                let absolute_pos = search_start + pos;
                last_as_match = Some((absolute_pos, match_len));
                search_start = absolute_pos + match_len;
            }

            if let Some((as_pos, match_len)) = last_as_match {
                let expr = full_expr[..as_pos].trim().to_string();
                let expr_span = Span::new(
                    expr_start,
                    TextSize::from(u32::from(expr_start) + as_pos as u32),
                );
                let rest_start = as_pos + match_len;
                (expr, expr_span, &full_expr[rest_start..], rest_start)
            } else {
                (full_expr.trim().to_string(), full_span, "", 0)
            }
        };

        let rest = rest.trim();

        // Parse context and index using brace-aware parsing
        let (context, context_span, index, key) = if let Some(paren_pos) =
            Self::find_char_in_expr(rest, '(')
        {
            // Has key expression: item, index (key)
            let before_paren = rest[..paren_pos].trim();

            // Find the matching closing paren
            let key_start = paren_pos + 1;
            let key_end = rest.len().saturating_sub(1); // Remove trailing )
            let key_expr = if key_start < key_end {
                rest[key_start..key_end].trim().to_string()
            } else {
                String::new()
            };

            // Calculate key span
            let key_span_start =
                TextSize::from(u32::from(expr_start) + rest_offset as u32 + paren_pos as u32 + 1);
            let key_span_end =
                TextSize::from(u32::from(expr_start) + rest_offset as u32 + key_end as u32);

            // Parse context and index from before_paren
            let (ctx, ctx_span, idx) = if let Some(comma_pos) =
                Self::find_char_in_expr(before_paren, ',')
            {
                let ctx = before_paren[..comma_pos].trim().to_string();
                let ctx_span_start = TextSize::from(u32::from(expr_start) + rest_offset as u32);
                let ctx_span_end =
                    TextSize::from(u32::from(expr_start) + rest_offset as u32 + comma_pos as u32);
                let idx = before_paren[comma_pos + 1..].trim();
                (
                    ctx,
                    Span::new(ctx_span_start, ctx_span_end),
                    Some(SmolStr::new(idx)),
                )
            } else {
                let ctx = before_paren.to_string();
                let ctx_span_start = TextSize::from(u32::from(expr_start) + rest_offset as u32);
                let ctx_span_end = TextSize::from(
                    u32::from(expr_start) + rest_offset as u32 + before_paren.len() as u32,
                );
                (ctx, Span::new(ctx_span_start, ctx_span_end), None)
            };

            (
                ctx,
                ctx_span,
                idx,
                Some(EachKey {
                    span: Span::new(key_span_start, key_span_end),
                    expression: key_expr,
                }),
            )
        } else if let Some(comma_pos) = Self::find_char_in_expr(rest, ',') {
            // Has index but no key: item, index
            let ctx = rest[..comma_pos].trim().to_string();
            let ctx_span_start = TextSize::from(u32::from(expr_start) + rest_offset as u32);
            let ctx_span_end =
                TextSize::from(u32::from(expr_start) + rest_offset as u32 + comma_pos as u32);
            let idx = rest[comma_pos + 1..].trim();
            (
                ctx,
                Span::new(ctx_span_start, ctx_span_end),
                Some(SmolStr::new(idx)),
                None,
            )
        } else {
            // Just context: item
            let ctx = rest.to_string();
            let ctx_span_start = TextSize::from(u32::from(expr_start) + rest_offset as u32);
            let ctx_span_end =
                TextSize::from(u32::from(expr_start) + rest_offset as u32 + rest.len() as u32);
            (ctx, Span::new(ctx_span_start, ctx_span_end), None, None)
        };

        // Parse body
        let body = self.parse_block_children(&["{:else", "{/each"]);

        // Check for else
        let fallback = if self.check_source("{:else}") {
            self.eat(TokenKind::LBraceColon); // {:
            self.eat(TokenKind::Else); // else
            self.eat(TokenKind::RBrace); // }
            Some(self.parse_block_children(&["{/each"]))
        } else {
            None
        };

        // Expect {/each}
        self.eat(TokenKind::LBraceSlash);
        self.eat(TokenKind::Each);
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::EachBlock(EachBlock {
            span: Span::new(start, end),
            expression_span,
            expression,
            context,
            context_span,
            index,
            key,
            body,
            fallback,
        }))
    }

    /// Parses an await block.
    fn parse_await_block(&mut self, start: TextSize) -> Option<TemplateNode> {
        let expr_start = self.current().span.start;
        let (full_expr, full_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        // Check for shorthand: {#await promise then value}
        // Use brace-aware parsing to find " then "
        let (expression, expression_span, immediate_then) =
            if let Some(then_pos) = Self::find_keyword_in_expr(&full_expr, " then ") {
                let expr = full_expr[..then_pos].trim().to_string();
                let expr_span = Span::new(
                    expr_start,
                    TextSize::from(u32::from(expr_start) + then_pos as u32),
                );
                let then_value = full_expr[then_pos + 6..].trim().to_string(); // " then " is 6 chars
                (expr, expr_span, Some(then_value))
            } else {
                (full_expr.trim().to_string(), full_span, None)
            };

        // Parse pending content or body
        let (pending, then, catch) = if let Some(then_value) = immediate_then {
            let then_start = self.current().span.start;
            let body = self.parse_block_children(&["{:catch", "{/await"]);
            let then_end = self.current().span.start;

            let catch_block = if self.check_source("{:catch") {
                let catch_start = self.current().span.start;
                self.eat(TokenKind::LBraceColon); // {:
                self.eat(TokenKind::Catch); // catch
                let (error_name, _) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);
                let catch_body = self.parse_block_children(&["{/await"]);
                let catch_end = self.current().span.start;
                Some(AwaitCatch {
                    span: Span::new(catch_start, catch_end),
                    error: if error_name.trim().is_empty() {
                        None
                    } else {
                        Some(SmolStr::new(error_name.trim()))
                    },
                    body: catch_body,
                })
            } else {
                None
            };

            (
                None,
                Some(AwaitThen {
                    span: Span::new(then_start, then_end),
                    value: if then_value.is_empty() {
                        None
                    } else {
                        Some(SmolStr::new(then_value))
                    },
                    body,
                }),
                catch_block,
            )
        } else {
            let pending = self.parse_block_children(&["{:then", "{:catch", "{/await"]);

            let then_block = if self.check_source("{:then") {
                let then_start = self.current().span.start;
                self.eat(TokenKind::LBraceColon); // {:
                self.eat(TokenKind::Then); // then
                let (value_name, _) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);
                let then_body = self.parse_block_children(&["{:catch", "{/await"]);
                let then_end = self.current().span.start;
                Some(AwaitThen {
                    span: Span::new(then_start, then_end),
                    value: if value_name.trim().is_empty() {
                        None
                    } else {
                        Some(SmolStr::new(value_name.trim()))
                    },
                    body: then_body,
                })
            } else {
                None
            };

            let catch_block = if self.check_source("{:catch") {
                let catch_start = self.current().span.start;
                self.eat(TokenKind::LBraceColon); // {:
                self.eat(TokenKind::Catch); // catch
                let (error_name, _) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);
                let catch_body = self.parse_block_children(&["{/await"]);
                let catch_end = self.current().span.start;
                Some(AwaitCatch {
                    span: Span::new(catch_start, catch_end),
                    error: if error_name.trim().is_empty() {
                        None
                    } else {
                        Some(SmolStr::new(error_name.trim()))
                    },
                    body: catch_body,
                })
            } else {
                None
            };

            (Some(pending), then_block, catch_block)
        };

        // Expect {/await}
        self.eat(TokenKind::LBraceSlash);
        self.eat(TokenKind::Await);
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::AwaitBlock(AwaitBlock {
            span: Span::new(start, end),
            expression_span,
            expression,
            pending,
            then,
            catch,
        }))
    }

    /// Parses a key block.
    fn parse_key_block(&mut self, start: TextSize) -> Option<TemplateNode> {
        let (expression, expression_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        let body = self.parse_block_children(&["{/key"]);

        // Expect {/key}
        self.eat(TokenKind::LBraceSlash);
        self.eat(TokenKind::Key);
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::KeyBlock(KeyBlock {
            span: Span::new(start, end),
            expression_span,
            expression: expression.trim().to_string(),
            body,
        }))
    }

    /// Parses a snippet block.
    fn parse_snippet_block(&mut self, start: TextSize) -> Option<TemplateNode> {
        let sig_start = self.current().span.start;
        let (full_signature, _) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        // Parse name and parameters: name(params)
        // Use brace-aware parsing for the parenthesis
        let (name, parameters, parameters_span) =
            if let Some(paren_pos) = Self::find_char_in_expr(&full_signature, '(') {
                let name = full_signature[..paren_pos].trim();

                // Find matching closing paren at depth 0
                let params_start = paren_pos + 1;
                let params_end = full_signature.len().saturating_sub(1); // Remove trailing )
                let params = if params_start < params_end {
                    full_signature[params_start..params_end].to_string()
                } else {
                    String::new()
                };

                // Calculate parameters span
                let params_span_start = TextSize::from(u32::from(sig_start) + paren_pos as u32 + 1);
                let params_span_end = TextSize::from(u32::from(sig_start) + params_end as u32);

                (
                    SmolStr::new(name),
                    params,
                    Span::new(params_span_start, params_span_end),
                )
            } else {
                (
                    SmolStr::new(full_signature.trim()),
                    String::new(),
                    Span::empty(sig_start),
                )
            };

        let body = self.parse_block_children(&["{/snippet"]);

        // Expect {/snippet}
        self.eat(TokenKind::LBraceSlash);
        self.eat(TokenKind::Snippet);
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::SnippetBlock(SnippetBlock {
            span: Span::new(start, end),
            name,
            parameters_span,
            parameters,
            body,
        }))
    }

    /// Parses children until one of the given delimiters.
    fn parse_block_children(&mut self, delimiters: &[&str]) -> Fragment {
        let start = self.current().span.start;
        let mut nodes = Vec::new();

        while !self.check(TokenKind::Eof) {
            // Check for delimiters
            let current_offset = u32::from(self.current().span.start) as usize;
            let remaining = &self.source[current_offset..];

            let at_delimiter = delimiters.iter().any(|d| remaining.starts_with(d));
            if at_delimiter {
                break;
            }

            if let Some(node) = self.parse_template_node() {
                nodes.push(node);
            } else if !self.check(TokenKind::Eof) {
                self.advance();
            } else {
                break;
            }
        }

        let end = nodes.last().map(|n| n.span().end).unwrap_or(start);

        Fragment {
            nodes,
            span: Span::new(start, end),
        }
    }

    /// Checks if the source at the current position starts with the given string.
    fn check_source(&self, s: &str) -> bool {
        let offset = u32::from(self.current().span.start) as usize;
        self.source[offset..].starts_with(s)
    }

    /// Handles an invalid block continuation tag like {:els} (typo for {:else}).
    /// Emits an appropriate error and skips past the invalid tag.
    fn parse_invalid_block_continuation(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;

        // Skip {:
        self.advance();

        // Read the invalid keyword
        let keyword = if self.check(TokenKind::Ident) || self.check(TokenKind::Text) {
            self.current_text().to_string()
        } else {
            String::new()
        };

        // Find the end of the tag (look for })
        while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
            self.advance();
        }

        let end = if self.check(TokenKind::RBrace) {
            let end = self.current().span.end;
            self.advance();
            end
        } else {
            self.current().span.start
        };

        // Emit appropriate error message
        let message = if keyword.is_empty() {
            "expected block continuation keyword (else, then, catch)".to_string()
        } else {
            format!(
                "'{keyword}' is not a valid block continuation. Expected {{:else}}, {{:else if}}, {{:then}}, or {{:catch}}"
            )
        };

        self.errors.push(ParseError::new(
            ParseErrorKind::InvalidBlockSyntax { message },
            Span::new(start, end),
        ));

        None
    }

    /// Parses a special tag ({@html}, {@const}, {@debug}, {@render}).
    fn parse_special_tag(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;

        if !self.eat(TokenKind::LBraceAt) {
            return None;
        }

        let tag_type = self.current_text().to_string();
        self.advance();

        match tag_type.as_str() {
            "html" => {
                let (expression, expression_span) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);

                let end = self
                    .tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| t.span.end)
                    .unwrap_or(start);

                Some(TemplateNode::HtmlTag(HtmlTag {
                    span: Span::new(start, end),
                    expression_span,
                    expression: expression.trim().to_string(),
                }))
            }
            "const" => {
                let (declaration, declaration_span) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);

                let end = self
                    .tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| t.span.end)
                    .unwrap_or(start);

                Some(TemplateNode::ConstTag(ConstTag {
                    span: Span::new(start, end),
                    declaration_span,
                    declaration: declaration.trim().to_string(),
                }))
            }
            "debug" => {
                let (identifiers_str, _) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);

                let identifiers: Vec<SmolStr> = identifiers_str
                    .split(',')
                    .map(|s| SmolStr::new(s.trim()))
                    .filter(|s| !s.is_empty())
                    .collect();

                let end = self
                    .tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| t.span.end)
                    .unwrap_or(start);

                Some(TemplateNode::DebugTag(DebugTag {
                    span: Span::new(start, end),
                    identifiers,
                }))
            }
            "render" => {
                let (expression, expression_span) = self.read_expression_until('}');
                self.eat(TokenKind::RBrace);

                let end = self
                    .tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| t.span.end)
                    .unwrap_or(start);

                Some(TemplateNode::RenderTag(RenderTag {
                    span: Span::new(start, end),
                    expression_span,
                    expression: expression.trim().to_string(),
                }))
            }
            _ => {
                self.error(ParseErrorKind::InvalidBlockSyntax {
                    message: format!("unknown special tag: @{}", tag_type),
                });
                None
            }
        }
    }

    /// Parses an expression tag {expr}.
    fn parse_expression_tag(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;

        if !self.eat(TokenKind::LBrace) {
            return None;
        }

        let (expression, expression_span) = self.read_expression_until('}');
        self.eat(TokenKind::RBrace);

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::Expression(ExpressionTag {
            span: Span::new(start, end),
            expression_span,
            expression: expression.trim().to_string(),
        }))
    }

    /// Parses text content.
    fn parse_text(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;
        let mut text = String::new();

        while matches!(
            self.current_kind(),
            TokenKind::Text
                | TokenKind::Ident
                | TokenKind::Number
                | TokenKind::Error
                | TokenKind::Newline
                | TokenKind::Dot
        ) {
            text.push_str(self.current_text());
            self.advance();

            // Stop at delimiters
            if self.check(TokenKind::LAngle) || self.check(TokenKind::LBrace) {
                break;
            }
        }

        if text.is_empty() {
            return None;
        }

        let is_whitespace = text.chars().all(|c| c.is_whitespace());
        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);

        Some(TemplateNode::Text(Text {
            span: Span::new(start, end),
            data: text,
            is_whitespace,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_element() {
        let result = Parser::new("<div>hello</div>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());
        assert_eq!(result.document.fragment.nodes.len(), 1);

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.name.as_str(), "div");
            assert_eq!(el.children.len(), 1);
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_parse_unicode_text() {
        let result = Parser::new("<div>—</div>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());
        assert_eq!(result.document.fragment.nodes.len(), 1);

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.name.as_str(), "div");
            assert_eq!(el.children.len(), 1);

            if let TemplateNode::Text(text) = &el.children[0] {
                assert_eq!(text.data, "—");
                assert!(!text.is_whitespace);
            } else {
                panic!("Expected Text");
            }
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_parse_self_closing() {
        let result = Parser::new("<br/>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert!(el.self_closing);
        }
    }

    #[test]
    fn test_parse_component() {
        let result = Parser::new("<Button>Click</Button>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.name.as_str(), "Button");
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_parse_if_block() {
        let result = Parser::new("{#if true}yes{/if}", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.condition.trim(), "true");
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_parse_each_block() {
        let result = Parser::new(
            "{#each items as item}{item}{/each}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::EachBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.expression.trim(), "items");
            assert_eq!(block.context.trim(), "item");
        } else {
            panic!("Expected EachBlock");
        }
    }

    #[test]
    fn test_parse_script() {
        let result = Parser::new("<script>let x = 1;</script>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());
        assert!(result.document.instance_script.is_some());

        let script = result.document.instance_script.unwrap();
        assert!(script.content.contains("let x = 1"));
    }

    #[test]
    fn test_parse_expression() {
        let result = Parser::new("{value}", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "value");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_parse_nested_braces_in_expression() {
        // Test that expressions with nested braces are parsed correctly
        let result =
            Parser::new("{items.map(x => { return x; })}", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "items.map(x => { return x; })");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_parse_object_literal_in_expression() {
        let result = Parser::new("{obj = { a: 1, b: 2 }}", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "obj = { a: 1, b: 2 }");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_parse_directive_with_modifiers() {
        let result = Parser::new(
            r#"<button on:click|preventDefault|stopPropagation={handler}>Click</button>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.attributes.len(), 1);
            if let Attribute::Directive(dir) = &el.attributes[0] {
                assert_eq!(dir.kind, DirectiveKind::On);
                assert_eq!(dir.name.as_str(), "click");
                assert_eq!(dir.modifiers.len(), 2);
                assert_eq!(dir.modifiers[0].as_str(), "preventDefault");
                assert_eq!(dir.modifiers[1].as_str(), "stopPropagation");
            } else {
                panic!("Expected Directive");
            }
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_parse_each_with_index_and_key() {
        let result = Parser::new(
            "{#each items as item, index (item.id)}{item}{/each}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::EachBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.expression.trim(), "items");
            assert_eq!(block.context.trim(), "item");
            assert_eq!(block.index, Some(SmolStr::new("index")));
            assert!(block.key.is_some());
            let key = block.key.as_ref().unwrap();
            assert_eq!(key.expression.trim(), "item.id");
        } else {
            panic!("Expected EachBlock");
        }
    }

    #[test]
    fn test_parse_if_with_complex_condition() {
        let result = Parser::new(
            "{#if obj.method({ key: 'value' })}yes{/if}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.condition.trim(), "obj.method({ key: 'value' })");
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_parse_render_with_nested_call() {
        let result = Parser::new(
            "{@render snippet({ data: { nested: true } })}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::RenderTag(tag) = &result.document.fragment.nodes[0] {
            assert_eq!(tag.expression.trim(), "snippet({ data: { nested: true } })");
        } else {
            panic!("Expected RenderTag");
        }
    }

    #[test]
    fn test_parse_comment() {
        let result = Parser::new(
            "<!-- This is a comment --><div>content</div>",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Comment(comment) = &result.document.fragment.nodes[0] {
            assert_eq!(comment.data.trim(), "This is a comment");
        } else {
            panic!(
                "Expected Comment, got {:?}",
                result.document.fragment.nodes[0]
            );
        }
    }

    #[test]
    fn test_parse_comment_multiline() {
        let result = Parser::new(
            "<!--\n  Multi-line\n  comment\n--><div/>",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Comment(comment) = &result.document.fragment.nodes[0] {
            assert!(comment.data.contains("Multi-line"));
            assert!(comment.data.contains("comment"));
        } else {
            panic!("Expected Comment");
        }
    }

    #[test]
    fn test_parse_concatenated_attribute() {
        let result = Parser::new(
            "<div class=\"static-{dynamic}-end\"/>",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.attributes.len(), 1);
            if let Attribute::Normal(attr) = &el.attributes[0] {
                assert_eq!(attr.name.as_str(), "class");
                if let AttributeValue::Concat(parts) = &attr.value {
                    assert_eq!(parts.len(), 3);
                    if let AttributeValuePart::Text(text) = &parts[0] {
                        assert_eq!(text.value, "static-");
                    } else {
                        panic!("Expected Text");
                    }
                    if let AttributeValuePart::Expression(expr) = &parts[1] {
                        assert_eq!(expr.expression.trim(), "dynamic");
                    } else {
                        panic!("Expected Expression");
                    }
                    if let AttributeValuePart::Text(text) = &parts[2] {
                        assert_eq!(text.value, "-end");
                    } else {
                        panic!("Expected Text");
                    }
                } else {
                    panic!("Expected Concat, got {:?}", attr.value);
                }
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_parse_expression_only_in_attribute() {
        let result = Parser::new("<div class=\"{dynamic}\"/>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            if let Attribute::Normal(attr) = &el.attributes[0] {
                // Single expression in quotes simplifies to Expression
                if let AttributeValue::Expression(expr) = &attr.value {
                    assert_eq!(expr.expression.trim(), "dynamic");
                } else {
                    panic!("Expected Expression, got {:?}", attr.value);
                }
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_invalid_block_continuation_els_typo() {
        // {:els} is a typo for {:else}
        let result = Parser::new("{#if true}yes{:els}no{/if}", ParseOptions::default()).parse();
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].to_string().contains("els"));
        assert!(result.errors[0]
            .to_string()
            .contains("not a valid block continuation"));
    }

    #[test]
    fn test_invalid_block_continuation_elseif_typo() {
        // {:elseif} should be {:else if} (with a space)
        let result = Parser::new(
            "{#if true}yes{:elseif false}maybe{/if}",
            ParseOptions::default(),
        )
        .parse();
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].to_string().contains("elseif"));
    }

    #[test]
    fn test_invalid_block_continuation_than_typo() {
        // {:than} is a typo for {:then}
        let result = Parser::new(
            "{#await promise}{:than value}done{/await}",
            ParseOptions::default(),
        )
        .parse();
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].to_string().contains("than"));
    }

    #[test]
    fn test_invalid_block_continuation_chatch_typo() {
        // {:chatch} is a typo for {:catch}
        let result = Parser::new(
            "{#await promise}{:then value}done{:chatch e}error{/await}",
            ParseOptions::default(),
        )
        .parse();
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].to_string().contains("chatch"));
    }

    #[test]
    fn test_invalid_block_continuation_unknown_keyword() {
        // {:foo} is not a valid block continuation
        let result = Parser::new("{#if true}yes{:foo}no{/if}", ParseOptions::default()).parse();
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].to_string().contains("foo"));
    }

    #[test]
    fn test_invalid_block_continuation_empty() {
        // {:} with no keyword
        let result = Parser::new("{#if true}yes{:}no{/if}", ParseOptions::default()).parse();
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0]
            .to_string()
            .contains("expected block continuation keyword"));
    }

    #[test]
    fn test_valid_else_not_flagged() {
        // Valid {:else} should not produce errors
        let result = Parser::new("{#if true}yes{:else}no{/if}", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_valid_else_if_not_flagged() {
        // Valid {:else if} should not produce errors
        let result = Parser::new(
            "{#if a}1{:else if b}2{:else}3{/if}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_valid_then_not_flagged() {
        // Valid {:then} should not produce errors
        let result = Parser::new(
            "{#await promise}{:then value}{value}{/await}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_valid_catch_not_flagged() {
        // Valid {:catch} should not produce errors
        let result = Parser::new(
            "{#await promise}{:then value}{value}{:catch e}{e}{/await}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_valid_each_else_not_flagged() {
        // Valid {:else} in each block should not produce errors
        let result = Parser::new(
            "{#each items as item}{item}{:else}no items{/each}",
            ParseOptions::default(),
        )
        .parse();
        assert!(result.errors.is_empty());
    }

    // ==========================================
    // Tests for keywords as attribute names
    // Issue #5: https://github.com/pheuter/svelte-check-rs/issues/5
    // ==========================================

    #[test]
    fn test_html_as_attribute_name() {
        // Issue #5: html should be allowed as a prop name
        let result = Parser::new(r#"<Other html="test" />"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "html should be valid as an attribute name, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name, "html");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_const_as_attribute_name() {
        let result = Parser::new(r#"<Other const="value" />"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "const should be valid as an attribute name, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name, "const");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_debug_as_attribute_name() {
        let result = Parser::new(r#"<Other debug="true" />"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "debug should be valid as an attribute name, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name, "debug");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_render_as_attribute_name() {
        let result = Parser::new(r#"<Other render="lazy" />"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "render should be valid as an attribute name, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name, "render");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_snippet_as_attribute_name() {
        let result = Parser::new(r#"<Other snippet="code" />"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "snippet should be valid as an attribute name, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name, "snippet");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_multiple_keyword_attributes() {
        // Test multiple keywords used as attributes on the same element
        let result = Parser::new(
            r#"<Other html="escaped" const="fixed" debug="on" />"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Multiple keyword attributes should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 3);
            let names: Vec<_> = comp
                .attributes
                .iter()
                .filter_map(|a| {
                    if let Attribute::Normal(attr) = a {
                        Some(attr.name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(names, vec!["html", "const", "debug"]);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_keyword_attributes_on_html_elements() {
        // Keywords should also work as attributes on regular HTML elements
        let result = Parser::new(
            r#"<div html="test" data-if="cond" />"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Keyword attributes on HTML elements should work, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_keyword_attributes_with_expressions() {
        // Keywords as attributes with expression values
        let result = Parser::new(
            r#"<Other html={content} const={value} />"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Keyword attributes with expressions should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 2);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name, "html");
                assert!(matches!(attr.value, AttributeValue::Expression(_)));
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_keyword_attributes_boolean() {
        // Keywords as boolean attributes (no value)
        let result = Parser::new(r#"<Other html debug />"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Keyword boolean attributes should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 2);
            for attr in &comp.attributes {
                if let Attribute::Normal(a) = attr {
                    assert!(matches!(a.value, AttributeValue::True));
                }
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_all_block_keywords_as_attributes() {
        // Test all block keywords (if, else, each, key, await, then, catch) as attributes
        let keywords = [
            ("if", r#"<Other if="condition" />"#),
            ("else", r#"<Other else="fallback" />"#),
            ("each", r#"<Other each="items" />"#),
            ("key", r#"<Other key="id" />"#),
            ("await", r#"<Other await="promise" />"#),
            ("then", r#"<Other then="callback" />"#),
            ("catch", r#"<Other catch="handler" />"#),
            ("as", r#"<Other as="alias" />"#),
            ("style", r#"<Other style="color: red" />"#),
            ("script", r#"<Other script="src.js" />"#),
        ];

        for (keyword, code) in keywords {
            let result = Parser::new(code, ParseOptions::default()).parse();
            assert!(
                result.errors.is_empty(),
                "{} should be valid as an attribute name, code: {}, errors: {:?}",
                keyword,
                code,
                result.errors
            );
        }
    }

    #[test]
    fn test_html_special_tag_still_works() {
        // Ensure {@html} special tag still works after the fix
        let result = Parser::new(r#"{@html content}"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "{{@html}} should still work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::HtmlTag(tag) = &result.document.fragment.nodes[0] {
            assert_eq!(tag.expression.trim(), "content");
        } else {
            panic!(
                "Expected HtmlTag, got {:?}",
                result.document.fragment.nodes[0]
            );
        }
    }

    #[test]
    fn test_const_special_tag_still_works() {
        // Ensure {@const} special tag still works after the fix
        let result = Parser::new(
            r#"{#if true}{@const x = 1}{x}{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "{{@const}} should still work, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_debug_special_tag_still_works() {
        // Ensure {@debug} special tag still works after the fix
        let result = Parser::new(r#"{@debug foo}"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "{{@debug}} should still work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::DebugTag(tag) = &result.document.fragment.nodes[0] {
            assert!(!tag.identifiers.is_empty());
        } else {
            panic!("Expected DebugTag");
        }
    }

    #[test]
    fn test_render_special_tag_still_works() {
        // Ensure {@render} special tag still works after the fix
        let result = Parser::new(r#"{@render children()}"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "{{@render}} should still work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::RenderTag(tag) = &result.document.fragment.nodes[0] {
            assert!(tag.expression.contains("children"));
        } else {
            panic!("Expected RenderTag");
        }
    }

    #[test]
    fn test_snippet_block_still_works() {
        // Ensure {#snippet} block still works after the fix
        let result = Parser::new(
            r#"{#snippet mySnippet(param)}<div>{param}</div>{/snippet}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "{{#snippet}} should still work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::SnippetBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.name, "mySnippet");
        } else {
            panic!("Expected SnippetBlock");
        }
    }

    // ==========================================
    // Tests for HTML void elements
    // Issue #38: https://github.com/pheuter/svelte-check-rs/issues/38
    // ==========================================

    #[test]
    fn test_void_element_br_without_closing_tag() {
        // Issue #38: <br> should not require a closing tag
        let result = Parser::new("<div>line 1<br>line 2</div>", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "<br> should be valid without closing tag, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.name.as_str(), "div");
            // Should have: text, br, text
            assert_eq!(el.children.len(), 3);
            if let TemplateNode::Element(br) = &el.children[1] {
                assert_eq!(br.name.as_str(), "br");
                assert!(br.self_closing);
            } else {
                panic!("Expected br element");
            }
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_void_element_hr() {
        let result = Parser::new("<div><hr></div>", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "<hr> should be valid without closing tag, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_void_element_img() {
        let result = Parser::new(
            r#"<img src="test.png" alt="test">"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "<img> should be valid without closing tag, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.name.as_str(), "img");
            assert!(el.self_closing);
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_void_element_input() {
        let result =
            Parser::new(r#"<input type="text" name="foo">"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "<input> should be valid without closing tag, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.name.as_str(), "input");
            assert!(el.self_closing);
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_void_element_meta() {
        let result = Parser::new(r#"<meta charset="utf-8">"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "<meta> should be valid without closing tag, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_void_element_link() {
        let result = Parser::new(
            r#"<link rel="stylesheet" href="style.css">"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "<link> should be valid without closing tag, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_all_void_elements() {
        // Test all HTML void elements
        let void_elements = [
            "<area>", "<base>", "<br>", "<col>", "<embed>", "<hr>", "<img>", "<input>", "<link>",
            "<meta>", "<param>", "<source>", "<track>", "<wbr>",
        ];

        for element in void_elements {
            let result = Parser::new(element, ParseOptions::default()).parse();
            assert!(
                result.errors.is_empty(),
                "{} should be valid without closing tag, got errors: {:?}",
                element,
                result.errors
            );
        }
    }

    #[test]
    fn test_void_element_with_explicit_self_closing() {
        // Should also work with explicit self-closing syntax
        let result = Parser::new("<br/>", ParseOptions::default()).parse();
        assert!(result.errors.is_empty());

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert!(el.self_closing);
        } else {
            panic!("Expected Element");
        }
    }

    // ==========================================
    // Tests for dot notation in attributes
    // Issue #36: https://github.com/pheuter/svelte-check-rs/issues/36
    // ==========================================

    #[test]
    fn test_dot_notation_attribute() {
        // Issue #36: rotation.x should be a valid attribute name
        let result = Parser::new(
            r#"<T.Mesh rotation.x={Math.PI / 2}></T.Mesh>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "dot notation in attribute names should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.name.as_str(), "T.Mesh");
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name.as_str(), "rotation.x");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_dot_notation_attribute_on_element() {
        let result = Parser::new(r#"<div data.value={42}></div>"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "dot notation in attribute names should work on elements, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Element(el) = &result.document.fragment.nodes[0] {
            assert_eq!(el.attributes.len(), 1);
            if let Attribute::Normal(attr) = &el.attributes[0] {
                assert_eq!(attr.name.as_str(), "data.value");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Element");
        }
    }

    #[test]
    fn test_multiple_dot_notation_attributes() {
        let result = Parser::new(
            r#"<T.Mesh position.x={0} position.y={1} position.z={2}></T.Mesh>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "multiple dot notation attributes should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 3);
            let names: Vec<_> = comp
                .attributes
                .iter()
                .filter_map(|a| {
                    if let Attribute::Normal(attr) = a {
                        Some(attr.name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(names, vec!["position.x", "position.y", "position.z"]);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_deeply_nested_dot_notation() {
        let result = Parser::new(
            r#"<Component prop.nested.deep={value}></Component>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "deeply nested dot notation should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            if let Attribute::Normal(attr) = &comp.attributes[0] {
                assert_eq!(attr.name.as_str(), "prop.nested.deep");
            } else {
                panic!("Expected Normal attribute");
            }
        } else {
            panic!("Expected Component");
        }
    }

    // ==========================================
    // Tests for CSS custom property attributes
    // Issue #37: https://github.com/pheuter/svelte-check-rs/issues/37
    // ==========================================

    #[test]
    fn test_css_custom_property_attribute() {
        // Issue #37: --primary-color should be a valid attribute on components
        let result = Parser::new(
            r#"<Component --primary-color="red"></Component>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "CSS custom property attributes should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 1);
            if let Attribute::CssCustomProperty { name, value, .. } = &comp.attributes[0] {
                assert_eq!(name.as_str(), "primary-color");
                assert!(value.is_some());
            } else {
                panic!("Expected CssCustomProperty, got {:?}", comp.attributes[0]);
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_css_custom_property_with_expression() {
        let result = Parser::new(
            r#"<Component --bg-color={dynamicColor}></Component>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "CSS custom property with expression should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            if let Attribute::CssCustomProperty { name, value, .. } = &comp.attributes[0] {
                assert_eq!(name.as_str(), "bg-color");
                if let Some(AttributeValue::Expression(expr)) = value {
                    assert_eq!(expr.expression.trim(), "dynamicColor");
                } else {
                    panic!("Expected Expression value");
                }
            } else {
                panic!("Expected CssCustomProperty");
            }
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_multiple_css_custom_properties() {
        let result = Parser::new(
            r#"<Component --color="red" --size="16px" --weight="bold"></Component>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "multiple CSS custom properties should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 3);
            let names: Vec<_> = comp
                .attributes
                .iter()
                .filter_map(|a| {
                    if let Attribute::CssCustomProperty { name, .. } = a {
                        Some(name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(names, vec!["color", "size", "weight"]);
        } else {
            panic!("Expected Component");
        }
    }

    #[test]
    fn test_css_custom_property_on_html_element() {
        // CSS custom properties should also work on HTML elements
        let result =
            Parser::new(r#"<div --my-var="value"></div>"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "CSS custom property on HTML element should work, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_css_custom_property_mixed_with_regular_attrs() {
        let result = Parser::new(
            r#"<Component class="foo" --theme="dark" id="bar"></Component>"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "CSS custom properties mixed with regular attrs should work, got errors: {:?}",
            result.errors
        );

        if let TemplateNode::Component(comp) = &result.document.fragment.nodes[0] {
            assert_eq!(comp.attributes.len(), 3);
            // First should be regular attribute
            assert!(matches!(comp.attributes[0], Attribute::Normal(_)));
            // Second should be CSS custom property
            assert!(matches!(
                comp.attributes[1],
                Attribute::CssCustomProperty { .. }
            ));
            // Third should be regular attribute
            assert!(matches!(comp.attributes[2], Attribute::Normal(_)));
        } else {
            panic!("Expected Component");
        }
    }

    // ==========================================
    // Tests for regex literals in expressions
    // Issue #46: https://github.com/pheuter/svelte-check-rs/issues/46
    // ==========================================

    #[test]
    fn test_regex_literal_simple() {
        // Simple regex in expression should parse correctly
        let result = Parser::new("{value.match(/test/)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Simple regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "value.match(/test/)");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_with_parens() {
        // Regex containing parentheses should parse correctly
        let result =
            Parser::new("{value.match(/^(.+?)\\s*test$/)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex with parens should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains("/^(.+?)"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_with_brackets() {
        // Regex containing brackets with special chars should parse correctly
        let result = Parser::new("{value.match(/[^)]+/)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex with brackets should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "value.match(/[^)]+/)");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_with_braces_in_char_class() {
        // Regex with } inside character class should parse correctly
        let result = Parser::new("{value.match(/[{}]+/)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex with braces in char class should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "value.match(/[{}]+/)");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_with_escaped_slash() {
        // Regex with escaped slash should parse correctly
        let result = Parser::new(
            r#"{value.match(/path\/to\/file/)}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Regex with escaped slash should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains(r"path\/to\/file"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_with_flags() {
        // Regex with flags should parse correctly
        let result = Parser::new("{value.match(/test/gi)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex with flags should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "value.match(/test/gi)");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_complex_pattern() {
        // Complex regex from issue #46 - regex with multiple special chars
        let result = Parser::new(
            r#"{value.match(/^(.+?)\s*\(([^)]+)\)$/)}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Complex regex pattern should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains(r"/^(.+?)"));
            assert!(expr.expression.contains(r"([^)]+)"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_literal_rgba_pattern() {
        // RGBA pattern regex from issue #46
        let result = Parser::new(
            r#"{/rgba\([^)]+[,/]\s*0(\.0*)?\s*\)$/.test(color)}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "RGBA regex pattern should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains(r"rgba\("));
            assert!(expr.expression.contains(".test(color)"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_const_tag_with_regex_match() {
        // {@const} with regex match - issue #46 case 1
        let result = Parser::new(
            r#"{#if true}{@const [, label] = value.match(/^(.+?)\s*\(([^)]+)\)$/) ?? [, value, ``]}{label}{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "@const with regex match should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.consequent.nodes.len(), 2); // ConstTag and Expression
            if let TemplateNode::ConstTag(const_tag) = &block.consequent.nodes[0] {
                assert!(const_tag.declaration.contains("value.match("));
                assert!(const_tag.declaration.contains("[, value, ``]"));
                // Make sure declaration doesn't include content after the }
                assert!(!const_tag.declaration.contains("{label}"));
            } else {
                panic!("Expected ConstTag, got {:?}", block.consequent.nodes[0]);
            }
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_const_tag_with_regex_test() {
        // {@const} with regex.test() - issue #46 case 2
        let result = Parser::new(
            r#"{#if true}{@const matches = /rgba\([^)]+\)$/.test(color)}{matches}{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "@const with regex.test() should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            if let TemplateNode::ConstTag(const_tag) = &block.consequent.nodes[0] {
                assert!(const_tag.declaration.contains("matches ="));
                assert!(const_tag.declaration.contains(".test(color)"));
                assert!(!const_tag.declaration.contains("{matches}"));
            } else {
                panic!("Expected ConstTag");
            }
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_multiple_const_tags_with_regex() {
        // Multiple {@const} tags where first has regex - issue #46 main case
        let result = Parser::new(
            r#"{#if true}{@const a = /test/.test(x)}{@const b = 2}{a}{b}{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Multiple @const with regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            // Should have: ConstTag, ConstTag, Expression, Expression
            assert_eq!(block.consequent.nodes.len(), 4);
            if let TemplateNode::ConstTag(const_a) = &block.consequent.nodes[0] {
                assert!(const_a.declaration.contains("a ="));
                assert!(const_a.declaration.contains("/test/"));
                assert!(!const_a.declaration.contains("@const b"));
            } else {
                panic!("Expected first ConstTag");
            }
            if let TemplateNode::ConstTag(const_b) = &block.consequent.nodes[1] {
                assert!(const_b.declaration.contains("b = 2"));
            } else {
                panic!("Expected second ConstTag");
            }
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_snippet_with_const_and_regex() {
        // Snippet containing @const with regex - issue #46 case
        let result = Parser::new(
            r#"{#snippet tooltip({ x })}
    {@const match = x.match(/test/)}
    <span>{match}</span>
{/snippet}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Snippet with @const and regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::SnippetBlock(block) = &result.document.fragment.nodes[0] {
            assert_eq!(block.name, "tooltip");
            // Body should have: ConstTag, Element
            let non_whitespace: Vec<_> = block
                .body
                .nodes
                .iter()
                .filter(|n| !matches!(n, TemplateNode::Text(t) if t.is_whitespace))
                .collect();
            assert_eq!(non_whitespace.len(), 2);
        } else {
            panic!("Expected SnippetBlock");
        }
    }

    #[test]
    fn test_division_vs_regex_after_paren() {
        // Division after ) should not be treated as regex
        let result = Parser::new("{(a + b) / 2}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Division should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "(a + b) / 2");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_division_vs_regex_after_ident() {
        // Division after identifier should not be treated as regex
        let result = Parser::new("{count / 2}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Division after ident should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "count / 2");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_equals() {
        // Regex after = should be treated as regex
        let result = Parser::new("{x = /test/}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex after = should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "x = /test/");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_comma() {
        // Regex after comma in function call
        let result = Parser::new("{fn(x, /test/)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex after comma should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "fn(x, /test/)");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_open_paren() {
        // Regex after ( should be treated as regex
        let result = Parser::new("{fn(/test/)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex after ( should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "fn(/test/)");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_open_bracket() {
        // Regex in array literal
        let result = Parser::new("{[/a/, /b/]}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex in array should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert_eq!(expr.expression.trim(), "[/a/, /b/]");
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_colon() {
        // Regex in object literal value
        let result = Parser::new("{{ pattern: /test/ }}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex in object should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains("pattern: /test/"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_logical_operators() {
        // Regex after || and &&
        let result = Parser::new("{a || /test/.test(b)}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex after || should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains("|| /test/"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_ternary() {
        // Regex in ternary expression
        let result = Parser::new("{cond ? /yes/ : /no/}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex in ternary should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains("? /yes/"));
            assert!(expr.expression.contains(": /no/"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_after_return() {
        // Regex after return keyword (in arrow function)
        let result = Parser::new(
            "{items.filter(x => /test/.test(x))}",
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "Regex in arrow function should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains("=> /test/"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_const_with_arrow_function_and_regex() {
        // Issue #46: @const with arrow function containing regex
        let result = Parser::new(
            r#"{#if true}{@const check = (s: string): boolean => /test/.test(s)}{check("x")}{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "@const with arrow function and regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            if let TemplateNode::ConstTag(const_tag) = &block.consequent.nodes[0] {
                assert!(const_tag.declaration.contains("=> /test/"));
                assert!(!const_tag.declaration.contains("check("));
            } else {
                panic!("Expected ConstTag");
            }
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_const_with_iife_and_regex() {
        // Issue #46: @const with IIFE containing regex
        let result = Parser::new(
            r#"{#if true}{@const result = (() => { return /test/.test(x); })()}{result}{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "@const with IIFE and regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            if let TemplateNode::ConstTag(const_tag) = &block.consequent.nodes[0] {
                assert!(const_tag.declaration.contains("(() =>"));
                assert!(const_tag.declaration.contains("/test/"));
                assert!(const_tag.declaration.ends_with("()"));
                assert!(!const_tag.declaration.contains("{result}"));
            } else {
                panic!("Expected ConstTag");
            }
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_html_tag_with_regex() {
        // {@html} with regex
        let result = Parser::new(
            r#"{@html value.replace(/test/g, 'replaced')}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "@html with regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::HtmlTag(tag) = &result.document.fragment.nodes[0] {
            assert!(tag.expression.contains("/test/g"));
            assert!(tag.expression.contains("'replaced'"));
        } else {
            panic!("Expected HtmlTag");
        }
    }

    #[test]
    fn test_render_tag_with_regex_in_args() {
        // {@render} with regex in arguments
        let result = Parser::new(
            r#"{@render snippet({ pattern: /test/ })}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "@render with regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::RenderTag(tag) = &result.document.fragment.nodes[0] {
            assert!(tag.expression.contains("pattern: /test/"));
        } else {
            panic!("Expected RenderTag");
        }
    }

    #[test]
    fn test_if_condition_with_regex() {
        // {#if} with regex in condition
        let result = Parser::new(
            r#"{#if /test/.test(value)}yes{/if}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "if condition with regex should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::IfBlock(block) = &result.document.fragment.nodes[0] {
            assert!(block.condition.contains("/test/"));
        } else {
            panic!("Expected IfBlock");
        }
    }

    #[test]
    fn test_each_expression_with_regex_filter() {
        // {#each} with regex in filter
        let result = Parser::new(
            r#"{#each items.filter(x => /test/.test(x)) as item}{item}{/each}"#,
            ParseOptions::default(),
        )
        .parse();
        assert!(
            result.errors.is_empty(),
            "each with regex filter should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::EachBlock(block) = &result.document.fragment.nodes[0] {
            assert!(block.expression.contains("/test/"));
        } else {
            panic!("Expected EachBlock");
        }
    }

    #[test]
    fn test_regex_with_quantifier_braces() {
        // Regex with {n,m} quantifier - should not confuse closing }
        let result = Parser::new(r#"{value.match(/\d{2,4}/)}"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex with quantifier braces should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains(r"\d{2,4}"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_regex_with_escape_sequences() {
        // Regex with various escape sequences
        let result = Parser::new(r#"{value.match(/\n\t\r\s\w/)}"#, ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex with escape sequences should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains(r"\n\t\r\s\w"));
        } else {
            panic!("Expected Expression");
        }
    }

    #[test]
    fn test_nested_regex_in_template_literal() {
        // Regex inside template literal expression
        let result = Parser::new("{`matches: ${/test/.test(x)}`}", ParseOptions::default()).parse();
        assert!(
            result.errors.is_empty(),
            "Regex in template literal should parse without errors, got: {:?}",
            result.errors
        );

        if let TemplateNode::Expression(expr) = &result.document.fragment.nodes[0] {
            assert!(expr.expression.contains("/test/"));
        } else {
            panic!("Expected Expression");
        }
    }
}
