//! Helpers for parsing snippet names with optional generic parameters.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnippetNameParts {
    pub(crate) base: String,
    pub(crate) generics: Option<String>,
}

pub(crate) fn split_snippet_name(name: &str) -> SnippetNameParts {
    let trimmed = name.trim();
    let mut chars = trimmed.char_indices().peekable();
    let mut in_string = false;
    let mut string_char = '\0';
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut angle_depth: i32 = 0;
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    let bytes = trimmed.as_bytes();
    let mut prev_char: Option<char> = None;

    while let Some((i, ch)) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            prev_char = Some(ch);
            continue;
        }

        if in_block_comment {
            if ch == '*' {
                if let Some((_, '/')) = chars.peek() {
                    chars.next();
                    in_block_comment = false;
                    prev_char = Some('/');
                    continue;
                }
            }
            prev_char = Some(ch);
            continue;
        }

        if in_string {
            let is_escaped = i > 0 && bytes.get(i - 1) == Some(&b'\\');
            if ch == string_char && !is_escaped {
                in_string = false;
            }
            prev_char = Some(ch);
            continue;
        }

        if ch == '/' {
            if let Some((_, next)) = chars.peek() {
                if *next == '/' {
                    chars.next();
                    in_line_comment = true;
                    prev_char = Some('/');
                    continue;
                }
                if *next == '*' {
                    chars.next();
                    in_block_comment = true;
                    prev_char = Some('*');
                    continue;
                }
            }
        }

        match ch {
            '"' | '\'' | '`' => {
                in_string = true;
                string_char = ch;
            }
            '<' => {
                if angle_depth == 0 {
                    start = Some(i);
                }
                angle_depth += 1;
            }
            '>' => {
                if angle_depth > 0 && prev_char != Some('=') {
                    angle_depth -= 1;
                    if angle_depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
            }
            _ => {}
        }

        prev_char = Some(ch);
    }

    if let (Some(start), Some(end)) = (start, end) {
        let base = trimmed[..start].trim();
        if base.is_empty() {
            return SnippetNameParts {
                base: trimmed.to_string(),
                generics: None,
            };
        }
        let generics = trimmed[start..=end].to_string();
        SnippetNameParts {
            base: base.to_string(),
            generics: Some(generics),
        }
    } else {
        SnippetNameParts {
            base: trimmed.to_string(),
            generics: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_snippet_name_without_generics() {
        let parts = split_snippet_name("item");
        assert_eq!(parts.base, "item");
        assert!(parts.generics.is_none());
    }

    #[test]
    fn split_snippet_name_with_generics() {
        let parts = split_snippet_name("item<T extends { id: string }>");
        assert_eq!(parts.base, "item");
        assert_eq!(
            parts.generics.as_deref(),
            Some("<T extends { id: string }>")
        );
    }

    #[test]
    fn split_snippet_name_with_nested_generics() {
        let parts = split_snippet_name("item<T extends Foo<Bar<Baz>>>");
        assert_eq!(parts.base, "item");
        assert_eq!(parts.generics.as_deref(), Some("<T extends Foo<Bar<Baz>>>"));
    }

    #[test]
    fn split_snippet_name_ignores_arrow_in_constraints() {
        let parts = split_snippet_name("item<T extends (value: string) => void>");
        assert_eq!(parts.base, "item");
        assert_eq!(
            parts.generics.as_deref(),
            Some("<T extends (value: string) => void>")
        );
    }

    #[test]
    fn split_snippet_name_preserves_multiline_generics() {
        let parts = split_snippet_name("item<\n  T extends {\n    id: string;\n  },\n>");
        assert_eq!(parts.base, "item");
        assert_eq!(
            parts.generics.as_deref(),
            Some("<\n  T extends {\n    id: string;\n  },\n>")
        );
    }
}
