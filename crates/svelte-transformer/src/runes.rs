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
pub fn transform_runes(script: &str, base_offset: u32) -> RuneTransformResult {
    let scanner = RuneScanner::new(script, base_offset);
    scanner.scan_and_transform()
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
}

impl<'a> RuneScanner<'a> {
    fn new(source: &'a str, base_offset: u32) -> Self {
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
                    });
                }
            }
        }

        None
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
        let rune_len = u32::from(rune_match.full_span.end) - u32::from(rune_match.full_span.start);
        for _ in 0..(rune_len as usize - 1) {
            // -1 because we already consumed '$'
            self.chars.next();
        }

        let gen_start = self.output_pos;

        match rune_match.kind {
            RuneKind::State | RuneKind::StateRaw => {
                // $state(init) → init
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
            }
            RuneKind::StateSnapshot => {
                // $state.snapshot(x) → (x)
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_char(')');
            }
            RuneKind::Derived => {
                // $derived(expr) → (expr)
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_char(')');
            }
            RuneKind::DerivedBy => {
                // $derived.by(fn) → (fn)()
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_str(")()");
            }
            RuneKind::Effect | RuneKind::EffectPre => {
                // $effect(fn) → ((fn))()
                self.push_str("((");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_str("))()");
            }
            RuneKind::EffectRoot => {
                // $effect.root(fn) → (fn)()
                self.push_char('(');
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_str(")()");
            }
            RuneKind::Bindable => {
                // $bindable(default?) → default or empty
                if let Some(content) = &rune_match.content {
                    if !content.trim().is_empty() {
                        self.push_str(content);
                    }
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
                // $props() is preserved as-is for type extraction
                self.push_str("$props(");
                if let Some(content) = &rune_match.content {
                    self.push_str(content);
                }
                self.push_char(')');
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
        assert_eq!(result.output, "let { value = 0 } = $props();");
        assert_eq!(result.runes.len(), 2);
        assert_eq!(result.runes[0].kind, RuneKind::Bindable);
        assert_eq!(result.runes[1].kind, RuneKind::Props);
    }

    #[test]
    fn test_transform_bindable_empty() {
        let result = transform_runes("let { value = $bindable() } = $props();", 0);
        assert_eq!(result.output, "let { value =  } = $props();");
    }

    #[test]
    fn test_props_preserved() {
        let result = transform_runes("let { a, b } = $props();", 0);
        assert_eq!(result.output, "let { a, b } = $props();");
        assert_eq!(result.runes[0].kind, RuneKind::Props);
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
}
