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
    /// The property name.
    pub name: String,
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
    // Find $props in the original script - it could be $props() or $props<Type>()
    let props_idx = original_script.find("$props")?;

    // Verify it's actually a $props call (followed by `(` or `<`)
    let after_props = &original_script[props_idx + 6..];
    if !after_props.starts_with('(') && !after_props.starts_with('<') {
        return None;
    }

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

    // Check if name contains type annotation: `prop: Type`
    let (final_name, type_ann) = if let Some(colon_idx) = name.find(':') {
        let n = name[..colon_idx].trim();
        let t = name[colon_idx + 1..].trim();
        (n, Some(t.to_string()))
    } else {
        (name, None)
    };

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

    // Simple heuristic: find `=` that's not inside angle brackets or parens
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, ch) in type_str.char_indices() {
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

        if ch == '<' || ch == '(' || ch == '{' || ch == '[' {
            depth += 1;
        } else if ch == '>' || ch == ')' || ch == '}' || ch == ']' {
            depth -= 1;
        } else if ch == '=' && depth == 0 {
            return Some(type_str[..i].trim().to_string());
        }
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

    for (i, ch) in after_props.char_indices() {
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

        if ch == '<' {
            depth += 1;
        } else if ch == '>' {
            depth -= 1;
            if depth == 0 {
                return Some(after_props[..i].trim().to_string());
            }
        }
    }

    None
}

/// Generate a TypeScript type string for the props.
pub fn generate_props_type(info: &PropsInfo) -> String {
    // If we have an explicit type annotation, use it
    if let Some(ref type_ann) = info.type_annotation {
        return type_ann.clone();
    }

    // If not destructured, we can't infer the type
    if !info.is_destructured {
        return "Record<string, unknown>".to_string();
    }

    // Generate type from properties
    let mut type_parts = Vec::new();

    for prop in &info.properties {
        if prop.is_rest {
            // Rest properties contribute to the type
            continue;
        }

        let optional = if prop.default_value.is_some() || prop.is_bindable {
            "?"
        } else {
            ""
        };

        let prop_type = prop.type_annotation.as_deref().unwrap_or("unknown");

        type_parts.push(format!("{}{}: {}", prop.name, optional, prop_type));
    }

    if type_parts.is_empty() {
        "{}".to_string()
    } else {
        format!("{{ {} }}", type_parts.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    span: Span::default(),
                    is_bindable: false,
                    default_value: None,
                    type_annotation: Some("number".to_string()),
                    is_rest: false,
                },
                PropProperty {
                    name: "label".to_string(),
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
}
