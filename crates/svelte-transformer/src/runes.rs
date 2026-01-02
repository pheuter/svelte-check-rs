//! Rune transformation logic.
//!
//! Transforms Svelte 5 runes into their TypeScript equivalents for type-checking.
//! This implementation uses a token-aware scanner that properly handles strings,
//! comments, and template literals to avoid false transformations.

use smol_str::SmolStr;
use source_map::Span;

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

/// Transform store subscriptions in an expression.
///
/// In Svelte, `$storeName` is shorthand for subscribing to a store and getting its value.
/// We transform `$storeName` to `__svelte_store_get(storeName)` so TypeScript sees the
/// dereferenced value type, not the store type.
///
/// Special case: `typeof $store` becomes `__StoreValue<typeof store>` because
/// TypeScript's typeof operator doesn't work with function calls.
///
/// This only applies to store subscriptions (identifier after $), not to:
/// - Runes like `$state()`, `$derived()` (have parentheses)
/// - Special variables like `$$props`, `$$slots`
/// - Strings like `'$lib/assets/favicon.svg'`
fn transform_store_subscriptions(expr: &str) -> String {
    let mut result = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();
    // Track typeof context: whitespace accumulated after "typeof"
    let mut typeof_state: Option<String> = None;
    // Track when we've just emitted __StoreValue<typeof ...> and need indexed property access
    let mut in_storevalue_access = false;
    // Track string context: None = not in string, Some(quote) = in string with that quote
    let mut in_string: Option<char> = None;
    // Track if previous char was a backslash (for escape sequences)
    let mut prev_was_escape = false;
    // Track template literal expression depth for nested ${...} handling
    let mut template_brace_depth: Vec<usize> = Vec::new();
    // Track comment context
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        // Handle line comment context - pass through until newline
        if in_line_comment {
            result.push(ch);
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }

        // Handle block comment context - pass through until */
        if in_block_comment {
            result.push(ch);
            if ch == '*' && chars.peek() == Some(&'/') {
                result.push(chars.next().unwrap()); // consume '/'
                in_block_comment = false;
            }
            continue;
        }

        // Handle string context (regular strings, not template literals)
        if let Some(quote) = in_string {
            if quote != '`' {
                // Regular string - just pass through
                result.push(ch);
                if prev_was_escape {
                    prev_was_escape = false;
                } else if ch == '\\' {
                    prev_was_escape = true;
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            } else {
                // Template literal - need to handle ${...} expressions
                if prev_was_escape {
                    result.push(ch);
                    prev_was_escape = false;
                    continue;
                }
                if ch == '\\' {
                    result.push(ch);
                    prev_was_escape = true;
                    continue;
                }
                if ch == '`' {
                    result.push(ch);
                    in_string = None;
                    continue;
                }
                if ch == '$' && chars.peek() == Some(&'{') {
                    // Start of template expression - emit ${ and track depth
                    result.push(ch);
                    result.push(chars.next().unwrap()); // consume '{'
                    template_brace_depth.push(0);
                    in_string = None; // Exit string context to process expression
                    continue;
                }
                result.push(ch);
                continue;
            }
        }

        // Handle template expression brace depth
        if !template_brace_depth.is_empty() {
            if ch == '{' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    *depth += 1;
                }
            } else if ch == '}' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    if *depth == 0 {
                        // End of template expression, back to template literal
                        template_brace_depth.pop();
                        result.push(ch);
                        in_string = Some('`');
                        continue;
                    } else {
                        *depth -= 1;
                    }
                }
            }
        }

        // Check for comment start (outside of strings)
        if ch == '/' {
            if chars.peek() == Some(&'/') {
                // Line comment - pass through until newline
                result.push(ch);
                result.push(chars.next().unwrap()); // consume second '/'
                in_line_comment = true;
                continue;
            } else if chars.peek() == Some(&'*') {
                // Block comment - pass through until */
                result.push(ch);
                result.push(chars.next().unwrap()); // consume '*'
                in_block_comment = true;
                continue;
            }
        }

        // Check for string start (outside of strings)
        if ch == '\'' || ch == '"' || ch == '`' {
            result.push(ch);
            in_string = Some(ch);
            continue;
        }

        // When in storevalue access mode, convert .prop to ["prop"]
        if in_storevalue_access {
            if ch == '.' {
                // Check if followed by identifier
                if chars
                    .peek()
                    .is_some_and(|&c| c.is_ascii_alphabetic() || c == '_')
                {
                    // Collect the property name
                    let mut prop = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            prop.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Emit as indexed access
                    result.push_str("[\"");
                    result.push_str(&prop);
                    result.push_str("\"]");
                    // Stay in storevalue_access mode for chained access
                    continue;
                }
            }
            // Not a property access, exit the mode
            in_storevalue_access = false;
        }

        if ch == '$' {
            // Check if this is a store subscription
            if let Some(&next) = chars.peek() {
                // Skip $$ patterns ($$props, $$slots, etc.)
                if next == '$' {
                    // Restore typeof if we were tracking it
                    if let Some(ws) = typeof_state.take() {
                        result.push_str("typeof");
                        result.push_str(&ws);
                    }
                    result.push(ch);
                    continue;
                }

                // Check if followed by valid identifier start
                if next.is_ascii_alphabetic() || next == '_' {
                    // Collect the identifier
                    let mut identifier = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            identifier.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Peek ahead to see what follows
                    let rest: String = chars.clone().collect();
                    let trimmed_rest = rest.trim_start();
                    let next_non_ws = trimmed_rest.chars().next();

                    // If followed by ( or <, it's a rune - keep the $identifier
                    // If followed by :, it's an object property key (e.g., { $or: ... })
                    if next_non_ws == Some('(')
                        || next_non_ws == Some('<')
                        || next_non_ws == Some(':')
                    {
                        // Restore typeof if we were tracking it
                        if let Some(ws) = typeof_state.take() {
                            result.push_str("typeof");
                            result.push_str(&ws);
                        }
                        result.push(ch);
                        result.push_str(&identifier);
                    } else if next_non_ws == Some('=')
                        && !trimmed_rest.starts_with("==")
                        && !trimmed_rest.starts_with("=>")
                    {
                        // Store assignment: $store = value
                        // This is a Svelte store setter. For type-checking, we transform to:
                        // __svelte_store_get(store) which returns the value type.
                        // The assignment will fail type-checking if value is incompatible.
                        // We use an unsafe cast to allow the assignment:
                        // $store = value -> (__svelte_store_get(store) as typeof value) = value
                        // Actually, simpler: we'll emit the store reference and trust that
                        // the Svelte type definitions handle $store syntax.
                        // But they don't, so let's use a workaround:
                        // Transform to: (store as any).$ = value (creates a dummy prop)
                        // This allows the assignment to type-check while preserving intent.
                        // Actually even simpler for type-checking purposes:
                        // Just emit the raw store access - TypeScript will complain about
                        // unknown property but won't error on assignment LHS.
                        if let Some(ws) = typeof_state.take() {
                            result.push_str("typeof");
                            result.push_str(&ws);
                        }
                        // Emit as property access: (store as any).$
                        result.push('(');
                        result.push_str(&identifier);
                        result.push_str(" as any).$");
                    } else if typeof_state.is_some() {
                        // In typeof context, use type helper instead of function call
                        // typeof $store.prop -> __StoreValue<typeof store>["prop"]
                        // Don't restore typeof - we're replacing it
                        typeof_state = None;
                        result.push_str("__StoreValue<typeof ");
                        result.push_str(&identifier);
                        result.push('>');
                        // Enter storevalue access mode for property access conversion
                        in_storevalue_access = true;
                    } else {
                        // It's a store subscription - wrap with helper function
                        result.push_str("__svelte_store_get(");
                        result.push_str(&identifier);
                        result.push(')');
                    }
                } else {
                    // Restore typeof if we were tracking it
                    if let Some(ws) = typeof_state.take() {
                        result.push_str("typeof");
                        result.push_str(&ws);
                    }
                    result.push(ch);
                }
            } else {
                // Restore typeof if we were tracking it
                if let Some(ws) = typeof_state.take() {
                    result.push_str("typeof");
                    result.push_str(&ws);
                }
                result.push(ch);
            }
        } else {
            // Not a $ - check if we're tracking typeof
            if let Some(ref mut ws) = typeof_state {
                if ch.is_whitespace() {
                    // Accumulate whitespace after typeof
                    ws.push(ch);
                } else {
                    // Non-whitespace, non-$ after typeof - restore typeof and continue
                    result.push_str("typeof");
                    result.push_str(ws);
                    result.push(ch);
                    typeof_state = None;
                }
            } else {
                result.push(ch);
                // Check if we just completed the word "typeof"
                if result.ends_with("typeof") {
                    // Peek at next char to ensure it's whitespace (typeof is followed by space)
                    if chars.peek().is_some_and(|&c| c.is_whitespace()) {
                        // Remove the "typeof" we just added - we'll track it separately
                        result.truncate(result.len() - 6);
                        typeof_state = Some(String::new());
                    }
                }
            }
        }
    }

    // Handle case where expression ends with "typeof " (restore it)
    if let Some(ws) = typeof_state {
        result.push_str("typeof");
        result.push_str(&ws);
    }

    result
}

/// Transforms rune expressions in script content.
///
/// This function identifies rune calls and transforms them for type-checking:
/// - `$state(init)` → `init`
/// - `$state.raw(init)` → `init`
/// - `$state.snapshot(x)` → `(x)`
/// - `$derived(expr)` → `(expr)`
/// - `$derived.by(fn)` → `(fn)()`
/// - `$effect(() => {...})` → `((() => {...}))()`
/// - `$effect.pre(() => {...})` → `((() => {...}))()`
/// - `$effect.root(fn)` → `(fn)()`
/// - `$bindable(default?)` → `default` or nothing
/// - `$inspect(...)` → `void 0`
/// - `$host()` → `this`
/// - `$props()` → preserved (handled during type extraction)
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

    // Transform store subscriptions in the output that weren't already transformed
    // (rune contents are transformed during scanning, but non-rune code is not)
    result.output = transform_store_subscriptions(&result.output);

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
                    // Just emit the $ - store subscriptions will be transformed later
                    // by transform_store_subscriptions() which handles typeof contexts
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
                self.template_depth -= 1;
                if self.template_depth == 0 {
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
        // Check $state<Type>() - transforms to the content or undefined
        if remaining.starts_with("$state<") {
            if let Some((full_end, content, content_span)) =
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
                    full_text: Some(self.source[start_pos..full_end].to_string()),
                });
            }
        }

        // Check $derived<Type>() - transforms to the content
        if remaining.starts_with("$derived<") {
            if let Some((full_end, content, content_span)) =
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
                    full_text: Some(self.source[start_pos..full_end].to_string()),
                });
            }
        }

        // Check $props<Type>() - special handling for props
        if remaining.starts_with("$props<") {
            if let Some((full_end, _content, _content_span)) =
                self.find_rune_with_generic(start_pos, "$props<")
            {
                let full_text = self.source[start_pos..full_end].to_string();
                return Some(RuneMatch {
                    kind: RuneKind::Props,
                    full_span: Span::new(
                        self.base_offset + start_pos as u32,
                        self.base_offset + full_end as u32,
                    ),
                    content: None,
                    content_span: None,
                    pattern_len: 7, // "$props<"
                    full_text: Some(full_text),
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
                        full_text: None,
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
                        full_text: None,
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
    ) -> Option<(usize, Option<String>, Option<Span>)> {
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
            Some((full_end, content_str, content_span))
        } else {
            None
        }
    }

    /// Find the matching closing parenthesis, handling nested parens, strings, etc.
    fn find_matching_paren<'b>(&self, s: &'b str) -> Option<(&'b str, usize)> {
        let mut depth = 1;
        let mut in_string = None; // None, Some('\''), Some('"'), Some('`')
        let mut prev_was_escape = false;

        for (i, ch) in s.char_indices() {
            if prev_was_escape {
                prev_was_escape = false;
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
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((&s[..i], i));
                        }
                    }
                    _ => {}
                },
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
            RuneKind::State | RuneKind::StateRaw => {
                // $state(init) → init, or undefined if empty
                if let Some(content) = &rune_match.content {
                    if content.trim().is_empty() {
                        self.push_str("undefined");
                    } else {
                        // Transform store subscriptions within the content
                        let transformed = transform_store_subscriptions(content);
                        self.push_str(&transformed);
                    }
                } else {
                    self.push_str("undefined");
                }
            }
            RuneKind::StateSnapshot => {
                // $state.snapshot(x) → (x)
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    let transformed = transform_store_subscriptions(content);
                    self.push_str(&transformed);
                }
                self.push_char(')');
            }
            RuneKind::Derived => {
                // $derived(expr) → (expr)
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    let transformed = transform_store_subscriptions(content);
                    self.push_str(&transformed);
                }
                self.push_char(')');
            }
            RuneKind::DerivedBy => {
                // $derived.by(fn) → (fn)()
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    let transformed = transform_store_subscriptions(content);
                    self.push_str(&transformed);
                }
                self.push_str(")()");
            }
            RuneKind::Effect | RuneKind::EffectPre => {
                // $effect(fn) → ((fn))()
                self.push_str("((");
                if let Some(content) = &rune_match.content {
                    let transformed = transform_store_subscriptions(content);
                    self.push_str(&transformed);
                }
                self.push_str("))()");
            }
            RuneKind::EffectRoot => {
                // $effect.root(fn) → (fn)()
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    let transformed = transform_store_subscriptions(content);
                    self.push_str(&transformed);
                }
                self.push_str(")()");
            }
            RuneKind::Bindable => {
                // $bindable(default?) → default or undefined
                if let Some(content) = &rune_match.content {
                    if !content.trim().is_empty() {
                        let transformed = transform_store_subscriptions(content);
                        self.push_str(&transformed);
                    } else {
                        self.push_str("undefined");
                    }
                } else {
                    self.push_str("undefined");
                }
            }
            RuneKind::Inspect => {
                // $inspect(...) → void 0
                self.push_str("void 0");
            }
            RuneKind::Host => {
                // $host() → this
                self.push_str("this");
            }
            RuneKind::Props => {
                // Transform $props() to valid TypeScript
                // $props<Type>() -> ({} as Type)
                // let {...}: Type = $props() -> ({} as Type) - extract type from LHS annotation
                // $props() in SvelteKit route -> ({} as PageData) or ({} as LayoutData)
                // $props() -> ({} as Record<string, unknown>) - fallback
                let fallback_type = self.default_props_type.unwrap_or("Record<string, unknown>");

                if let Some(full_text) = &rune_match.full_text {
                    // Extract the type from $props<Type>()
                    // full_text is like "$props<{ name: string }>()"
                    let type_start = "$props<".len();
                    let type_end = full_text.len() - ">()".len();
                    if type_end > type_start {
                        let type_str = &full_text[type_start..type_end];
                        self.push_str("({} as ");
                        self.push_str(type_str);
                        self.push_char(')');
                    } else {
                        // Try to extract type annotation from LHS (e.g., }: Type = $props())
                        let type_from_lhs = self.extract_type_from_lhs();
                        if let Some(type_str) = type_from_lhs {
                            self.push_str("({} as ");
                            self.push_str(&type_str);
                            self.push_char(')');
                        } else {
                            self.push_str("({} as ");
                            self.push_str(fallback_type);
                            self.push_char(')');
                        }
                    }
                } else {
                    // $props() without generics - try to extract type from LHS
                    let type_from_lhs = self.extract_type_from_lhs();
                    if let Some(type_str) = type_from_lhs {
                        self.push_str("({} as ");
                        self.push_str(&type_str);
                        self.push_char(')');
                    } else {
                        self.push_str("({} as ");
                        self.push_str(fallback_type);
                        self.push_char(')');
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

        // Find the last `= ` which is right before $props()
        let equals_pos = output.rfind(" = ").or_else(|| output.rfind("= "))?;

        // Now look backwards from the equals sign to find `: Type`
        // We need to find the closing `}` of destructuring, then the `: Type` after it
        let before_equals = &output[..equals_pos];

        // Find the colon that starts the type annotation
        // This is tricky because the type itself might contain colons (in object types)
        // Look for `}: ` or `): ` which indicates end of pattern + type annotation
        let type_start = before_equals
            .rfind("}: ")
            .or_else(|| before_equals.rfind("): "))?;
        let type_str = &before_equals[type_start + 3..].trim();

        if type_str.is_empty() {
            return None;
        }

        // Validate that this looks like a type (not empty, starts reasonably)
        let first_char = type_str.chars().next()?;
        if !first_char.is_alphabetic() && first_char != '{' && first_char != '(' {
            return None;
        }

        Some(type_str.to_string())
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
    /// The full original text of the rune call (for preserving generics).
    full_text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_state() {
        let result = transform_runes("let count = $state(0);", 0);
        assert_eq!(result.output, "let count = 0;");
        assert_eq!(result.runes.len(), 1);
        assert_eq!(result.runes[0].kind, RuneKind::State);
    }

    #[test]
    fn test_transform_state_with_object() {
        let result = transform_runes("let obj = $state({ a: 1 });", 0);
        assert_eq!(result.output, "let obj = { a: 1 };");
    }

    #[test]
    fn test_transform_state_raw() {
        let result = transform_runes("let arr = $state.raw([1, 2, 3]);", 0);
        assert_eq!(result.output, "let arr = [1, 2, 3];");
        assert_eq!(result.runes[0].kind, RuneKind::StateRaw);
    }

    #[test]
    fn test_transform_state_snapshot() {
        let result = transform_runes("const snap = $state.snapshot(obj);", 0);
        assert_eq!(result.output, "const snap = (obj);");
        assert_eq!(result.runes[0].kind, RuneKind::StateSnapshot);
    }

    #[test]
    fn test_transform_derived() {
        let result = transform_runes("let double = $derived(count * 2);", 0);
        assert_eq!(result.output, "let double = (count * 2);");
        assert_eq!(result.runes[0].kind, RuneKind::Derived);
    }

    #[test]
    fn test_transform_derived_by() {
        let result = transform_runes("let value = $derived.by(() => compute());", 0);
        assert_eq!(result.output, "let value = (() => compute())();");
        assert_eq!(result.runes[0].kind, RuneKind::DerivedBy);
    }

    #[test]
    fn test_transform_effect() {
        let result = transform_runes("$effect(() => console.log(count));", 0);
        assert_eq!(result.output, "((() => console.log(count)))();");
        assert_eq!(result.runes[0].kind, RuneKind::Effect);
    }

    #[test]
    fn test_transform_effect_pre() {
        let result = transform_runes("$effect.pre(() => setup());", 0);
        assert_eq!(result.output, "((() => setup()))();");
        assert_eq!(result.runes[0].kind, RuneKind::EffectPre);
    }

    #[test]
    fn test_transform_effect_root() {
        let result = transform_runes("const cleanup = $effect.root(() => {});", 0);
        assert_eq!(result.output, "const cleanup = (() => {})();");
        assert_eq!(result.runes[0].kind, RuneKind::EffectRoot);
    }

    #[test]
    fn test_transform_host() {
        let result = transform_runes("$host().dispatchEvent(event);", 0);
        assert_eq!(result.output, "this.dispatchEvent(event);");
        assert_eq!(result.runes[0].kind, RuneKind::Host);
    }

    #[test]
    fn test_transform_inspect() {
        let result = transform_runes("$inspect(value);", 0);
        assert_eq!(result.output, "void 0;");
        assert_eq!(result.runes[0].kind, RuneKind::Inspect);
    }

    #[test]
    fn test_transform_bindable_with_default() {
        let result = transform_runes("let { value = $bindable(0) } = $props();", 0);
        assert_eq!(
            result.output,
            "let { value = 0 } = ({} as Record<string, unknown>);"
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
            "let { value = undefined } = ({} as Record<string, unknown>);"
        );
    }

    #[test]
    fn test_props_transformed() {
        let result = transform_runes("let { a, b } = $props();", 0);
        assert_eq!(
            result.output,
            "let { a, b } = ({} as Record<string, unknown>);"
        );
        assert_eq!(result.runes[0].kind, RuneKind::Props);
    }

    #[test]
    fn test_props_with_typescript_generics() {
        let result = transform_runes("let { name } = $props<{ name: string }>();", 0);
        assert_eq!(result.output, "let { name } = ({} as { name: string });");
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
            "let { data } = ({} as { data: Array<{ id: number }> });"
        );
        assert_eq!(result.runes[0].kind, RuneKind::Props);
    }

    #[test]
    fn test_props_with_arrow_function_type() {
        let result = transform_runes("let { onClick } = $props<{ onClick?: () => void }>();", 0);
        assert_eq!(
            result.output,
            "let { onClick } = ({} as { onClick?: () => void });"
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
            r#"let { name, onchange } = ({} as {
    name: string;
    onchange?: (n: number) => void;
});"#
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
        assert_eq!(result.output, "let x = fn(a, b);");
    }

    #[test]
    fn test_multiple_runes() {
        let result = transform_runes(
            "let count = $state(0); let doubled = $derived(count * 2);",
            0,
        );
        assert_eq!(result.output, "let count = 0; let doubled = (count * 2);");
        assert_eq!(result.runes.len(), 2);
    }

    #[test]
    fn test_mappings_tracked() {
        let result = transform_runes("let x = $state(0);", 0);
        assert_eq!(result.mappings.len(), 1);
        // Original: "$state(0)" at position 8-17
        assert_eq!(u32::from(result.mappings[0].original.start), 8);
        assert_eq!(u32::from(result.mappings[0].original.end), 17);
        // Generated: "0" at position 8-9
        assert_eq!(u32::from(result.mappings[0].generated.start), 8);
        assert_eq!(u32::from(result.mappings[0].generated.end), 9);
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
        // Runes inside template expressions should be transformed
        let result = transform_runes("let x = `value: ${$state(0)}`;", 0);
        assert_eq!(result.output, "let x = `value: ${0}`;");
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
            result.output.contains("__svelte_store_get(formData)"),
            "Expected __svelte_store_get(formData) but got:\n{}",
            result.output
        );
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
        // Both should be transformed
        assert!(
            !result.output.contains("$formData"),
            "Found untransformed $formData in:\n{}",
            result.output
        );
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
        // All $formData references should be transformed
        assert!(
            !result.output.contains("$formData"),
            "Found untransformed $formData in:\n{}",
            result.output
        );
        // Should contain the transformed version
        assert!(
            result.output.contains("__svelte_store_get(formData)"),
            "Expected __svelte_store_get(formData) but got:\n{}",
            result.output
        );
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
        // All $formData references should be transformed
        assert!(
            !result.output.contains("$formData"),
            "Found untransformed $formData in:\n{}",
            result.output
        );
    }
}
