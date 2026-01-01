//! Rune transformation logic.
//!
//! Transforms Svelte 5 runes into their TypeScript equivalents for type-checking.

use smol_str::SmolStr;

/// Information about a rune found in the script.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Scaffolding for future use
pub struct RuneInfo {
    /// The kind of rune.
    pub kind: RuneKind,
    /// The variable name being assigned.
    pub variable_name: SmolStr,
    /// The original expression text.
    pub original: String,
    /// The transformed expression.
    pub transformed: String,
}

/// The kind of rune.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Scaffolding for future use
pub enum RuneKind {
    /// `$props()`
    Props,
    /// `$state(init)`
    State,
    /// `$state.raw(init)`
    StateRaw,
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

/// Transforms rune expressions in script content.
///
/// This function identifies rune calls and transforms them for type-checking:
/// - `$state(init)` → `init`
/// - `$derived(expr)` → `(expr)`
/// - `$effect(() => {...})` → `(() => {...})()`
pub fn transform_runes(script: &str) -> (String, Vec<RuneInfo>) {
    let mut result = script.to_string();
    let mut runes = Vec::new();

    // Transform $state(init) → init
    result = transform_state(&result, &mut runes);

    // Transform $state.raw(init) → init
    result = transform_state_raw(&result, &mut runes);

    // Transform $derived(expr) → (expr)
    result = transform_derived(&result, &mut runes);

    // Transform $derived.by(fn) → fn()
    result = transform_derived_by(&result, &mut runes);

    // Transform $effect(() => {...}) → (() => {...})()
    result = transform_effect(&result, &mut runes);

    // Transform $effect.pre(() => {...}) → (() => {...})()
    result = transform_effect_pre(&result, &mut runes);

    // $props() is kept as-is for now (handled during type extraction)
    // $bindable() is kept as-is for now
    // $inspect() is removed or transformed to no-op
    result = transform_inspect(&result, &mut runes);

    // $host() → this
    result = transform_host(&result, &mut runes);

    (result, runes)
}

fn transform_state(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    // Simple pattern matching for $state(...)
    // A full implementation would use proper parsing
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(5).collect();
            if rest == "state" {
                // Check if it's $state( not $state.
                let after: String = chars.clone().skip(5).take(1).collect();
                if after == "(" {
                    // Skip "state("
                    for _ in 0..6 {
                        chars.next();
                    }
                    // Find matching closing paren and extract content
                    let mut depth = 1;
                    let mut content = String::new();
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                            content.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    result.push_str(&content);
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_state_raw(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(9).collect();
            if rest == "state.raw" {
                let after: String = chars.clone().skip(9).take(1).collect();
                if after == "(" {
                    for _ in 0..10 {
                        chars.next();
                    }
                    let mut depth = 1;
                    let mut content = String::new();
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                            content.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    result.push_str(&content);
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_derived(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(7).collect();
            if rest == "derived" {
                let after: String = chars.clone().skip(7).take(1).collect();
                if after == "(" {
                    for _ in 0..8 {
                        chars.next();
                    }
                    let mut depth = 1;
                    let mut content = String::new();
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                            content.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    result.push('(');
                    result.push_str(&content);
                    result.push(')');
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_derived_by(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(10).collect();
            if rest == "derived.by" {
                let after: String = chars.clone().skip(10).take(1).collect();
                if after == "(" {
                    for _ in 0..11 {
                        chars.next();
                    }
                    let mut depth = 1;
                    let mut content = String::new();
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                            content.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    result.push('(');
                    result.push_str(&content);
                    result.push_str(")()");
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_effect(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(6).collect();
            if rest == "effect" {
                let after: String = chars.clone().skip(6).take(1).collect();
                if after == "(" {
                    for _ in 0..7 {
                        chars.next();
                    }
                    let mut depth = 1;
                    let mut content = String::new();
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                            content.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    result.push('(');
                    result.push_str(&content);
                    result.push_str(")()");
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_effect_pre(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(10).collect();
            if rest == "effect.pre" {
                let after: String = chars.clone().skip(10).take(1).collect();
                if after == "(" {
                    for _ in 0..11 {
                        chars.next();
                    }
                    let mut depth = 1;
                    let mut content = String::new();
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                            content.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    result.push('(');
                    result.push_str(&content);
                    result.push_str(")()");
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_inspect(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    // Replace $inspect(...) with void 0
    let mut result = String::new();
    let mut chars = script.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let rest: String = chars.clone().take(7).collect();
            if rest == "inspect" {
                let after: String = chars.clone().skip(7).take(1).collect();
                if after == "(" {
                    for _ in 0..8 {
                        chars.next();
                    }
                    let mut depth = 1;
                    for c in chars.by_ref() {
                        if c == '(' {
                            depth += 1;
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    result.push_str("void 0");
                    continue;
                }
            }
        }
        result.push(c);
    }

    result
}

fn transform_host(script: &str, _runes: &mut Vec<RuneInfo>) -> String {
    script.replace("$host()", "this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_state() {
        let (result, _) = transform_runes("let count = $state(0);");
        assert_eq!(result, "let count = 0;");
    }

    #[test]
    fn test_transform_state_with_object() {
        let (result, _) = transform_runes("let obj = $state({ a: 1 });");
        assert_eq!(result, "let obj = { a: 1 };");
    }

    #[test]
    fn test_transform_derived() {
        let (result, _) = transform_runes("let double = $derived(count * 2);");
        assert_eq!(result, "let double = (count * 2);");
    }

    #[test]
    fn test_transform_effect() {
        let (result, _) = transform_runes("$effect(() => console.log(count));");
        assert_eq!(result, "(() => console.log(count))();");
    }

    #[test]
    fn test_transform_host() {
        let (result, _) = transform_runes("$host().dispatchEvent(event);");
        assert_eq!(result, "this.dispatchEvent(event);");
    }

    #[test]
    fn test_transform_inspect() {
        let (result, _) = transform_runes("$inspect(value);");
        assert_eq!(result, "void 0;");
    }
}
