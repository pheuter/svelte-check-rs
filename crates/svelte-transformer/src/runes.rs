//! Rune transformation logic.
//!
//! Transforms Svelte 5 runes into their TypeScript equivalents for type-checking.
//! This implementation uses a token-aware scanner that properly handles strings,
//! comments, and template literals to avoid false transformations.

use smol_str::SmolStr;
use source_map::Span;
use std::collections::HashSet;

/// Information about a rune found in the script.
#[derive(Debug, Clone)]
pub struct RuneInfo {
    /// The kind of rune.
    pub kind: RuneKind,
    /// The variable name being assigned (if applicable).
    pub variable_name: Option<SmolStr>,
    /// The original span in the source.
    pub original_span: Span,
    /// The span of the content/argument inside the rune call.
    pub content_span: Option<Span>,
}

/// The kind of rune.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuneKind {
    /// `$props()`
    Props,
    /// `$state(init)`
    State,
    /// `$state.raw(init)`
    StateRaw,
    /// `$state.snapshot(value)`
    StateSnapshot,
    /// `$derived(expr)`
    Derived,
    /// `$derived.by(fn)`
    DerivedBy,
    /// `$effect(fn)`
    Effect,
    /// `$effect.pre(fn)`
    EffectPre,
    /// `$effect.root(fn)`
    EffectRoot,
    /// `$bindable(default?)`
    Bindable,
    /// `$inspect(value)`
    Inspect,
    /// `$host()`
    Host,
}

/// Result of rune transformation including output and mappings.
#[derive(Debug)]
pub struct RuneTransformResult {
    /// The transformed script content.
    pub output: String,
    /// Information about each rune transformation.
    pub runes: Vec<RuneInfo>,
    /// Mappings from original spans to generated spans.
    pub mappings: Vec<RuneMapping>,
    /// Store names referenced via $store syntax in the script.
    pub store_names: HashSet<SmolStr>,
    /// Whether `$props.<name>()` accessors are used in the script.
    pub uses_props_accessor: bool,
}

/// A mapping from an original span to a generated span.
#[derive(Debug, Clone)]
pub struct RuneMapping {
    /// The span in the original source.
    pub original: Span,
    /// The span in the generated output.
    pub generated: Span,
}

/// Scanner state for tracking context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanContext {
    /// Normal code context.
    Code,
    /// Inside a single-quoted string.
    SingleQuoteString,
    /// Inside a double-quoted string.
    DoubleQuoteString,
    /// Inside a template literal.
    TemplateLiteral,
    /// Inside a single-line comment.
    LineComment,
    /// Inside a block comment.
    BlockComment,
    /// Inside a regex literal (placeholder for future implementation).
    #[allow(dead_code)]
    RegexLiteral,
}

struct StoreScanResult {
    store_names: HashSet<SmolStr>,
    uses_props_accessor: bool,
}

/// Collect store subscriptions in script output without rewriting them.
///
/// This preserves `$store` identifiers so TypeScript can narrow on them,
/// while we emit `$store` aliases in the script prologue.
fn scan_store_subscriptions(expr: &str) -> StoreScanResult {
    let mut store_names = HashSet::new();
    let mut uses_props_accessor = false;
    let mut chars = expr.chars().peekable();

    let mut in_string: Option<char> = None;
    let mut prev_was_escape = false;
    let mut template_brace_depth: Vec<usize> = Vec::new();
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if let Some(quote) = in_string {
            if quote != '`' {
                if prev_was_escape {
                    prev_was_escape = false;
                } else if ch == '\\' {
                    prev_was_escape = true;
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            } else {
                if prev_was_escape {
                    prev_was_escape = false;
                    continue;
                }
                if ch == '\\' {
                    prev_was_escape = true;
                    continue;
                }
                if ch == '`' {
                    in_string = None;
                    continue;
                }
                if ch == '$' && chars.peek() == Some(&'{') {
                    chars.next();
                    template_brace_depth.push(0);
                    in_string = None;
                    continue;
                }
                continue;
            }
        }

        if !template_brace_depth.is_empty() {
            if ch == '{' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    *depth += 1;
                }
            } else if ch == '}' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    if *depth == 0 {
                        template_brace_depth.pop();
                        in_string = Some('`');
                        continue;
                    } else {
                        *depth -= 1;
                    }
                }
            }
        }

        if ch == '/' {
            if chars.peek() == Some(&'/') {
                chars.next();
                in_line_comment = true;
                continue;
            } else if chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment = true;
                continue;
            }
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            in_string = Some(ch);
            continue;
        }

        if ch == '$' {
            if let Some(&next) = chars.peek() {
                if next == '$' {
                    chars.next();
                    continue;
                }
                if next.is_ascii_alphabetic() || next == '_' {
                    let mut identifier = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            identifier.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    let rest: String = chars.clone().collect();
                    let trimmed_rest = rest.trim_start();
                    let next_non_ws = trimmed_rest.chars().next();

                    if identifier == "props" {
                        if matches!(next_non_ws, Some('.') | Some('?') | Some('[')) {
                            uses_props_accessor = true;
                        }
                        continue;
                    }

                    let mut is_rune = matches!(next_non_ws, Some('(') | Some('<') | Some(':'));
                    if !is_rune && next_non_ws == Some('.') {
                        let is_dot_rune = match identifier.as_str() {
                            "derived" => trimmed_rest.starts_with(".by"),
                            "state" => {
                                trimmed_rest.starts_with(".raw")
                                    || trimmed_rest.starts_with(".snapshot")
                            }
                            "effect" => {
                                trimmed_rest.starts_with(".pre")
                                    || trimmed_rest.starts_with(".root")
                            }
                            _ => false,
                        };
                        if is_dot_rune {
                            is_rune = true;
                        }
                    }
                    if !is_rune {
                        store_names.insert(SmolStr::new(&identifier));
                    }
                    continue;
                }
            }
        }
    }

    StoreScanResult {
        store_names,
        uses_props_accessor,
    }
}

/// Transforms rune expressions in script content.
///
/// This function identifies rune calls and preserves them for type-checking.
/// The transformed output relies on ambient type declarations (in the helpers file)
/// that match Svelte's official rune signatures. This approach ensures TypeScript
/// understands the reactive semantics without false "used before assigned" errors.
///
/// Runes are preserved as-is (with trailing comma stripping for validity):
/// - `$state(init)` → `$state(init)`
/// - `$state<T>(init)` → `$state<T>(init)`
/// - `$state.raw(init)` → `$state.raw(init)`
/// - `$state.snapshot(x)` → `$state.snapshot(x)`
/// - `$derived(expr)` → `$derived(expr)`
/// - `$derived.by(fn)` → `$derived.by(fn)`
/// - `$effect(fn)` → `$effect(fn)`
/// - `$effect.pre(fn)` → `$effect.pre(fn)`
/// - `$effect.root(fn)` → `$effect.root(fn)`
/// - `$bindable(default?)` → `$bindable(default?)`
/// - `$inspect(...)` → `$inspect(...)`
/// - `$host()` → `$host()`
/// - `$props()` → `({} as __SvelteLoosen<Type>)` (special handling for component props)
///
/// The `default_props_type` parameter specifies the type to use for untyped `$props()`.
/// For SvelteKit route files, this would be "PageData" or "LayoutData".
pub fn transform_runes(script: &str, base_offset: u32) -> RuneTransformResult {
    transform_runes_with_options(script, base_offset, None)
}

/// Transforms rune expressions with a custom default props type.
pub fn transform_runes_with_options(
    script: &str,
    base_offset: u32,
    default_props_type: Option<&str>,
) -> RuneTransformResult {
    let scanner = RuneScanner::new(script, base_offset, default_props_type);
    let mut result = scanner.scan_and_transform();

    // Collect store subscriptions so we can declare aliases in the script prologue.
    let store_scan = scan_store_subscriptions(&result.output);
    result.store_names = store_scan.store_names;
    result.uses_props_accessor = store_scan.uses_props_accessor;

    result
}

/// A scanner that finds and transforms runes while respecting string/comment contexts.
struct RuneScanner<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    output: String,
    context: ScanContext,
    /// Stack for nested template literals with expressions.
    template_depth: usize,
    /// Brace depth for tracking template expression boundaries.
    brace_depth: Vec<usize>,
    runes: Vec<RuneInfo>,
    mappings: Vec<RuneMapping>,
    base_offset: u32,
    /// Current position in output.
    output_pos: u32,
    /// Default type to use for untyped $props().
    default_props_type: Option<&'a str>,
}

impl<'a> RuneScanner<'a> {
    fn new(source: &'a str, base_offset: u32, default_props_type: Option<&'a str>) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            output: String::with_capacity(source.len()),
            context: ScanContext::Code,
            template_depth: 0,
            brace_depth: Vec::new(),
            runes: Vec::new(),
            mappings: Vec::new(),
            base_offset,
            output_pos: 0,
            default_props_type,
        }
    }

    fn scan_and_transform(mut self) -> RuneTransformResult {
        while let Some((pos, ch)) = self.chars.next() {
            match self.context {
                ScanContext::Code => self.handle_code(pos, ch),
                ScanContext::SingleQuoteString => self.handle_single_quote_string(ch),
                ScanContext::DoubleQuoteString => self.handle_double_quote_string(ch),
                ScanContext::TemplateLiteral => self.handle_template_literal(pos, ch),
                ScanContext::LineComment => self.handle_line_comment(ch),
                ScanContext::BlockComment => self.handle_block_comment(ch),
                ScanContext::RegexLiteral => self.handle_regex_literal(ch),
            }
        }

        RuneTransformResult {
            output: self.output,
            runes: self.runes,
            mappings: self.mappings,
            store_names: HashSet::new(),
            uses_props_accessor: false,
        }
    }

    fn handle_code(&mut self, pos: usize, ch: char) {
        match ch {
            '\'' => {
                self.push_char(ch);
                self.context = ScanContext::SingleQuoteString;
            }
            '"' => {
                self.push_char(ch);
                self.context = ScanContext::DoubleQuoteString;
            }
            '`' => {
                self.push_char(ch);
                self.context = ScanContext::TemplateLiteral;
                self.template_depth += 1;
            }
            '/' => {
                if let Some((_, next)) = self.chars.peek().copied() {
                    match next {
                        '/' => {
                            self.push_char(ch);
                            let next_ch = self.chars.next().unwrap().1;
                            self.push_char(next_ch);
                            self.context = ScanContext::LineComment;
                        }
                        '*' => {
                            self.push_char(ch);
                            let next_ch = self.chars.next().unwrap().1;
                            self.push_char(next_ch);
                            self.context = ScanContext::BlockComment;
                        }
                        _ => {
                            // Could be regex or division - for simplicity, treat as code
                            // A full implementation would track expression context
                            self.push_char(ch);
                        }
                    }
                } else {
                    self.push_char(ch);
                }
            }
            '$' => {
                // Check if this is a rune
                if let Some(rune_match) = self.try_match_rune(pos) {
                    self.apply_rune_transform(rune_match);
                } else {
                    // Just emit the $ - store subscriptions are collected later
                    // so we can declare aliases in the script prologue.
                    self.push_char(ch);
                }
            }
            '{' => {
                self.push_char(ch);
                if let Some(depth) = self.brace_depth.last_mut() {
                    *depth += 1;
                }
            }
            '}' => {
                if let Some(depth) = self.brace_depth.last_mut() {
                    if *depth == 0 {
                        // End of template expression
                        self.brace_depth.pop();
                        self.push_char(ch);
                        self.context = ScanContext::TemplateLiteral;
                    } else {
                        *depth -= 1;
                        self.push_char(ch);
                    }
                } else {
                    self.push_char(ch);
                }
            }
            _ => self.push_char(ch),
        }
    }

    fn handle_single_quote_string(&mut self, ch: char) {
        self.push_char(ch);
        match ch {
            '\'' => self.context = ScanContext::Code,
            '\\' => {
                // Skip next char (escaped)
                if let Some((_, next)) = self.chars.next() {
                    self.push_char(next);
                }
            }
            _ => {}
        }
    }

    fn handle_double_quote_string(&mut self, ch: char) {
        self.push_char(ch);
        match ch {
            '"' => self.context = ScanContext::Code,
            '\\' => {
                if let Some((_, next)) = self.chars.next() {
                    self.push_char(next);
                }
            }
            _ => {}
        }
    }

    fn handle_template_literal(&mut self, _pos: usize, ch: char) {
        match ch {
            '`' => {
                self.push_char(ch);
                self.template_depth = self.template_depth.saturating_sub(1);
                // After exiting a template literal, check if we're still inside a template
                // expression (brace_depth not empty). If so, return to Code context to
                // properly handle the closing `}`. Otherwise, only return to Code if
                // we've exited all nested template literals (template_depth == 0).
                if self.template_depth == 0 || !self.brace_depth.is_empty() {
                    self.context = ScanContext::Code;
                }
            }
            '$' => {
                if let Some((_, '{')) = self.chars.peek().copied() {
                    // Template expression start
                    self.push_char(ch);
                    let next_ch = self.chars.next().unwrap().1;
                    self.push_char(next_ch);
                    self.brace_depth.push(0);
                    self.context = ScanContext::Code;
                } else {
                    self.push_char(ch);
                }
            }
            '\\' => {
                self.push_char(ch);
                if let Some((_, next)) = self.chars.next() {
                    self.push_char(next);
                }
            }
            _ => self.push_char(ch),
        }
    }

    fn handle_line_comment(&mut self, ch: char) {
        self.push_char(ch);
        if ch == '\n' {
            self.context = ScanContext::Code;
        }
    }

    fn handle_block_comment(&mut self, ch: char) {
        self.push_char(ch);
        if ch == '*' {
            if let Some((_, '/')) = self.chars.peek().copied() {
                let next_ch = self.chars.next().unwrap().1;
                self.push_char(next_ch);
                self.context = ScanContext::Code;
            }
        }
    }

    fn handle_regex_literal(&mut self, ch: char) {
        self.push_char(ch);
        match ch {
            '/' => self.context = ScanContext::Code,
            '\\' => {
                if let Some((_, next)) = self.chars.next() {
                    self.push_char(next);
                }
            }
            _ => {}
        }
    }

    fn push_char(&mut self, ch: char) {
        self.output.push(ch);
        self.output_pos += ch.len_utf8() as u32;
    }

    fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
        self.output_pos += s.len() as u32;
    }

    /// Strips trailing comma (and surrounding whitespace) from rune argument content.
    ///
    /// JavaScript/TypeScript allows trailing commas in function arguments:
    ///   $state<Type>(
    ///       value,
    ///   )
    ///
    /// When transforming to `(value as Type)`, we must strip the trailing comma
    /// to produce valid TypeScript, otherwise we'd get `(value, as Type)`.
    ///
    /// Returns (body, trailing_whitespace) where:
    /// - body: content with trailing comma stripped
    /// - trailing_whitespace: whitespace after the value (for formatting preservation)
    fn strip_trailing_comma(content: &str) -> (&str, &str) {
        // First find where trailing whitespace starts
        let trimmed = content.trim_end();
        let trailing_ws_start = trimmed.len();
        let trailing_ws = &content[trailing_ws_start..];

        // Check if the last non-whitespace character is a comma
        if let Some(without_comma) = trimmed.strip_suffix(',') {
            // Strip the comma and any whitespace before it
            (without_comma.trim_end(), trailing_ws)
        } else {
            (trimmed, trailing_ws)
        }
    }

    /// Try to match a rune at the current position.
    fn try_match_rune(&mut self, start_pos: usize) -> Option<RuneMatch> {
        let remaining = &self.source[start_pos..];

        // Try each rune pattern in order of specificity (longer patterns first)
        let patterns: &[(&str, RuneKind)] = &[
            ("$state.snapshot(", RuneKind::StateSnapshot),
            ("$state.raw(", RuneKind::StateRaw),
            ("$derived.by(", RuneKind::DerivedBy),
            ("$effect.root(", RuneKind::EffectRoot),
            ("$effect.pre(", RuneKind::EffectPre),
            ("$bindable(", RuneKind::Bindable),
            ("$inspect(", RuneKind::Inspect),
            ("$derived(", RuneKind::Derived),
            ("$effect(", RuneKind::Effect),
            ("$state(", RuneKind::State),
            ("$props(", RuneKind::Props),
            ("$host()", RuneKind::Host),
        ];

        // Special cases for runes with TypeScript generics: $rune<Type>() or $rune<Type>(init)
        // Check $state.raw<Type>() first (longer pattern takes precedence over $state<)
        if remaining.starts_with("$state.raw<") {
            if let Some((full_end, content, content_span, generic)) =
                self.find_rune_with_generic(start_pos, "$state.raw<")
            {
                return Some(RuneMatch {
                    kind: RuneKind::StateRaw,
                    full_span: Span::new(
                        self.base_offset + start_pos as u32,
                        self.base_offset + full_end as u32,
                    ),
                    content,
                    content_span,
                    pattern_len: 11, // "$state.raw<"
                    generic: Some(generic),
                });
            }
        }

        // Check $state<Type>() - transforms to the content or undefined
        if remaining.starts_with("$state<") {
            if let Some((full_end, content, content_span, generic)) =
                self.find_rune_with_generic(start_pos, "$state<")
            {
                return Some(RuneMatch {
                    kind: RuneKind::State,
                    full_span: Span::new(
                        self.base_offset + start_pos as u32,
                        self.base_offset + full_end as u32,
                    ),
                    content,
                    content_span,
                    pattern_len: 7, // "$state<"
                    generic: Some(generic),
                });
            }
        }

        // Check $derived<Type>() - transforms to the content
        if remaining.starts_with("$derived<") {
            if let Some((full_end, content, content_span, generic)) =
                self.find_rune_with_generic(start_pos, "$derived<")
            {
                return Some(RuneMatch {
                    kind: RuneKind::Derived,
                    full_span: Span::new(
                        self.base_offset + start_pos as u32,
                        self.base_offset + full_end as u32,
                    ),
                    content,
                    content_span,
                    pattern_len: 9, // "$derived<"
                    generic: Some(generic),
                });
            }
        }

        // Check $derived.by<Type>() - transforms to (fn)()
        if remaining.starts_with("$derived.by<") {
            if let Some((full_end, content, content_span, generic)) =
                self.find_rune_with_generic(start_pos, "$derived.by<")
            {
                return Some(RuneMatch {
                    kind: RuneKind::DerivedBy,
                    full_span: Span::new(
                        self.base_offset + start_pos as u32,
                        self.base_offset + full_end as u32,
                    ),
                    content,
                    content_span,
                    pattern_len: 12, // "$derived.by<"
                    generic: Some(generic),
                });
            }
        }

        // Check $props<Type>() - special handling for props
        if remaining.starts_with("$props<") {
            if let Some((full_end, _content, _content_span, generic)) =
                self.find_rune_with_generic(start_pos, "$props<")
            {
                return Some(RuneMatch {
                    kind: RuneKind::Props,
                    full_span: Span::new(
                        self.base_offset + start_pos as u32,
                        self.base_offset + full_end as u32,
                    ),
                    content: None,
                    content_span: None,
                    pattern_len: 7, // "$props<"
                    generic: Some(generic),
                });
            }
        }

        for (pattern, kind) in patterns {
            if remaining.starts_with(pattern) {
                // Special case for $host() which has no arguments
                if *kind == RuneKind::Host {
                    return Some(RuneMatch {
                        kind: *kind,
                        full_span: Span::new(
                            self.base_offset + start_pos as u32,
                            self.base_offset + start_pos as u32 + pattern.len() as u32,
                        ),
                        content: None,
                        content_span: None,
                        pattern_len: pattern.len(),
                        generic: None,
                    });
                }

                // Skip past the pattern (already includes opening paren)
                let content_start = start_pos + pattern.len();

                // Find matching closing paren
                if let Some((content, content_end)) =
                    self.find_matching_paren(&self.source[content_start..])
                {
                    let full_end = content_start + content_end + 1; // +1 for closing paren
                    return Some(RuneMatch {
                        kind: *kind,
                        full_span: Span::new(
                            self.base_offset + start_pos as u32,
                            self.base_offset + full_end as u32,
                        ),
                        content: Some(content.to_string()),
                        content_span: Some(Span::new(
                            self.base_offset + content_start as u32,
                            self.base_offset + (content_start + content.len()) as u32,
                        )),
                        pattern_len: pattern.len(),
                        generic: None,
                    });
                }
            }
        }

        None
    }

    /// Find a rune with TypeScript generics like $state<Type>() or $state<Type>(init).
    /// Returns (full_end_position, content, content_span) if found.
    fn find_rune_with_generic(
        &self,
        start_pos: usize,
        prefix: &str,
    ) -> Option<(usize, Option<String>, Option<Span>, String)> {
        let s = &self.source[start_pos..];

        if !s.starts_with(prefix) {
            return None;
        }

        let prefix_len = prefix.len();

        // Find matching > for the generic type parameter
        let after_prefix = &s[prefix_len..];
        let mut depth = 1;
        let mut in_string = None;
        let mut prev_was_escape = false;
        let mut generic_end = None;
        let mut prev_char = None;

        for (i, ch) in after_prefix.char_indices() {
            if prev_was_escape {
                prev_was_escape = false;
                prev_char = Some(ch);
                continue;
            }

            match in_string {
                Some(quote) => {
                    if ch == '\\' {
                        prev_was_escape = true;
                    } else if ch == quote {
                        in_string = None;
                    }
                }
                None => match ch {
                    '\'' | '"' | '`' => in_string = Some(ch),
                    '<' => depth += 1,
                    '>' => {
                        // Skip `>` if it's part of `=>` (arrow function in type)
                        if prev_char == Some('=') {
                            // This is `=>`, not a closing angle bracket
                        } else {
                            depth -= 1;
                            if depth == 0 {
                                generic_end = Some(i);
                                break;
                            }
                        }
                    }
                    _ => {}
                },
            }
            prev_char = Some(ch);
        }

        let generic_end = generic_end?;
        let generic = after_prefix[..generic_end].to_string();

        // After the >, we need "(" followed by optional content and ")"
        let after_generic = &after_prefix[generic_end + 1..];
        if !after_generic.starts_with('(') {
            return None;
        }

        // Find matching closing paren
        let paren_content_start = generic_end + 2; // +1 for >, +1 for (
        let after_open_paren = &after_prefix[paren_content_start..];

        if let Some((content, content_end_offset)) = self.find_matching_paren(after_open_paren) {
            let full_end = start_pos + prefix_len + paren_content_start + content_end_offset + 1;
            let content_str = if content.is_empty() {
                None
            } else {
                Some(content.to_string())
            };
            let content_span = if content.is_empty() {
                None
            } else {
                let content_start = start_pos + prefix_len + paren_content_start;
                Some(Span::new(
                    self.base_offset + content_start as u32,
                    self.base_offset + (content_start + content.len()) as u32,
                ))
            };
            Some((full_end, content_str, content_span, generic))
        } else {
            None
        }
    }

    /// Find the matching closing parenthesis, handling nested parens, strings, etc.
    fn find_matching_paren<'b>(&self, s: &'b str) -> Option<(&'b str, usize)> {
        let mut depth = 1;
        let mut in_string = None; // None, Some('\''), Some('"'), Some('`')
        let mut prev_was_escape = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;

        let mut chars = s.char_indices().peekable();
        while let Some((i, ch)) = chars.next() {
            if in_line_comment {
                if ch == '\n' {
                    in_line_comment = false;
                }
                continue;
            }

            if in_block_comment {
                if ch == '*' {
                    if let Some((_, '/')) = chars.peek().copied() {
                        chars.next();
                        in_block_comment = false;
                    }
                }
                continue;
            }

            if prev_was_escape {
                prev_was_escape = false;
                continue;
            }

            if let Some(quote) = in_string {
                if ch == '\\' {
                    prev_was_escape = true;
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            }

            match ch {
                '/' => {
                    if let Some((_, next)) = chars.peek().copied() {
                        match next {
                            '/' => {
                                chars.next();
                                in_line_comment = true;
                                continue;
                            }
                            '*' => {
                                chars.next();
                                in_block_comment = true;
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                '\'' | '"' | '`' => {
                    in_string = Some(ch);
                    continue;
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((&s[..i], i));
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Apply the transformation for a matched rune.
    fn apply_rune_transform(&mut self, rune_match: RuneMatch) {
        // Advance the chars iterator past the rune
        // We need to skip characters until we reach the end byte position
        // Note: rune_match.full_span uses byte offsets, but chars iterator is char-based
        let end_byte_offset = (u32::from(rune_match.full_span.end) - self.base_offset) as usize;

        // Skip characters until we've passed the rune's end position
        // We already consumed '$', so continue from current position
        while let Some((next_pos, _)) = self.chars.peek() {
            if *next_pos >= end_byte_offset {
                break;
            }
            self.chars.next();
        }

        let gen_start = self.output_pos;

        match rune_match.kind {
            RuneKind::State => {
                // $state(init) → $state(init) - preserve rune call
                // TypeScript uses ambient declaration: $state<T>(initial: T): T
                if let Some(generic) = rune_match.generic.as_deref() {
                    self.push_str("$state<");
                    self.push_str(generic);
                    self.push_str(">(");
                } else {
                    self.push_str("$state(");
                }
                if let Some(content) = &rune_match.content {
                    let (body, _trailing) = Self::strip_trailing_comma(content);
                    self.push_str(body);
                }
                self.push_char(')');
            }
            RuneKind::StateRaw => {
                // $state.raw(init) → $state.raw(init) - preserve rune call
                if let Some(generic) = rune_match.generic.as_deref() {
                    self.push_str("$state.raw<");
                    self.push_str(generic);
                    self.push_str(">(");
                } else {
                    self.push_str("$state.raw(");
                }
                if let Some(content) = &rune_match.content {
                    let (body, _trailing) = Self::strip_trailing_comma(content);
                    self.push_str(body);
                }
                self.push_char(')');
            }
            RuneKind::StateSnapshot => {
                // $state.snapshot(x) → $state.snapshot(x) - preserve rune call
                self.push_str("$state.snapshot(");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_char(')');
            }
            RuneKind::Derived => {
                // $derived(expr) → $derived(expr) - preserve rune call
                // TypeScript uses ambient declaration: $derived<T>(expression: T): T
                if let Some(generic) = rune_match.generic.as_deref() {
                    self.push_str("$derived<");
                    self.push_str(generic);
                    self.push_str(">(");
                } else {
                    self.push_str("$derived(");
                }
                if let Some(content) = &rune_match.content {
                    let (body, _trailing) = Self::strip_trailing_comma(content);
                    self.push_str(body);
                }
                self.push_char(')');
            }
            RuneKind::DerivedBy => {
                // $derived.by(fn) → $derived.by(fn) - preserve rune call
                // TypeScript uses ambient declaration: $derived.by<T>(fn: () => T): T
                if let Some(generic) = rune_match.generic.as_deref() {
                    self.push_str("$derived.by<");
                    self.push_str(generic);
                    self.push_str(">(");
                } else {
                    self.push_str("$derived.by(");
                }
                if let Some(content) = &rune_match.content {
                    let (body, _trailing) = Self::strip_trailing_comma(content);
                    self.push_str(body);
                }
                self.push_char(')');
            }
            RuneKind::Effect => {
                // $effect(fn) → $effect(fn) - preserve rune call
                self.push_str("$effect(");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_str(")");
            }
            RuneKind::EffectPre => {
                // $effect.pre(fn) → $effect.pre(fn) - preserve rune call
                self.push_str("$effect.pre(");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_str(")");
            }
            RuneKind::EffectRoot => {
                // $effect.root(fn) → $effect.root(fn) - preserve rune call
                self.push_str("$effect.root(");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_str(")");
            }
            RuneKind::Bindable => {
                // $bindable(default?) → $bindable(default?) - preserve rune call
                // TypeScript uses ambient declaration: $bindable<T>(fallback?: T): T
                self.push_str("$bindable(");
                if let Some(content) = &rune_match.content {
                    let (body, _trailing) = Self::strip_trailing_comma(content);
                    self.push_str(body);
                }
                self.push_char(')');
            }
            RuneKind::Inspect => {
                // $inspect(...) → $inspect(...) - preserve rune call
                self.push_str("$inspect(");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_char(')');
            }
            RuneKind::Host => {
                // $host() → $host() - preserve rune call
                self.push_str("$host()");
            }
            RuneKind::Props => {
                // Transform $props() to valid TypeScript
                // $props<Type>() -> ({} as __SvelteLoosen<Type>)
                // let {...}: Type = $props() -> ({} as __SvelteLoosen<Type>) - extract type from LHS annotation
                // $props() in SvelteKit route -> ({} as __SvelteLoosen<PageData>) or __SvelteLoosen<LayoutData>
                // $props() -> ({} as __SvelteLoosen<Record<string, unknown>>) - fallback
                let fallback_type = self.default_props_type.unwrap_or("Record<string, unknown>");

                if let Some(generic) = rune_match.generic.as_deref() {
                    self.push_str("({} as __SvelteLoosen<");
                    self.push_str(generic);
                    self.push_str(">)");
                } else {
                    // $props() without generics - try to extract type from LHS
                    let type_from_lhs = self.extract_type_from_lhs();
                    if let Some(type_str) = type_from_lhs {
                        self.push_str("({} as __SvelteLoosen<");
                        self.push_str(&type_str);
                        self.push_str(">)");
                    } else {
                        self.push_str("({} as __SvelteLoosen<");
                        self.push_str(fallback_type);
                        self.push_str(">)");
                    }
                }
            }
        }

        let gen_end = self.output_pos;

        // Record the mapping
        self.mappings.push(RuneMapping {
            original: rune_match.full_span,
            generated: Span::new(gen_start, gen_end),
        });

        // Record rune info
        self.runes.push(RuneInfo {
            kind: rune_match.kind,
            variable_name: None, // Would need more context to determine
            original_span: rune_match.full_span,
            content_span: rune_match.content_span,
        });
    }

    /// Extract type annotation from the LHS of a $props() assignment.
    ///
    /// Looks for patterns like `}: Type = ` at the end of the output buffer,
    /// where Type is the type annotation we want to use.
    ///
    /// Examples:
    /// - `let { a, b }: Props = ` -> Some("Props")
    /// - `let { a }: { a: string; b: number } = ` -> Some("{ a: string; b: number }")
    fn extract_type_from_lhs(&self) -> Option<String> {
        // Look back in the output for `: Type = ` pattern
        // The output currently ends with everything up to $props()
        let output = &self.output;

        // Find the last assignment `=` which is right before $props()
        let bytes = output.as_bytes();
        let mut equals_pos = None;
        for i in (0..bytes.len()).rev() {
            if bytes[i] != b'=' {
                continue;
            }

            let prev = if i > 0 { bytes[i - 1] } else { 0 };
            let next = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };

            // Skip arrows and comparisons (=>, ==, ===, !=, <=, >=)
            if next == b'=' || next == b'>' {
                continue;
            }
            if prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>' {
                continue;
            }

            equals_pos = Some(i);
            break;
        }

        let equals_pos = equals_pos?;

        // Now look backwards from the equals sign to find `: Type`
        // We need to find the closing `}` of destructuring, then the `: Type` after it
        let before_equals = &output[..equals_pos];
        let comment_mask = build_comment_mask(before_equals);

        // Find the colon that starts the type annotation.
        // We only accept a `:` at top-level (not inside braces/paren/brackets/strings).
        // When iterating backwards, we need to track string context properly.
        let mut type_start = None;
        let mut brace_depth = 0;
        let mut paren_depth = 0;
        let mut bracket_depth = 0;
        let mut in_string: Option<char> = None;

        // Iterate backwards through characters
        let chars: Vec<(usize, char)> = before_equals.char_indices().collect();
        let mut i = chars.len();
        while i > 0 {
            i -= 1;
            let (idx, ch) = chars[i];
            if comment_mask.get(idx).copied().unwrap_or(false) {
                continue;
            }

            // When iterating backwards through strings, we need to detect string boundaries
            // and skip their contents. A quote at position i closes a string (when going backwards),
            // and we need to find the opening quote.
            if in_string.is_some() {
                // We're inside a string (going backwards), look for the opening quote
                if let Some(quote) = in_string {
                    if ch == quote {
                        // Check if this quote is escaped
                        let mut escape_count = 0;
                        let mut j = i;
                        while j > 0 && chars[j - 1].1 == '\\' {
                            escape_count += 1;
                            j -= 1;
                        }
                        // If even number of escapes (including 0), this is the opening quote
                        if escape_count % 2 == 0 {
                            in_string = None;
                        }
                    }
                }
                continue;
            }

            // Check if we're entering a string (going backwards means we hit a closing quote)
            if ch == '\'' || ch == '"' || ch == '`' {
                in_string = Some(ch);
                continue;
            }

            // Handle regex literals: when we see '/' going backwards, check if it's a regex
            // A closing '/' of a regex is typically followed by flags (gimsuvy) or operators
            // We simplify by treating any '/' followed by a valid regex flag as potentially a regex
            if ch == '/' {
                // Check if this could be a regex literal (going backwards)
                // Look for the opening '/' by scanning backwards
                let mut regex_start = None;
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    let (prev_idx, prev_ch) = chars[j];
                    if comment_mask.get(prev_idx).copied().unwrap_or(false) {
                        continue;
                    }
                    if prev_ch == '/' {
                        // Check if it's escaped
                        let mut escape_count = 0;
                        let mut k = j;
                        while k > 0 && chars[k - 1].1 == '\\' {
                            escape_count += 1;
                            k -= 1;
                        }
                        if escape_count % 2 == 0 {
                            regex_start = Some(j);
                            break;
                        }
                    }
                    // If we hit a newline, it's not a regex
                    if prev_ch == '\n' {
                        break;
                    }
                }
                if let Some(start) = regex_start {
                    // Skip past this regex literal
                    i = start;
                    continue;
                }
            }

            match ch {
                '}' => brace_depth += 1,
                '{' => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
                ')' => paren_depth += 1,
                '(' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                }
                ']' => bracket_depth += 1,
                '[' => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                    }
                }
                ':' => {
                    if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 {
                        let mut j = idx;
                        while j > 0 {
                            let prev_idx = j - 1;
                            if comment_mask.get(prev_idx).copied().unwrap_or(false) {
                                j -= 1;
                                continue;
                            }
                            let prev = before_equals.as_bytes()[prev_idx];
                            if !prev.is_ascii_whitespace() {
                                let is_ident = (prev as char).is_ascii_alphanumeric()
                                    || prev == b'_'
                                    || prev == b'$';
                                if prev == b'}' || prev == b')' || prev == b']' || is_ident {
                                    type_start = Some(idx);
                                }
                                break;
                            }
                            j -= 1;
                        }
                    }
                }
                _ => {}
            }

            if type_start.is_some() {
                break;
            }
        }

        let type_start = type_start?;
        let type_str = &before_equals[type_start + 1..].trim();

        if type_str.is_empty() {
            return None;
        }

        // Validate that this looks like a type (not empty, starts reasonably)
        let first_char = type_str.chars().next()?;
        if !first_char.is_alphabetic()
            && first_char != '{'
            && first_char != '('
            && first_char != '['
        {
            return None;
        }

        Some(type_str.to_string())
    }
}

fn build_comment_mask(source: &str) -> Vec<bool> {
    let mut mask = vec![false; source.len()];
    let mut chars = source.char_indices().peekable();
    let mut in_string: Option<char> = None;
    let mut prev_was_escape = false;
    let mut template_brace_depth: Vec<usize> = Vec::new();
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some((idx, ch)) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            } else {
                mark_comment_char(&mut mask, idx, ch);
            }
            continue;
        }

        if in_block_comment {
            mark_comment_char(&mut mask, idx, ch);
            if ch == '*' && chars.peek().map(|(_, next)| *next) == Some('/') {
                let (next_idx, next_ch) = chars.next().unwrap();
                mark_comment_char(&mut mask, next_idx, next_ch);
                in_block_comment = false;
            }
            continue;
        }

        if let Some(quote) = in_string {
            if quote != '`' {
                if prev_was_escape {
                    prev_was_escape = false;
                } else if ch == '\\' {
                    prev_was_escape = true;
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            } else {
                if prev_was_escape {
                    prev_was_escape = false;
                    continue;
                }
                if ch == '\\' {
                    prev_was_escape = true;
                    continue;
                }
                if ch == '`' {
                    in_string = None;
                    continue;
                }
                if ch == '$' && chars.peek().map(|(_, next)| *next) == Some('{') {
                    chars.next();
                    template_brace_depth.push(0);
                    in_string = None;
                    continue;
                }
                continue;
            }
        }

        if !template_brace_depth.is_empty() {
            if ch == '{' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    *depth += 1;
                }
            } else if ch == '}' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    if *depth == 0 {
                        template_brace_depth.pop();
                        in_string = Some('`');
                        continue;
                    } else {
                        *depth -= 1;
                    }
                }
            }
        }

        if ch == '/' {
            if chars.peek().map(|(_, next)| *next) == Some('/') {
                mark_comment_char(&mut mask, idx, ch);
                let (next_idx, next_ch) = chars.next().unwrap();
                mark_comment_char(&mut mask, next_idx, next_ch);
                in_line_comment = true;
                continue;
            } else if chars.peek().map(|(_, next)| *next) == Some('*') {
                mark_comment_char(&mut mask, idx, ch);
                let (next_idx, next_ch) = chars.next().unwrap();
                mark_comment_char(&mut mask, next_idx, next_ch);
                in_block_comment = true;
                continue;
            }
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            in_string = Some(ch);
            prev_was_escape = false;
            continue;
        }
    }

    mask
}

fn mark_comment_char(mask: &mut [bool], idx: usize, ch: char) {
    let len = ch.len_utf8();
    for i in idx..idx + len {
        if let Some(slot) = mask.get_mut(i) {
            *slot = true;
        }
    }
}

/// Information about a matched rune.
struct RuneMatch {
    kind: RuneKind,
    /// Span of the entire rune call including parentheses.
    full_span: Span,
    /// The content inside the parentheses.
    content: Option<String>,
    /// Span of just the content.
    content_span: Option<Span>,
    /// Length of the pattern (e.g., "$state(" is 7).
    #[allow(dead_code)]
    pattern_len: usize,
    /// The generic type argument, if provided (e.g. `$state<Type>()`).
    generic: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_state() {
        let result = transform_runes("let count = $state(0);", 0);
        assert_eq!(result.output, "let count = $state(0);");
        assert_eq!(result.runes.len(), 1);
        assert_eq!(result.runes[0].kind, RuneKind::State);
    }

    #[test]
    fn test_transform_state_with_object() {
        let result = transform_runes("let obj = $state({ a: 1 });", 0);
        assert_eq!(result.output, "let obj = $state({ a: 1 });");
    }

    #[test]
    fn test_transform_state_raw() {
        let result = transform_runes("let arr = $state.raw([1, 2, 3]);", 0);
        assert_eq!(result.output, "let arr = $state.raw([1, 2, 3]);");
        assert_eq!(result.runes[0].kind, RuneKind::StateRaw);
    }

    #[test]
    fn test_transform_state_snapshot() {
        let result = transform_runes("const snap = $state.snapshot(obj);", 0);
        assert_eq!(result.output, "const snap = $state.snapshot(obj);");
        assert_eq!(result.runes[0].kind, RuneKind::StateSnapshot);
    }

    #[test]
    fn test_transform_derived() {
        let result = transform_runes("let double = $derived(count * 2);", 0);
        assert_eq!(result.output, "let double = $derived(count * 2);");
        assert_eq!(result.runes[0].kind, RuneKind::Derived);
    }

    #[test]
    fn test_transform_derived_by() {
        let result = transform_runes("let value = $derived.by(() => compute());", 0);
        assert_eq!(result.output, "let value = $derived.by(() => compute());");
        assert_eq!(result.runes[0].kind, RuneKind::DerivedBy);
    }

    #[test]
    fn test_transform_derived_by_generic() {
        let result = transform_runes("let value = $derived.by<number>(() => compute());", 0);
        assert_eq!(
            result.output,
            "let value = $derived.by<number>(() => compute());"
        );
        assert_eq!(result.runes[0].kind, RuneKind::DerivedBy);
    }

    #[test]
    fn test_transform_derived_by_multiline() {
        let input = r#"let total = $derived.by(() => {
        let sum = 0;
        for (const n of numbers) {
            sum += n;
        }
        return sum;
    });"#;
        let result = transform_runes(input, 0);
        // Rune is preserved
        assert!(result.output.starts_with("let total = $derived.by(() => {"));
        assert!(result.output.ends_with("});"));
        assert!(result.output.contains("$derived.by"));
        assert_eq!(result.runes[0].kind, RuneKind::DerivedBy);
    }

    #[test]
    fn test_transform_effect() {
        let result = transform_runes("$effect(() => console.log(count));", 0);
        assert_eq!(result.output, "$effect(() => console.log(count));");
        assert_eq!(result.runes[0].kind, RuneKind::Effect);
    }

    #[test]
    fn test_transform_effect_pre() {
        let result = transform_runes("$effect.pre(() => setup());", 0);
        assert_eq!(result.output, "$effect.pre(() => setup());");
        assert_eq!(result.runes[0].kind, RuneKind::EffectPre);
    }

    #[test]
    fn test_transform_effect_root() {
        let result = transform_runes("const cleanup = $effect.root(() => {});", 0);
        assert_eq!(result.output, "const cleanup = $effect.root(() => {});");
        assert_eq!(result.runes[0].kind, RuneKind::EffectRoot);
    }

    #[test]
    fn test_transform_host() {
        let result = transform_runes("$host().dispatchEvent(event);", 0);
        assert_eq!(result.output, "$host().dispatchEvent(event);");
        assert_eq!(result.runes[0].kind, RuneKind::Host);
    }

    #[test]
    fn test_transform_inspect() {
        let result = transform_runes("$inspect(value);", 0);
        assert_eq!(result.output, "$inspect(value);");
        assert_eq!(result.runes[0].kind, RuneKind::Inspect);
    }

    #[test]
    fn test_transform_bindable_with_default() {
        let result = transform_runes("let { value = $bindable(0) } = $props();", 0);
        assert_eq!(
            result.output,
            "let { value = $bindable(0) } = ({} as __SvelteLoosen<Record<string, unknown>>);"
        );
        assert_eq!(result.runes.len(), 2);
        assert_eq!(result.runes[0].kind, RuneKind::Bindable);
        assert_eq!(result.runes[1].kind, RuneKind::Props);
    }

    #[test]
    fn test_transform_bindable_empty() {
        let result = transform_runes("let { value = $bindable() } = $props();", 0);
        assert_eq!(
            result.output,
            "let { value = $bindable() } = ({} as __SvelteLoosen<Record<string, unknown>>);"
        );
    }

    #[test]
    fn test_props_transformed() {
        let result = transform_runes("let { a, b } = $props();", 0);
        assert_eq!(
            result.output,
            "let { a, b } = ({} as __SvelteLoosen<Record<string, unknown>>);"
        );
        assert_eq!(result.runes[0].kind, RuneKind::Props);
    }

    #[test]
    fn test_props_with_typescript_generics() {
        let result = transform_runes("let { name } = $props<{ name: string }>();", 0);
        assert_eq!(
            result.output,
            "let { name } = ({} as __SvelteLoosen<{ name: string }>);"
        );
        assert_eq!(result.runes.len(), 1);
        assert_eq!(result.runes[0].kind, RuneKind::Props);
    }

    #[test]
    fn test_props_with_complex_generic() {
        let result = transform_runes(
            "let { data } = $props<{ data: Array<{ id: number }> }>();",
            0,
        );
        assert_eq!(
            result.output,
            "let { data } = ({} as __SvelteLoosen<{ data: Array<{ id: number }> }>);"
        );
        assert_eq!(result.runes[0].kind, RuneKind::Props);
    }

    #[test]
    fn test_props_with_arrow_function_type() {
        let result = transform_runes("let { onClick } = $props<{ onClick?: () => void }>();", 0);
        assert_eq!(
            result.output,
            "let { onClick } = ({} as __SvelteLoosen<{ onClick?: () => void }>);"
        );
        assert_eq!(result.runes.len(), 1);
        assert_eq!(result.runes[0].kind, RuneKind::Props);
    }

    #[test]
    fn test_props_with_multiline_arrow_function() {
        let result = transform_runes(
            r#"let { name, onchange } = $props<{
    name: string;
    onchange?: (n: number) => void;
}>();"#,
            0,
        );
        assert_eq!(
            result.output,
            r#"let { name, onchange } = ({} as __SvelteLoosen<{
    name: string;
    onchange?: (n: number) => void;
}>);"#
        );
    }

    #[test]
    fn test_no_transform_in_string() {
        let result = transform_runes(r#"let x = "$state(0)";"#, 0);
        assert_eq!(result.output, r#"let x = "$state(0)";"#);
        assert!(result.runes.is_empty());
    }

    #[test]
    fn test_no_transform_in_template_literal() {
        let result = transform_runes(r#"let x = `$state(0)`;"#, 0);
        assert_eq!(result.output, r#"let x = `$state(0)`;"#);
        assert!(result.runes.is_empty());
    }

    #[test]
    fn test_no_transform_in_single_quote_string() {
        let result = transform_runes(r#"let x = '$state(0)';"#, 0);
        assert_eq!(result.output, r#"let x = '$state(0)';"#);
        assert!(result.runes.is_empty());
    }

    #[test]
    fn test_no_transform_in_line_comment() {
        let result = transform_runes("// $state(0)\nlet x = 1;", 0);
        assert_eq!(result.output, "// $state(0)\nlet x = 1;");
        assert!(result.runes.is_empty());
    }

    #[test]
    fn test_no_transform_in_block_comment() {
        let result = transform_runes("/* $state(0) */let x = 1;", 0);
        assert_eq!(result.output, "/* $state(0) */let x = 1;");
        assert!(result.runes.is_empty());
    }

    #[test]
    fn test_nested_parens() {
        let result = transform_runes("let x = $state(fn(a, b));", 0);
        assert_eq!(result.output, "let x = $state(fn(a, b));");
    }

    #[test]
    fn test_multiple_runes() {
        let result = transform_runes(
            "let count = $state(0); let doubled = $derived(count * 2);",
            0,
        );
        assert_eq!(
            result.output,
            "let count = $state(0); let doubled = $derived(count * 2);"
        );
        assert_eq!(result.runes.len(), 2);
    }

    #[test]
    fn test_mappings_tracked() {
        let result = transform_runes("let x = $state(0);", 0);
        assert_eq!(result.mappings.len(), 1);
        // Original: "$state(0)" at position 8-17
        assert_eq!(u32::from(result.mappings[0].original.start), 8);
        assert_eq!(u32::from(result.mappings[0].original.end), 17);
        // Generated: "$state(0)" at position 8-17 (preserved)
        assert_eq!(u32::from(result.mappings[0].generated.start), 8);
        assert_eq!(u32::from(result.mappings[0].generated.end), 17);
    }

    #[test]
    fn test_with_base_offset() {
        let result = transform_runes("let x = $state(0);", 100);
        // Spans should include base offset
        assert_eq!(u32::from(result.mappings[0].original.start), 108);
        assert_eq!(u32::from(result.mappings[0].original.end), 117);
    }

    #[test]
    fn test_template_expression_with_rune() {
        // Runes inside template expressions are preserved
        let result = transform_runes("let x = `value: ${$state(0)}`;", 0);
        assert_eq!(result.output, "let x = `value: ${$state(0)}`;");
        assert_eq!(result.runes.len(), 1);
    }

    #[test]
    fn test_escaped_string() {
        let result = transform_runes(r#"let x = "foo\"$state(0)\"bar";"#, 0);
        assert_eq!(result.output, r#"let x = "foo\"$state(0)\"bar";"#);
        assert!(result.runes.is_empty());
    }

    #[test]
    fn test_store_subscription_in_function() {
        let result = transform_runes(
            r#"function setBillRateMethod() {
    // If the custom bill rate is selected, set its method based on the shift method.
    if (isCustomBillRate)
      $formData.billRate.method = 'hourly';
  }"#,
            0,
        );
        assert!(
            result.output.contains("$formData"),
            "Expected $formData to remain but got:\n{}",
            result.output
        );
        assert!(result.store_names.contains("formData"));
    }

    #[test]
    fn test_store_after_derived() {
        // This tests the case where we have:
        // 1. A $derived() expression (which gets transformed)
        // 2. Followed by a regular function with $store access
        let result = transform_runes(
            r#"let x = $derived($formData.value);

function test() {
  $formData.other = 'value';
}"#,
            0,
        );
        assert!(
            result.output.contains("$formData"),
            "Expected $formData to remain but got:\n{}",
            result.output
        );
        assert!(result.store_names.contains("formData"));
    }

    #[test]
    fn test_store_subscription_assignment() {
        // Test store assignment in a function
        let result = transform_runes(
            r#"const { form: formData, enhance, errors } = form;

function updateEndTime() {
  $formData.endTime = $formData.startTime ? 'test' : null;
}"#,
            0,
        );
        assert!(
            result.output.contains("$formData"),
            "Expected $formData to remain but got:\n{}",
            result.output
        );
        assert!(result.store_names.contains("formData"));
    }

    #[test]
    fn test_store_subscription_with_comment() {
        // Test store assignment after a comment (matches the real file pattern)
        let result = transform_runes(
            r#"function setBillRateMethod() {
    // If the custom bill rate is selected, set its method based on the shift method.
    if (isCustomBillRate)
      $formData.billRate.method =
        $formData.method === 'hourly' ? 'hourly'
        : $formData.method === 'liveIn' ? 'daily'
        : 'flat';
  }"#,
            0,
        );
        assert!(
            result.output.contains("$formData"),
            "Expected $formData to remain but got:\n{}",
            result.output
        );
        assert!(result.store_names.contains("formData"));
    }

    #[test]
    fn test_props_with_colon_in_import() {
        // Test that colons inside import strings don't interfere with $props() transformation
        // This was issue #21 - imports like 'virtual:something' caused parsing errors
        let result = transform_runes(
            r#"import 'virtual:something';

let { children } = $props();"#,
            0,
        );
        // The output should NOT contain the import string as part of the type
        assert!(
            !result.output.contains("__SvelteLoosen<something"),
            "Colon in import string was incorrectly parsed as type annotation: {}",
            result.output
        );
        // Should use the default fallback type
        assert!(
            result
                .output
                .contains("__SvelteLoosen<Record<string, unknown>>"),
            "Expected fallback type for $props() but got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_props_with_colon_in_block_comment() {
        let result = transform_runes(
            "let { children } = /* note: comment before props */ $props();",
            0,
        );
        assert_eq!(
            result.output,
            "let { children } = /* note: comment before props */ ({} as __SvelteLoosen<Record<string, unknown>>);"
        );
    }

    #[test]
    fn test_props_with_trailing_inline_comment() {
        let result = transform_runes(
            "let { children } = $props(); // trailing: comment after props",
            0,
        );
        assert_eq!(
            result.output,
            "let { children } = ({} as __SvelteLoosen<Record<string, unknown>>); // trailing: comment after props"
        );
    }

    #[test]
    fn test_props_with_various_string_patterns() {
        // Test various string patterns with colons that should be skipped
        let result = transform_runes(
            r#"const url = "http://example.com";
const regex = /foo:bar/;
let { data } = $props();"#,
            0,
        );
        assert!(
            result
                .output
                .contains("__SvelteLoosen<Record<string, unknown>>"),
            "Expected fallback type but got:\n{}",
            result.output
        );
    }

    // =========================================================================
    // Issue #35: Multiline $state<T>(value) with trailing commas
    // =========================================================================
    // These tests verify that multiline rune expressions with trailing commas
    // are correctly handled. Trailing commas are stripped from the content to
    // produce valid TypeScript within the preserved rune call.

    #[test]
    fn test_state_multiline_with_trailing_comma() {
        // This is the exact pattern from issue #35
        let result = transform_runes(
            r#"eventOrder = $state<'status-start-title' | 'start-title' | 'title'>(
    'status-start-title',
);"#,
            0,
        );

        // The rune should be preserved with trailing comma stripped
        assert!(
            result.output.contains("$state<"),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        // No trailing comma before the closing paren
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_state_multiline_with_trailing_comma_and_whitespace() {
        // Test with extra whitespace around the trailing comma
        let result = transform_runes(
            r#"value = $state<number>(
    42   ,
);"#,
            0,
        );

        // Rune preserved, trailing comma stripped
        assert!(
            result.output.contains("$state<number>("),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_state_multiline_complex_type_with_trailing_comma() {
        // Test with complex union type including function type
        let result = transform_runes(
            r#"handler = $state<(() => void) | null>(
    null,
);"#,
            0,
        );

        assert!(
            result.output.contains("$state<(() => void) | null>("),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_state_multiline_object_literal_with_trailing_comma() {
        // Test with object literal that has its own trailing comma
        let result = transform_runes(
            r#"config = $state<{ enabled: boolean }>(
    { enabled: true, },
);"#,
            0,
        );

        // The inner trailing comma inside the object should be preserved
        assert!(
            result.output.contains("{ enabled: true, }"),
            "Inner object comma should be preserved. Got:\n{}",
            result.output
        );
        // But no trailing comma after the object
        assert!(
            !result.output.ends_with(",);"),
            "Outer trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_state_single_line_no_trailing_comma() {
        // Ensure single-line without trailing comma still works
        let result = transform_runes("let x = $state<number>(42);", 0);
        assert_eq!(result.output, "let x = $state<number>(42);");
    }

    #[test]
    fn test_state_single_line_with_trailing_comma() {
        // Single line with trailing comma should also work
        let result = transform_runes("let x = $state<number>(42,);", 0);

        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
        assert!(
            result.output.contains("$state<number>(42)"),
            "Rune should be preserved with stripped comma. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_derived_by_multiline_with_trailing_comma() {
        // Test $derived.by with multiline and trailing comma
        let result = transform_runes(
            r#"doubled = $derived.by<number>(
    () => count * 2,
);"#,
            0,
        );

        assert!(
            result.output.contains("$derived.by<number>("),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_derived_multiline_with_trailing_comma() {
        // Test $derived with multiline and trailing comma
        let result = transform_runes(
            r#"doubled = $derived<number>(
    count * 2,
);"#,
            0,
        );

        assert!(
            result.output.contains("$derived<number>("),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_derived_complex_type_multiline_with_trailing_comma() {
        // Test $derived with complex union type and trailing comma
        let result = transform_runes(
            r#"value = $derived<string | number>(
    someCondition ? 'text' : 42,
);"#,
            0,
        );

        assert!(
            result.output.contains("$derived<string | number>("),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_bindable_multiline_with_trailing_comma() {
        // Test $bindable with multiline and trailing comma
        let result = transform_runes(
            r#"let { value = $bindable(
    42,
) } = $props();"#,
            0,
        );

        // Bindable rune should be preserved with stripped trailing comma
        assert!(
            result.output.contains("$bindable("),
            "Bindable rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains("42,"),
            "Trailing comma should be stripped from bindable. Got:\n{}",
            result.output
        );
    }

    // =========================================================================
    // $state.raw<T>() generic support tests
    // =========================================================================

    #[test]
    fn test_state_raw_with_generic() {
        let result = transform_runes("let items = $state.raw<number[]>([1, 2, 3]);", 0);
        assert_eq!(
            result.output,
            "let items = $state.raw<number[]>([1, 2, 3]);"
        );
        assert_eq!(result.runes[0].kind, RuneKind::StateRaw);
    }

    #[test]
    fn test_state_raw_with_generic_empty() {
        let result = transform_runes("let items = $state.raw<string[]>([]);", 0);
        assert_eq!(result.output, "let items = $state.raw<string[]>([]);");
    }

    #[test]
    fn test_state_raw_multiline_with_trailing_comma() {
        let result = transform_runes(
            r#"let items = $state.raw<number[]>(
    [1, 2, 3],
);"#,
            0,
        );

        assert!(
            result.output.contains("$state.raw<number[]>("),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
        assert!(
            !result.output.contains(",)"),
            "Trailing comma should be stripped. Got:\n{}",
            result.output
        );
    }

    #[test]
    fn test_state_raw_with_complex_generic_type() {
        let result = transform_runes(
            "let data = $state.raw<Map<string, { id: number; name: string }>>(new Map());",
            0,
        );
        assert!(
            result
                .output
                .contains("$state.raw<Map<string, { id: number; name: string }>>(new Map())"),
            "Rune should be preserved. Got:\n{}",
            result.output
        );
    }
}
