//! Recursive descent parser for Svelte 5.

use crate::ast::*;
use crate::error::{ParseError, ParseErrorKind};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::{ParseOptions, ParseResult};
use smol_str::SmolStr;
use source_map::Span;
use text_size::TextSize;

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
            TokenKind::LAngle => self.parse_element_or_component(),
            TokenKind::LBraceHash => self.parse_block(),
            TokenKind::LBraceAt => self.parse_special_tag(),
            TokenKind::LBrace => self.parse_expression_tag(),
            TokenKind::Text | TokenKind::Ident | TokenKind::Number => self.parse_text(),
            TokenKind::Newline => {
                self.advance();
                None
            }
            _ => None,
        }
    }

    /// Parses an element or component.
    fn parse_element_or_component(&mut self) -> Option<TemplateNode> {
        let start = self.current().span.start;

        // Expect `<`
        if !self.eat(TokenKind::LAngle) {
            return None;
        }

        // Get tag name
        let name = if self.check(TokenKind::Ident) || self.check(TokenKind::NamespacedIdent) {
            SmolStr::new(self.current_text())
        } else {
            return None;
        };
        self.advance();

        // Check if this is a component (PascalCase) or svelte: element
        let is_component = name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);
        let is_svelte_element = name.starts_with("svelte:");

        // Parse attributes
        let attributes = self.parse_attributes();

        // Check for self-closing
        let self_closing = self.eat(TokenKind::SlashRAngle);

        if !self_closing {
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

        // Check for identifier (normal attribute or directive)
        if !self.check(TokenKind::Ident) && !self.check(TokenKind::NamespacedIdent) {
            return None;
        }

        let full_name = self.current_text().to_string();
        self.advance();

        // Check for directive (name:arg)
        if let Some(colon_pos) = full_name.find(':') {
            let directive_name = &full_name[..colon_pos];
            let arg_name = &full_name[colon_pos + 1..];

            let kind = match directive_name {
                "on" => DirectiveKind::On,
                "bind" => DirectiveKind::Bind,
                "class" => DirectiveKind::Class,
                "style" => DirectiveKind::StyleDirective,
                "use" => DirectiveKind::Use,
                "transition" => DirectiveKind::Transition,
                "in" => DirectiveKind::In,
                "out" => DirectiveKind::Out,
                "animate" => DirectiveKind::Animate,
                "let" => DirectiveKind::Let,
                _ => return None,
            };

            // Parse modifiers (|modifier)
            let mut modifiers = Vec::new();
            let mut remaining = arg_name.to_string();
            while let Some(pipe_pos) = remaining.find('|') {
                let modifier = &remaining[pipe_pos + 1..];
                modifiers.push(SmolStr::new(modifier));
                remaining = remaining[..pipe_pos].to_string();
            }

            // Parse value
            let expression = if self.eat(TokenKind::Eq) {
                if self.eat(TokenKind::LBrace) {
                    let (expr, expr_span) = self.read_until(&["}"]);
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
            let (text, span) = self.read_until(&["\""]);
            self.eat(TokenKind::DoubleQuote);
            AttributeValue::Text(TextValue { span, value: text })
        } else if self.check(TokenKind::SingleQuote) {
            self.advance();
            let (text, span) = self.read_until(&["'"]);
            self.eat(TokenKind::SingleQuote);
            AttributeValue::Text(TextValue { span, value: text })
        } else if self.check(TokenKind::LBrace) {
            let start = self.current().span.start;
            self.advance();
            let (expr, expr_span) = self.read_until(&["}"]);
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
            })
        } else {
            AttributeValue::True
        }
    }

    /// Parses a spread attribute or shorthand.
    fn parse_spread_or_shorthand(&mut self) -> Option<Attribute> {
        let start = self.current().span.start;

        if !self.eat(TokenKind::LBrace) {
            return None;
        }

        let (expr, expr_span) = self.read_until(&["}"]);
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

        let found_name = if self.check(TokenKind::Ident) || self.check(TokenKind::NamespacedIdent) {
            self.current_text().to_string()
        } else {
            String::new()
        };
        self.advance();

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
        let (condition, condition_span) = self.read_until(&["}"]);
        self.eat(TokenKind::RBrace);

        // Parse consequent
        let consequent = self.parse_block_children(&["{:else", "{/if"]);

        // Check for else
        let alternate = if self.check_source("{:else if") {
            self.eat(TokenKind::LBraceColon);
            self.eat(TokenKind::Else);
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
            self.advance(); // {
            self.advance(); // :
            self.advance(); // else
            self.eat(TokenKind::RBrace);
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
        let (full_expr, _) = self.read_until(&["}"]);
        self.eat(TokenKind::RBrace);

        // Parse the expression
        let parts: Vec<&str> = full_expr.split(" as ").collect();
        let expression = parts.first().unwrap_or(&"").trim().to_string();
        let expression_span = Span::empty(start); // Simplified

        let rest = parts.get(1).unwrap_or(&"").trim();

        // Parse context and index
        let (context, index, key) = if let Some(paren_pos) = rest.find('(') {
            let before_paren = &rest[..paren_pos].trim();
            let key_expr = &rest[paren_pos + 1..rest.len() - 1];

            let (ctx, idx) = if let Some(comma_pos) = before_paren.find(',') {
                (
                    before_paren[..comma_pos].trim().to_string(),
                    Some(SmolStr::new(before_paren[comma_pos + 1..].trim())),
                )
            } else {
                (before_paren.to_string(), None)
            };

            (
                ctx,
                idx,
                Some(EachKey {
                    span: Span::empty(start),
                    expression: key_expr.trim().to_string(),
                }),
            )
        } else if let Some(comma_pos) = rest.find(',') {
            (
                rest[..comma_pos].trim().to_string(),
                Some(SmolStr::new(rest[comma_pos + 1..].trim())),
                None,
            )
        } else {
            (rest.to_string(), None, None)
        };

        let context_span = Span::empty(start); // Simplified

        // Parse body
        let body = self.parse_block_children(&["{:else", "{/each"]);

        // Check for else
        let fallback = if self.check_source("{:else}") {
            self.advance(); // {
            self.advance(); // :
            self.advance(); // else
            self.eat(TokenKind::RBrace);
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
        let (full_expr, expression_span) = self.read_until(&["}"]);
        self.eat(TokenKind::RBrace);

        // Check for shorthand: {#await promise then value}
        let (expression, immediate_then) = if full_expr.contains(" then ") {
            let parts: Vec<&str> = full_expr.split(" then ").collect();
            (
                parts[0].trim().to_string(),
                Some(parts.get(1).unwrap_or(&"").trim().to_string()),
            )
        } else {
            (full_expr.trim().to_string(), None)
        };

        // Parse pending content or body
        let (pending, then, catch) = if let Some(then_value) = immediate_then {
            let body = self.parse_block_children(&["{:catch", "{/await"]);

            let catch_block = if self.check_source("{:catch") {
                self.advance();
                self.advance();
                self.advance();
                let (error_name, _) = self.read_until(&["}"]);
                self.eat(TokenKind::RBrace);
                let catch_body = self.parse_block_children(&["{/await"]);
                Some(AwaitCatch {
                    span: Span::empty(start),
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
                    span: Span::empty(start),
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
                self.advance();
                self.advance();
                self.advance();
                let (value_name, _) = self.read_until(&["}"]);
                self.eat(TokenKind::RBrace);
                let then_body = self.parse_block_children(&["{:catch", "{/await"]);
                Some(AwaitThen {
                    span: Span::empty(start),
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
                self.advance();
                self.advance();
                self.advance();
                let (error_name, _) = self.read_until(&["}"]);
                self.eat(TokenKind::RBrace);
                let catch_body = self.parse_block_children(&["{/await"]);
                Some(AwaitCatch {
                    span: Span::empty(start),
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
        let (expression, expression_span) = self.read_until(&["}"]);
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
        let (full_signature, _) = self.read_until(&["}"]);
        self.eat(TokenKind::RBrace);

        // Parse name and parameters: name(params)
        let (name, parameters, parameters_span) = if let Some(paren_pos) = full_signature.find('(')
        {
            let name = full_signature[..paren_pos].trim();
            let params_end = full_signature.rfind(')').unwrap_or(full_signature.len());
            let params = &full_signature[paren_pos + 1..params_end];
            (SmolStr::new(name), params.to_string(), Span::empty(start))
        } else {
            (
                SmolStr::new(full_signature.trim()),
                String::new(),
                Span::empty(start),
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
                let (expression, expression_span) = self.read_until(&["}"]);
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
                let (declaration, declaration_span) = self.read_until(&["}"]);
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
                let (identifiers_str, _) = self.read_until(&["}"]);
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
                let (expression, expression_span) = self.read_until(&["}"]);
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

        let (expression, expression_span) = self.read_until(&["}"]);
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
            TokenKind::Text | TokenKind::Ident | TokenKind::Number | TokenKind::Newline
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
}
