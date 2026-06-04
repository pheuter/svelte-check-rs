//! Props type extraction from `$props()` patterns.
//!
//! This module extracts type information from Svelte 5's `$props()` rune
//! to generate proper TypeScript types for component exports.
//!
//! Supported patterns:
//! - `let { a, b }: Props = $props()`
//! - `let { a } = $props<{ a: string }>()`
//! - `let { a = defaultValue } = $props()`
//! - `let { value = $bindable(0) } = $props()`
//! - `let props = $props()` (generic props object)

use source_map::Span;
use std::sync::Arc;
use swc_common::{FileName, SourceMap as SwcSourceMap, Spanned};
use swc_ecma_ast::{Expr, Lit};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

/// Information about the component's props extracted from `$props()`.
#[derive(Debug, Clone, Default)]
pub struct PropsInfo {
    /// The TypeScript type annotation for props (if explicitly provided).
    pub type_annotation: Option<String>,
    /// The span of the type annotation in the source.
    pub type_span: Option<Span>,
    /// Individual destructured properties.
    pub properties: Vec<PropProperty>,
    /// Whether props were destructured or captured as a single object.
    pub is_destructured: bool,
    /// If not destructured, the variable name (e.g., `props` in `let props = $props()`).
    pub object_binding: Option<String>,
}

/// A single property from props destructuring.
#[derive(Debug, Clone)]
pub struct PropProperty {
    /// The property name (the *exported* prop name, before any `:` rename).
    pub name: String,
    /// The local binding name (the alias after `:` in a `prop: local` rename),
    /// falling back to the exported `name` when there is no rename.
    ///
    /// This is used to mark bindable props as "used" so `noUnusedLocals` does
    /// not flag them. The local name is always a valid identifier, whereas the
    /// exported `name` can be a reserved word (e.g. `class`), which would make
    /// the generated `;class;` reference a TS syntax error (upstream #3017).
    pub local_name: Option<String>,
    /// The span of the property name in the source.
    pub span: Span,
    /// Whether this property is marked with `$bindable()`.
    pub is_bindable: bool,
    /// Default value expression (if any).
    pub default_value: Option<String>,
    /// Type annotation for this specific property (if using inline types).
    pub type_annotation: Option<String>,
    /// Whether the property is a rest element (`...rest`).
    pub is_rest: bool,
}

/// Extract props information from a script that contains `$props()`.
///
/// # Arguments
/// * `_script` - The script content (after rune transformation).
/// * `original_script` - The original script content (before transformation).
/// * `base_offset` - The byte offset where the script starts in the original file.
///
/// # Returns
/// `Some(PropsInfo)` if `$props()` was found, `None` otherwise.
pub fn extract_props_info(
    _script: &str,
    original_script: &str,
    base_offset: u32,
) -> Option<PropsInfo> {
    // Find the `$props()`/`$props<...>()` rune call, skipping any `$props`
    // mentioned in a comment or string literal. A plain `find("$props")` is
    // fooled by an explanatory comment like `// untyped $props()` and then fails
    // the backward search for the declaration, silently falling back to
    // `Record<string, unknown>`.
    let props_idx = find_props_rune(original_script)?;

    // Look backwards from $props to find the variable declaration
    let before_props = &original_script[..props_idx];

    // Find the `let` or `const` keyword and destructuring pattern
    let decl_start = before_props
        .rfind("let ")
        .or_else(|| before_props.rfind("const "))?;
    let declaration = &original_script[decl_start..];

    // Parse the declaration pattern
    parse_props_declaration(declaration, base_offset + decl_start as u32)
}

/// Find the byte offset of the `$props` rune call (`$props(` or `$props<`),
/// skipping occurrences inside strings and comments. Returns the offset of the
/// `$`. `$props.x` accessors and identifiers like `$propsFoo` are not matched.
fn find_props_rune(script: &str) -> Option<usize> {
    let mut chars = script.char_indices().peekable();
    let mut in_string: Option<char> = None;
    let mut prev_escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some((idx, ch)) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && chars.peek().map(|(_, c)| *c) == Some('/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if let Some(quote) = in_string {
            if prev_escape {
                prev_escape = false;
            } else if ch == '\\' {
                prev_escape = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }
        if ch == '/' && chars.peek().map(|(_, c)| *c) == Some('/') {
            chars.next();
            in_line_comment = true;
            continue;
        }
        if ch == '/' && chars.peek().map(|(_, c)| *c) == Some('*') {
            chars.next();
            in_block_comment = true;
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '`' {
            in_string = Some(ch);
            continue;
        }

        // A `$props` token is a rune only when it is not part of a larger
        // identifier (`$propsFoo`) and is immediately followed by `(` or `<`.
        if ch == '$' && script[idx..].starts_with("$props") {
            let preceded_by_ident = idx > 0
                && script[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
            let after = idx + "$props".len();
            let next = script[after..].chars().next();
            if !preceded_by_ident && matches!(next, Some('(') | Some('<')) {
                return Some(idx);
            }
        }
    }

    None
}

/// Parse a props declaration to extract property information.
fn parse_props_declaration(declaration: &str, base_offset: u32) -> Option<PropsInfo> {
    let mut info = PropsInfo::default();

    // Skip 'let ' or 'const '
    let (after_keyword, keyword_len) = if let Some(rest) = declaration.strip_prefix("let ") {
        (rest, 4)
    } else if let Some(rest) = declaration.strip_prefix("const ") {
        (rest, 6)
    } else {
        return None;
    };

    let trimmed = after_keyword.trim_start();
    let whitespace_len = after_keyword.len() - trimmed.len();
    let pattern_start = keyword_len + whitespace_len;

    // Check if it's destructuring `{ ... }` or simple binding `props`
    if let Some(brace_rest) = trimmed.strip_prefix('{') {
        info.is_destructured = true;

        // Find the matching closing brace
        if let Some((content, closing_idx)) = find_matching_brace(brace_rest) {
            // Parse destructuring properties
            let props_start_offset = base_offset + pattern_start as u32 + 1; // +1 for '{'
            info.properties = parse_destructuring_properties(content, props_start_offset);

            // Check for type annotation after closing brace
            let after_brace = &brace_rest[closing_idx + 1..]; // +1 for '}'
            if let Some(type_ann) = extract_type_annotation(after_brace) {
                info.type_annotation = Some(type_ann);
            }
        }
    } else {
        // Simple binding: `let props = $props()`
        info.is_destructured = false;

        // Find the variable name (before `=` or `:`)
        let end_of_name = trimmed
            .find(|c: char| c == '=' || c == ':' || c.is_whitespace())
            .unwrap_or(trimmed.len());
        let var_name = &trimmed[..end_of_name];
        if !var_name.is_empty() {
            info.object_binding = Some(var_name.to_string());
        }

        // Check for type annotation
        let after_name = &trimmed[end_of_name..].trim_start();
        if after_name.starts_with(':') {
            if let Some(type_ann) = extract_type_annotation(after_name) {
                info.type_annotation = Some(type_ann);
            }
        }
    }

    // Check for generic type parameter in $props<Type>()
    if let Some(generic_type) = extract_generic_type_param(declaration) {
        // Generic type takes precedence over annotation
        if info.type_annotation.is_none() {
            info.type_annotation = Some(generic_type);
        }
    }

    Some(info)
}

/// Parse destructuring properties from the content between braces.
fn parse_destructuring_properties(content: &str, base_offset: u32) -> Vec<PropProperty> {
    let mut properties = Vec::new();
    let mut current_pos = 0;
    let mut current_prop = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, ch) in content.char_indices() {
        // Handle strings
        if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
            in_string = true;
            string_char = ch;
            current_prop.push(ch);
            continue;
        }
        if in_string {
            current_prop.push(ch);
            if ch == string_char {
                in_string = false;
            }
            continue;
        }

        // Track nesting
        if ch == '(' || ch == '{' || ch == '[' {
            depth += 1;
            current_prop.push(ch);
            continue;
        }
        if ch == ')' || ch == '}' || ch == ']' {
            depth -= 1;
            current_prop.push(ch);
            continue;
        }

        // Property separator
        if ch == ',' && depth == 0 {
            if !current_prop.trim().is_empty() {
                if let Some(prop) =
                    parse_single_property(current_prop.trim(), base_offset + current_pos as u32)
                {
                    properties.push(prop);
                }
            }
            current_prop.clear();
            current_pos = i + 1;
            continue;
        }

        current_prop.push(ch);
    }

    // Don't forget the last property
    if !current_prop.trim().is_empty() {
        if let Some(prop) =
            parse_single_property(current_prop.trim(), base_offset + current_pos as u32)
        {
            properties.push(prop);
        }
    }

    properties
}

/// Parse a single destructuring property.
fn parse_single_property(prop_str: &str, base_offset: u32) -> Option<PropProperty> {
    let trimmed = prop_str.trim();

    if trimmed.is_empty() {
        return None;
    }

    // Handle rest element: ...rest
    if let Some(rest_name) = trimmed.strip_prefix("...") {
        return Some(PropProperty {
            name: rest_name.to_string(),
            // Rest elements are never bindable, so the local name is unused.
            local_name: None,
            span: Span::new(base_offset, base_offset + trimmed.len() as u32),
            is_bindable: false,
            default_value: None,
            type_annotation: None,
            is_rest: true,
        });
    }

    // Check for default value: `prop = defaultValue` or `prop = $bindable(...)`
    let (name_part, default_part) = if let Some(eq_idx) = find_first_equals(trimmed) {
        (&trimmed[..eq_idx], Some(&trimmed[eq_idx + 1..]))
    } else {
        (trimmed, None)
    };

    let name = name_part.trim();

    // Inside a destructuring pattern, `prop: local` is a *rename* (bind the
    // `prop` property to the local variable `local`), NOT a type annotation —
    // TypeScript does not allow inline type annotations within a destructuring
    // pattern (the type goes after the closing brace: `let { a }: T = ...`).
    // The exposed prop name is the part before the colon; the alias after it is
    // a local-binding name and irrelevant to the generated props type. Treating
    // the alias as a type produced types like `class?: className` — a value used
    // as a type, i.e. TS2749 (see issue #150).
    let final_name = match name.find(':') {
        Some(colon_idx) => name[..colon_idx].trim(),
        None => name,
    };
    // The local binding name is the alias after the colon (always a valid
    // identifier); without a rename it is the exported name itself. Used to
    // mark bindable props as read (upstream #3017).
    let local_name = match name.find(':') {
        Some(colon_idx) => name[colon_idx + 1..].trim().to_string(),
        None => final_name.to_string(),
    };
    let type_ann: Option<String> = None;

    // Check if default is $bindable
    let (is_bindable, actual_default) = if let Some(default) = default_part {
        let default = default.trim();
        if let Some(bindable_inner) = default.strip_prefix("$bindable(") {
            // Extract the content inside $bindable()
            if let Some(end) = find_matching_paren_simple(bindable_inner) {
                let bindable_content = &bindable_inner[..end];
                let actual = if bindable_content.trim().is_empty() {
                    None
                } else {
                    Some(bindable_content.trim().to_string())
                };
                (true, actual)
            } else {
                (true, None)
            }
        } else {
            (false, Some(default.to_string()))
        }
    } else {
        (false, None)
    };

    Some(PropProperty {
        name: final_name.to_string(),
        local_name: Some(local_name),
        span: Span::new(base_offset, base_offset + prop_str.len() as u32),
        is_bindable,
        default_value: actual_default,
        type_annotation: type_ann,
        is_rest: false,
    })
}

/// Find the first `=` that's not inside parens/braces/brackets.
fn find_first_equals(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, ch) in s.char_indices() {
        if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
            in_string = true;
            string_char = ch;
            continue;
        }
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }

        if ch == '(' || ch == '{' || ch == '[' {
            depth += 1;
        } else if ch == ')' || ch == '}' || ch == ']' {
            depth -= 1;
        } else if ch == '=' && depth == 0 {
            // Make sure it's not ==, ===, =>, etc.
            let next = s[i + 1..].chars().next();
            if next != Some('=') && next != Some('>') {
                return Some(i);
            }
        }
    }

    None
}

/// Find matching brace and return content and closing index.
fn find_matching_brace(s: &str) -> Option<(&str, usize)> {
    let mut depth = 1;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, ch) in s.char_indices() {
        if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
            in_string = true;
            string_char = ch;
            continue;
        }
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }

        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return Some((&s[..i], i));
            }
        }
    }

    None
}

/// Simple paren matching that returns the index of content end.
fn find_matching_paren_simple(s: &str) -> Option<usize> {
    let mut depth = 1;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, ch) in s.char_indices() {
        if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
            in_string = true;
            string_char = ch;
            continue;
        }
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }

        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }

    None
}

/// Extract type annotation after `: Type = `.
fn extract_type_annotation(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if !trimmed.starts_with(':') {
        return None;
    }

    // Find the end of the type (before `=`)
    let type_str = &trimmed[1..].trim();
    let chars: Vec<char> = type_str.chars().collect();
    let len = chars.len();

    // Track depth for brackets, strings, and comments
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev_char: Option<char> = None;
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Handle line comments: skip until newline
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            prev_char = Some(ch);
            i += 1;
            continue;
        }

        // Handle block comments: skip until */
        if in_block_comment {
            if ch == '*' && i + 1 < len && chars[i + 1] == '/' {
                in_block_comment = false;
                i += 2;
                prev_char = Some('/');
                continue;
            }
            prev_char = Some(ch);
            i += 1;
            continue;
        }

        // Check for comment start (only outside strings)
        if !in_string && ch == '/' && i + 1 < len {
            if chars[i + 1] == '/' {
                in_line_comment = true;
                i += 2;
                prev_char = Some('/');
                continue;
            } else if chars[i + 1] == '*' {
                in_block_comment = true;
                i += 2;
                prev_char = Some('*');
                continue;
            }
        }

        // Handle strings
        if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
            in_string = true;
            string_char = ch;
            prev_char = Some(ch);
            i += 1;
            continue;
        }
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            prev_char = Some(ch);
            i += 1;
            continue;
        }

        // Track depth for brackets
        if ch == '<' || ch == '(' || ch == '{' || ch == '[' {
            depth += 1;
        } else if ch == '>' {
            // Skip `>` if it's part of `=>` (arrow function in type)
            if prev_char != Some('=') {
                depth -= 1;
            }
        } else if ch == ')' || ch == '}' || ch == ']' {
            depth -= 1;
        } else if ch == '=' && depth == 0 {
            // Make sure it's not `=>` or `==`
            let next = if i + 1 < len {
                Some(chars[i + 1])
            } else {
                None
            };
            if next != Some('>') && next != Some('=') {
                // Calculate byte position from char position
                let byte_pos: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
                return Some(type_str[..byte_pos].trim().to_string());
            }
        }
        prev_char = Some(ch);
        i += 1;
    }

    None
}

/// Extract generic type parameter from `$props<Type>()`.
fn extract_generic_type_param(declaration: &str) -> Option<String> {
    let props_idx = declaration.find("$props<")?;
    let after_props = &declaration[props_idx + "$props<".len()..];

    // Find matching >
    let mut depth = 1;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut prev_char = None;

    for (i, ch) in after_props.char_indices() {
        if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
            in_string = true;
            string_char = ch;
            prev_char = Some(ch);
            continue;
        }
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            prev_char = Some(ch);
            continue;
        }

        if ch == '<' {
            depth += 1;
        } else if ch == '>' {
            // Skip `>` if it's part of `=>` (arrow function in type)
            if prev_char == Some('=') {
                // This is `=>`, not a closing angle bracket
            } else {
                depth -= 1;
                if depth == 0 {
                    return Some(after_props[..i].trim().to_string());
                }
            }
        }
        prev_char = Some(ch);
    }

    None
}

/// Generate a TypeScript type string for the props.
pub fn generate_props_type(info: &PropsInfo) -> String {
    // If we have an explicit type annotation, use it (but honor defaults/bindables as optional)
    if let Some(ref type_ann) = info.type_annotation {
        let optional_props: Vec<String> = info
            .properties
            .iter()
            .filter(|prop| prop.default_value.is_some() || prop.is_bindable)
            .map(|prop| format_prop_key_literal(&prop.name))
            .collect();

        if optional_props.is_empty() {
            return type_ann.clone();
        }

        // Avoid exploding complex prop types that can trigger TS2590.
        // If the annotation is large or has many optional keys, keep it as-is.
        if is_complex_type_reference(type_ann) {
            return type_ann.clone();
        }
        if optional_props.len() > 5 || type_ann.len() > 120 {
            return type_ann.clone();
        }

        let keys = optional_props.join(" | ");
        return format!("__SvelteOptionalProps<{t}, {k}>", t = type_ann, k = keys);
    }

    // If not destructured, we can't infer the type
    if !info.is_destructured {
        return "Record<string, unknown>".to_string();
    }

    // Generate type from properties
    let mut type_parts = Vec::new();
    let mut has_rest = false;

    for prop in &info.properties {
        if prop.is_rest {
            // Rest properties contribute to the type
            has_rest = true;
            continue;
        }

        let optional = if prop.default_value.is_some() || prop.is_bindable {
            "?"
        } else {
            ""
        };

        // Infer the prop type from its default value (mirrors svelte2tsx) so a
        // wrong-typed prop is flagged at the call site, e.g. `let { label = "" }`
        // makes `<Comp label={123} />` an error. Falls back to `unknown` when the
        // default can't be classified — consumer-equivalent to svelte2tsx's `any`,
        // so no spurious errors are introduced.
        let prop_type = prop
            .type_annotation
            .clone()
            .or_else(|| {
                prop.default_value
                    .as_deref()
                    .and_then(infer_type_from_default)
            })
            .unwrap_or_else(|| "unknown".to_string());

        type_parts.push(format!("{}{}: {}", prop.name, optional, prop_type));
    }

    let base = if type_parts.is_empty() {
        "{}".to_string()
    } else {
        format!("{{ {} }}", type_parts.join("; "))
    };

    if has_rest {
        if base == "{}" {
            "Record<string, unknown>".to_string()
        } else {
            format!("{} & Record<string, unknown>", base)
        }
    } else {
        base
    }
}

/// Infer a TypeScript type for an untyped prop from its default-value expression,
/// mirroring svelte2tsx's inference (`packages/svelte2tsx/.../ExportedNames.ts`).
///
/// The default expression is parsed with swc and classified by AST node, exactly
/// as svelte2tsx classifies the TS AST node. Anything not in the table returns
/// `None`, so the caller falls back to `unknown`; for a consumer-facing prop type
/// that accepts any passed value just like svelte2tsx's `any`, so falling back
/// never introduces a spurious error — it only forgoes the extra check. Parsing
/// (rather than string heuristics) matters for cases like `-5`, which is a unary
/// expression — not a numeric literal — so it is correctly *not* inferred as
/// `number` (which would wrongly reject `<Comp prop="x" />`).
fn infer_type_from_default(default: &str) -> Option<String> {
    let trimmed = default.trim();
    if trimmed.is_empty() {
        return None;
    }

    let cm: Arc<SwcSourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("svelte-prop-default".into()).into(),
        trimmed.to_string(),
    );
    let syntax = Syntax::Typescript(TsSyntax {
        tsx: false,
        ..Default::default()
    });
    let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
    let expr = parser.parse_expr().ok()?;

    match &*expr {
        Expr::Lit(Lit::Str(_)) => Some("string".to_string()),
        Expr::Lit(Lit::Num(_)) => Some("number".to_string()),
        Expr::Lit(Lit::Bool(_)) => Some("boolean".to_string()),
        Expr::Arrow(_) => Some("Function".to_string()),
        Expr::Object(_) => Some("Record<string, any>".to_string()),
        Expr::Array(_) => Some("any[]".to_string()),
        // `value as Foo` -> `Foo`: slice the type annotation's source text
        // (swc BytePos is 1-based per the fresh source file, so offset by
        // `fm.start_pos`).
        Expr::TsAs(as_expr) => {
            let span = as_expr.type_ann.span();
            let lo = span.lo.0.saturating_sub(fm.start_pos.0) as usize;
            let hi = span.hi.0.saturating_sub(fm.start_pos.0) as usize;
            trimmed.get(lo..hi).map(|s| s.trim().to_string())
        }
        // A bare identifier default (e.g. an imported/declared constant) carries a
        // useful type via `typeof`. `undefined` does not; `null` parses as a
        // literal, not an identifier, so it is handled by the fallback.
        Expr::Ident(id) if &*id.sym != "undefined" => Some(format!("typeof {}", id.sym)),
        _ => None,
    }
}

fn format_prop_key_literal(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next().unwrap_or_default();
        let last = trimmed.chars().last().unwrap_or_default();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return format!("{:?}", &trimmed[1..trimmed.len() - 1]);
        }
    }

    format!("{:?}", trimmed)
}

fn is_complex_type_reference(type_ann: &str) -> bool {
    let trimmed = type_ann.trim();
    if trimmed.starts_with('{') {
        return false;
    }

    trimmed.contains('.')
        || trimmed.contains('<')
        || trimmed.contains('>')
        || trimmed.contains('|')
        || trimmed.contains('&')
        || trimmed.contains('(')
        || trimmed.contains(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_type_from_default() {
        // Literals classify to their precise types (matches svelte2tsx).
        assert_eq!(infer_type_from_default("\"\""), Some("string".to_string()));
        // Template literals are not string literals (svelte2tsx's
        // `ts.isStringLiteral` is false for them too) -> fall back.
        assert_eq!(infer_type_from_default("`x`"), None);
        assert_eq!(infer_type_from_default("0"), Some("number".to_string()));
        assert_eq!(infer_type_from_default("3.14"), Some("number".to_string()));
        assert_eq!(infer_type_from_default("true"), Some("boolean".to_string()));
        assert_eq!(infer_type_from_default("[]"), Some("any[]".to_string()));
        assert_eq!(
            infer_type_from_default("{ a: 1 }"),
            Some("Record<string, any>".to_string())
        );
        assert_eq!(
            infer_type_from_default("() => {}"),
            Some("Function".to_string())
        );
        assert_eq!(
            infer_type_from_default("DEFAULT"),
            Some("typeof DEFAULT".to_string())
        );
        assert_eq!(infer_type_from_default("x as Foo"), Some("Foo".to_string()));

        // `-5` is a unary expression, not a numeric literal — svelte2tsx infers
        // `any` for it, so we must NOT infer `number` (that would wrongly reject
        // a string value at the call site). Falls back to `unknown` (caller),
        // which is consumer-equivalent to `any`.
        assert_eq!(infer_type_from_default("-5"), None);
        // `undefined`/`null` and unclassifiable expressions carry no useful type.
        assert_eq!(infer_type_from_default("undefined"), None);
        assert_eq!(infer_type_from_default("null"), None);
        assert_eq!(infer_type_from_default("foo()"), None);
        assert_eq!(infer_type_from_default("a.b"), None);
    }

    #[test]
    fn test_extract_simple_destructuring() {
        let script = "let { a, b } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert!(info.is_destructured);
        assert_eq!(info.properties.len(), 2);
        assert_eq!(info.properties[0].name, "a");
        assert_eq!(info.properties[1].name, "b");
        assert!(!info.properties[0].is_bindable);
    }

    #[test]
    fn test_extract_with_type_annotation() {
        let script = "let { a, b }: Props = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert!(info.is_destructured);
        assert_eq!(info.type_annotation, Some("Props".to_string()));
    }

    #[test]
    fn test_extract_ignores_props_in_comment_and_string() {
        // A `$props` mentioned in a comment or string must not be mistaken for
        // the rune call; otherwise the backward search for the declaration fails
        // and props silently fall back to `Record<string, unknown>`.
        let script = r#"// configure via $props() below
        const note = "see $props docs";
        let { a, b } = $props();"#;
        let info = extract_props_info(script, script, 0).unwrap();

        assert!(info.is_destructured);
        assert_eq!(info.properties.len(), 2);
        assert_eq!(info.properties[0].name, "a");
        assert_eq!(info.properties[1].name, "b");
    }

    #[test]
    fn test_find_props_rune_skips_accessor_and_lookalikes() {
        // `$props.id()` accessor and `$propsFoo` identifier are not the rune call.
        assert_eq!(find_props_rune("let { a } = $props();"), Some(12));
        assert_eq!(find_props_rune("const x = $propsFoo();"), None);
        assert_eq!(
            find_props_rune("const id = $props.id(); let { a } = $props();"),
            Some(36)
        );
    }

    #[test]
    fn test_extract_with_generic_type() {
        let script = "let { a } = $props<{ a: string }>();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert!(info.is_destructured);
        assert_eq!(info.type_annotation, Some("{ a: string }".to_string()));
    }

    #[test]
    fn test_extract_with_defaults() {
        let script = "let { a = 1, b = 'hello' } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 2);
        assert_eq!(info.properties[0].name, "a");
        assert_eq!(info.properties[0].default_value, Some("1".to_string()));
        assert_eq!(info.properties[1].name, "b");
        assert_eq!(
            info.properties[1].default_value,
            Some("'hello'".to_string())
        );
    }

    #[test]
    fn test_extract_with_bindable() {
        let script = "let { value = $bindable(0) } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 1);
        assert_eq!(info.properties[0].name, "value");
        assert!(info.properties[0].is_bindable);
        assert_eq!(info.properties[0].default_value, Some("0".to_string()));
    }

    #[test]
    fn test_extract_with_empty_bindable() {
        let script = "let { value = $bindable() } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 1);
        assert!(info.properties[0].is_bindable);
        assert!(info.properties[0].default_value.is_none());
    }

    #[test]
    fn test_local_name_falls_back_to_exported_name() {
        // No `:` rename: local name equals the exported name (upstream #3017).
        let script = "let { value = $bindable(0) } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 1);
        assert_eq!(info.properties[0].name, "value");
        assert_eq!(info.properties[0].local_name, Some("value".to_string()));
    }

    #[test]
    fn test_local_name_uses_alias_for_renamed_bindable() {
        // `class: className` — exported name is the reserved word `class`, the
        // local binding is `className`. The marker must use the local name so
        // the generated `;className;` is valid TS (upstream #3017).
        let script = "let { class: className = $bindable() } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 1);
        assert_eq!(info.properties[0].name, "class");
        assert_eq!(info.properties[0].local_name, Some("className".to_string()));
        assert!(info.properties[0].is_bindable);
    }

    #[test]
    fn test_extract_object_binding() {
        let script = "let props = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert!(!info.is_destructured);
        assert_eq!(info.object_binding, Some("props".to_string()));
    }

    #[test]
    fn test_extract_rest_element() {
        let script = "let { a, ...rest } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 2);
        assert_eq!(info.properties[0].name, "a");
        assert!(!info.properties[0].is_rest);
        assert_eq!(info.properties[1].name, "rest");
        assert!(info.properties[1].is_rest);
    }

    #[test]
    fn test_generate_props_type_with_annotation() {
        let info = PropsInfo {
            type_annotation: Some("MyProps".to_string()),
            ..Default::default()
        };
        assert_eq!(generate_props_type(&info), "MyProps");
    }

    #[test]
    fn test_generate_props_type_from_properties() {
        let info = PropsInfo {
            is_destructured: true,
            properties: vec![
                PropProperty {
                    name: "count".to_string(),
                    local_name: Some("count".to_string()),
                    span: Span::default(),
                    is_bindable: false,
                    default_value: None,
                    type_annotation: Some("number".to_string()),
                    is_rest: false,
                },
                PropProperty {
                    name: "label".to_string(),
                    local_name: Some("label".to_string()),
                    span: Span::default(),
                    is_bindable: false,
                    default_value: Some("'default'".to_string()),
                    type_annotation: Some("string".to_string()),
                    is_rest: false,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            generate_props_type(&info),
            "{ count: number; label?: string }"
        );
    }

    #[test]
    fn test_complex_generic_type() {
        let script = "let { items } = $props<{ items: Array<{ id: number; name: string }> }>();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(
            info.type_annotation,
            Some("{ items: Array<{ id: number; name: string }> }".to_string())
        );
    }

    #[test]
    fn test_property_with_complex_default() {
        let script = "let { items = [], callback = () => {} } = $props();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(info.properties.len(), 2);
        assert_eq!(info.properties[0].name, "items");
        assert_eq!(info.properties[0].default_value, Some("[]".to_string()));
        assert_eq!(info.properties[1].name, "callback");
        assert_eq!(
            info.properties[1].default_value,
            Some("() => {}".to_string())
        );
    }

    #[test]
    fn test_generic_type_with_arrow_function() {
        let script = "let { onClick } = $props<{ onClick?: () => void }>();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(
            info.type_annotation,
            Some("{ onClick?: () => void }".to_string())
        );
    }

    #[test]
    fn test_generic_type_with_callback() {
        let script = "let { onchange } = $props<{ onchange?: (n: number) => void }>();";
        let info = extract_props_info(script, script, 0).unwrap();

        assert_eq!(
            info.type_annotation,
            Some("{ onchange?: (n: number) => void }".to_string())
        );
    }

    #[test]
    fn test_extract_type_with_template_literal_in_generic() {
        // This pattern from Trajectory.svelte: Omit<ComponentProps<typeof Histogram>, `series`>
        let script = r#"let { histogram_props = {} }: {
    histogram_props?: Omit<ComponentProps<typeof Histogram>, `series`>
  } = $props()"#;
        let info = extract_props_info(script, script, 0).unwrap();

        assert!(info.is_destructured);
        let type_ann = info.type_annotation.as_ref().unwrap();
        // Type annotation should NOT include "= $props()"
        assert!(
            !type_ann.contains("$props"),
            "Type annotation should not contain $props: {}",
            type_ann
        );
        assert!(type_ann.contains("Omit<ComponentProps<typeof Histogram>, `series`>"));
    }

    #[test]
    fn test_extract_type_annotation_function_directly() {
        // Test the extract_type_annotation function directly
        let input = ": { x?: `foo` | `bar` } = $props()";
        let result = extract_type_annotation(input);
        assert_eq!(result, Some("{ x?: `foo` | `bar` }".to_string()));
    }

    #[test]
    fn test_extract_type_annotation_with_omit_template_literal() {
        let input =
            ": { histogram_props?: Omit<ComponentProps<typeof Histogram>, `series`> } = $props()";
        let result = extract_type_annotation(input);
        assert!(result.is_some(), "Should extract type annotation");
        let type_ann = result.unwrap();
        assert!(
            !type_ann.contains("$props"),
            "Should not contain $props: {}",
            type_ann
        );
    }

    #[test]
    fn test_find_matching_brace_with_template_literals() {
        // Destructuring with template literal defaults
        let input = r#"
    trajectory = $bindable(),
    layout = `auto`,
    display_mode = $bindable(`structure+scatter`),
  }: EventHandlers & { x: number } = $props()"#;

        let result = find_matching_brace(input);
        assert!(result.is_some(), "Should find matching brace");
        let (content, _idx) = result.unwrap();
        // Content should NOT include the }: part after
        assert!(
            !content.contains("EventHandlers"),
            "Content should not include type annotation: {}",
            content
        );
    }

    #[test]
    fn test_trajectory_actual_props_pattern() {
        // Actual props declaration from matterviz Trajectory.svelte
        let script = r#"  let {
    trajectory = $bindable(),
    data_url,
    current_step_idx = $bindable(0),
    data_extractor = full_data_extractor,
    allow_file_drop = true,
    layout = `auto`,
    structure_props = {},
    scatter_props = {},
    histogram_props = {},
    spinner_props = {},
    trajectory_controls,
    error_snippet,
    show_controls,
    fullscreen_toggle = DEFAULTS.trajectory.fullscreen_toggle,
    auto_play = false,
    display_mode = $bindable(`structure+scatter`),
    step_labels = 5,
    visible_properties = $bindable(),
    ELEM_PROPERTY_LABELS,
    on_play,
    on_pause,
    on_step_change,
    on_end,
    on_loop,
    on_frame_rate_change,
    on_display_mode_change,
    on_fullscreen_change,
    on_file_load,
    on_error,
    fps_range = DEFAULTS.trajectory.fps_range,
    fps = $bindable(5),
    loading_options = {},
    atom_type_mapping,
    plot_skimming = true,
    ...rest
  }: EventHandlers & HTMLAttributes<HTMLDivElement> & {
    trajectory?: TrajectoryType
    data_url?: string
    current_step_idx?: number
    data_extractor?: TrajectoryDataExtractor
    allow_file_drop?: boolean
    layout?: `auto` | Orientation
    structure_props?: ComponentProps<typeof Structure>
    scatter_props?: ComponentProps<typeof ScatterPlot>
    histogram_props?: Omit<ComponentProps<typeof Histogram>, `series`>
    spinner_props?: ComponentProps<typeof Spinner>
    trajectory_controls?: Snippet<[ControlsProps]>
    error_snippet?: Snippet<[{ error_msg: string; on_dismiss: () => void }]>
    show_controls?: ShowControlsProp
    fullscreen_toggle?: Snippet<[{ fullscreen: boolean }]> | boolean
    auto_play?: boolean
    display_mode?:
      | `structure+scatter`
      | `structure`
      | `scatter`
      | `histogram`
      | `structure+histogram`
    step_labels?: number | number[]
    visible_properties?: string[]
    ELEM_PROPERTY_LABELS?: Record<string, string>
    units?: {
      energy?: string
      energy_per_atom?: string
      force_max?: string
      [key: string]: string | undefined
    }
    fps_range?: [number, number]
    fps?: number
    loading_options?: LoadingOptions
    atom_type_mapping?: AtomTypeMapping
    plot_skimming?: boolean
  } = $props()

  let dragover = $state(false)
  let loading = $state(false)"#;

        let info = extract_props_info(script, script, 0).unwrap();

        assert!(info.is_destructured, "Should be destructured");
        let type_ann = info
            .type_annotation
            .as_ref()
            .expect("Should have type annotation");

        // Type annotation should NOT include "= $props()" or anything after
        assert!(
            !type_ann.contains("$props"),
            "Type annotation should not contain $props. Got:\n{}",
            type_ann
        );
        assert!(
            !type_ann.contains("$state"),
            "Type annotation should not contain $state. Got:\n{}",
            type_ann
        );
        assert!(
            !type_ann.contains("dragover"),
            "Type annotation should not contain script content. Got:\n{}",
            type_ann
        );

        // It should end with the closing brace of the type
        let trimmed = type_ann.trim();
        assert!(
            trimmed.ends_with('}'),
            "Type annotation should end with }}. Got:\n{}",
            type_ann
        );
    }

    #[test]
    fn test_extract_type_annotation_trajectory_full() {
        // The exact content after closing brace of destructuring in Trajectory.svelte
        let input = r#": EventHandlers & HTMLAttributes<HTMLDivElement> & {
    // trajectory data - can be provided directly or loaded from file
    trajectory?: TrajectoryType
    // URL to load trajectory from (alternative to providing trajectory directly)
    data_url?: string
    // current step index being displayed
    current_step_idx?: number
    // custom function to extract plot data from trajectory frames
    data_extractor?: TrajectoryDataExtractor

    // file drop handlers
    allow_file_drop?: boolean
    // layout configuration - 'auto' (default) adapts to element size, 'horizontal'/'vertical' forces layout
    layout?: `auto` | Orientation
    // structure viewer props (passed to Structure component)
    structure_props?: ComponentProps<typeof Structure>
    // plot props (passed to ScatterPlot component)
    scatter_props?: ComponentProps<typeof ScatterPlot>
    // histogram props (passed to Histogram component, excluding series which is handled separately)
    histogram_props?: Omit<ComponentProps<typeof Histogram>, `series`>
    // spinner props (passed to Spinner component)
    spinner_props?: ComponentProps<typeof Spinner>
    // custom snippets for additional UI elements
    trajectory_controls?: Snippet<[ControlsProps]>
    // Custom error snippet for advanced error handling
    error_snippet?: Snippet<[{ error_msg: string; on_dismiss: () => void }]>
    // Controls visibility configuration.
    // - 'always': controls always visible
    // - 'hover': controls visible on component hover (default)
    // - 'never': controls never visible
    // - object: { mode, hidden, style } for fine-grained control
    // Control names: 'filename', 'nav', 'step', 'fps', 'info-pane', 'export-pane', 'view-mode', 'fullscreen'
    show_controls?: ShowControlsProp
    // show/hide the fullscreen button
    fullscreen_toggle?: Snippet<[{ fullscreen: boolean }]> | boolean
    // automatically start playing when trajectory data is loaded
    auto_play?: boolean
    // display mode: 'structure+scatter' (default), 'structure' (only structure), 'scatter' (only scatter), 'histogram' (only histogram), 'structure+histogram' (structure with histogram)
    display_mode?:
      | `structure+scatter`
      | `structure`
      | `scatter`
      | `histogram`
      | `structure+histogram`
    // step labels configuration for slider
    // - positive number: number of evenly spaced ticks
    // - negative number: spacing between ticks (e.g. -10 = every 10th step)
    // - array: exact step indices to label
    // - undefined: no labels
    step_labels?: number | number[]
    // visible properties - bindable array of property keys currently shown in the plot
    // - controls which trajectory properties are plotted (e.g. ['energy', 'volume', 'force_max'])
    // - bindable: reflects current visibility state and can be used for external control
    // - if not provided, uses default visible properties (energy, force_max, stress_frobenius)
    // - if specified properties don't exist in data, falls back to automatic selection
    visible_properties?: string[]
    // custom labels for trajectory properties - maps property keys to display labels
    // - e.g. {energy: 'Total Energy', volume: 'Cell Volume', force_max: 'Max Force'}
    // - merged with built-in trajectory_property_config
    ELEM_PROPERTY_LABELS?: Record<string, string>
    // units configuration - developers can override these (deprecated - use ELEM_PROPERTY_LABELS instead)
    units?: {
      energy?: string
      energy_per_atom?: string
      force_max?: string
      force_norm?: string
      stress_max?: string
      volume?: string
      density?: string
      temperature?: string
      pressure?: string
      length?: string
      a?: string
      b?: string
      c?: string
      [key: string]: string | undefined
    }
    fps_range?: [number, number] // allowed FPS range [min_fps, max_fps]
    fps?: number // frame rate for playback
    // Loading options for large files
    loading_options?: LoadingOptions
    // Map LAMMPS atom types to element symbols (e.g. {1: 'Na', 2: 'Cl'})
    atom_type_mapping?: AtomTypeMapping
    // Disable plot skimming (mouse over plot doesn't update structure/step slider)
    plot_skimming?: boolean
  } = $props()

  let dragover = $state(false)"#;

        let result = extract_type_annotation(input);
        assert!(result.is_some(), "Should find type annotation");

        let type_ann = result.unwrap();
        assert!(
            !type_ann.contains("$props"),
            "Should not contain $props. Got:\n{}",
            type_ann
        );
        assert!(
            !type_ann.contains("$state"),
            "Should not contain $state. Got:\n{}",
            type_ann
        );
        assert!(
            type_ann.trim().ends_with('}'),
            "Should end with }}. Got:\n{}",
            type_ann
        );
    }

    #[test]
    fn test_comment_with_equals() {
        // Test with a comment containing =
        let input = r#": {
    // spacing between ticks (e.g. -10 = every 10th step)
    step_labels?: number
  } = $props()"#;

        let result = extract_type_annotation(input);
        assert!(result.is_some(), "Should find type annotation");
        let t = result.unwrap();
        assert!(!t.contains("$props"), "Should not contain $props");
    }
}
