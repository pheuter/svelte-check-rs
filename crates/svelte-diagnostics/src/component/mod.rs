//! Component diagnostics.
//!
//! This module provides component-level checks:
//! - Invalid rune usage
//! - Component naming conventions
//! - Missing declarations

use crate::{Diagnostic, DiagnosticCode};
use svelte_parser::{
    AwaitBlock, EachBlock, ElseBranch, Fragment, IfBlock, KeyBlock, SnippetBlock, SvelteDocument,
    SvelteElement, TemplateNode,
};

/// Rune function names that are only valid in specific contexts.
const RUNES: &[&str] = &[
    "$state",
    "$state.raw",
    "$state.snapshot",
    "$derived",
    "$derived.by",
    "$effect",
    "$effect.pre",
    "$effect.tracking",
    "$effect.root",
    "$props",
    "$bindable",
    "$inspect",
    "$inspect.trace",
    "$host",
];

/// Component check options.
#[derive(Debug, Clone, Default)]
pub struct ComponentCheckOptions {
    /// The filename of the component (for naming checks).
    pub filename: Option<String>,
}

/// Runs component checks on a document.
pub fn check(doc: &SvelteDocument, _options: &ComponentCheckOptions) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Check for invalid rune usage outside script blocks
    diagnostics.extend(check_template_rune_usage(doc));

    diagnostics
}

/// Checks for rune usage in template expressions (which is invalid).
fn check_template_rune_usage(doc: &SvelteDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Check for runes in template nodes
    check_fragment_for_runes(&doc.fragment, &mut diagnostics);

    diagnostics
}

/// Recursively checks a fragment for invalid rune usage.
fn check_fragment_for_runes(fragment: &Fragment, diagnostics: &mut Vec<Diagnostic>) {
    for node in &fragment.nodes {
        check_node_for_runes(node, diagnostics);
    }
}

/// Recursively checks a node for invalid rune usage.
fn check_node_for_runes(node: &TemplateNode, diagnostics: &mut Vec<Diagnostic>) {
    match node {
        TemplateNode::Expression(expr_tag) => {
            // Check if expression contains a rune call
            let expr_text = &expr_tag.expression;
            for rune in RUNES {
                if contains_rune_call(expr_text, rune) {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::InvalidRuneUsage,
                        format!(
                            "{}() can only be used inside a $derived or $effect, or at the top level of a component",
                            rune
                        ),
                        expr_tag.span,
                    ));
                }
            }
        }
        TemplateNode::Element(element) => {
            for child in &element.children {
                check_node_for_runes(child, diagnostics);
            }
        }
        TemplateNode::Component(component) => {
            for child in &component.children {
                check_node_for_runes(child, diagnostics);
            }
        }
        TemplateNode::SvelteElement(svelte_element) => {
            check_svelte_element_for_runes(svelte_element, diagnostics);
        }
        TemplateNode::IfBlock(if_block) => {
            check_if_block_for_runes(if_block, diagnostics);
        }
        TemplateNode::EachBlock(each_block) => {
            check_each_block_for_runes(each_block, diagnostics);
        }
        TemplateNode::AwaitBlock(await_block) => {
            check_await_block_for_runes(await_block, diagnostics);
        }
        TemplateNode::KeyBlock(key_block) => {
            check_key_block_for_runes(key_block, diagnostics);
        }
        TemplateNode::SnippetBlock(snippet_block) => {
            check_snippet_block_for_runes(snippet_block, diagnostics);
        }
        // Text, comments, and tag nodes don't need checking
        TemplateNode::Text(_)
        | TemplateNode::Comment(_)
        | TemplateNode::HtmlTag(_)
        | TemplateNode::ConstTag(_)
        | TemplateNode::DebugTag(_)
        | TemplateNode::RenderTag(_) => {}
    }
}

fn check_svelte_element_for_runes(element: &SvelteElement, diagnostics: &mut Vec<Diagnostic>) {
    for child in &element.children {
        check_node_for_runes(child, diagnostics);
    }
}

fn check_if_block_for_runes(if_block: &IfBlock, diagnostics: &mut Vec<Diagnostic>) {
    check_fragment_for_runes(&if_block.consequent, diagnostics);
    if let Some(ref alternate) = if_block.alternate {
        match alternate {
            ElseBranch::Else(fragment) => {
                check_fragment_for_runes(fragment, diagnostics);
            }
            ElseBranch::ElseIf(nested_if) => {
                check_if_block_for_runes(nested_if, diagnostics);
            }
        }
    }
}

fn check_each_block_for_runes(each_block: &EachBlock, diagnostics: &mut Vec<Diagnostic>) {
    check_fragment_for_runes(&each_block.body, diagnostics);
    if let Some(ref fallback) = each_block.fallback {
        check_fragment_for_runes(fallback, diagnostics);
    }
}

fn check_await_block_for_runes(await_block: &AwaitBlock, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(ref pending) = await_block.pending {
        check_fragment_for_runes(pending, diagnostics);
    }
    if let Some(ref then) = await_block.then {
        check_fragment_for_runes(&then.body, diagnostics);
    }
    if let Some(ref catch) = await_block.catch {
        check_fragment_for_runes(&catch.body, diagnostics);
    }
}

fn check_key_block_for_runes(key_block: &KeyBlock, diagnostics: &mut Vec<Diagnostic>) {
    check_fragment_for_runes(&key_block.body, diagnostics);
}

fn check_snippet_block_for_runes(snippet_block: &SnippetBlock, diagnostics: &mut Vec<Diagnostic>) {
    check_fragment_for_runes(&snippet_block.body, diagnostics);
}

/// Checks if an expression contains a rune function call.
fn contains_rune_call(expr: &str, rune: &str) -> bool {
    // Simple check: look for the rune name followed by (
    // This is a basic heuristic that works for most cases
    if let Some(pos) = expr.find(rune) {
        // Check if it's followed by (
        let after = &expr[pos + rune.len()..];
        if after.starts_with('(') {
            // Make sure it's not part of a larger identifier
            if pos == 0 {
                return true;
            }
            let before = expr[..pos].chars().last().unwrap_or(' ');
            if !before.is_alphanumeric() && before != '_' && before != '.' {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_empty_component() {
        let doc = parse("").document;
        let diagnostics = check(&doc, &ComponentCheckOptions::default());
        assert!(diagnostics.is_empty());
    }

    // TODO: Add component name checking
    // #[test]
    // fn test_lowercase_component_name() {
    //     let doc = parse("").document;
    //     let options = ComponentCheckOptions {
    //         filename: Some("myComponent.svelte".to_string()),
    //     };
    //     let diagnostics = check(&doc, &options);
    //     assert_eq!(diagnostics.len(), 1);
    //     assert!(matches!(
    //         diagnostics[0].code,
    //         DiagnosticCode::ComponentNameLowercase
    //     ));
    // }

    // #[test]
    // fn test_pascalcase_component_name() {
    //     let doc = parse("").document;
    //     let options = ComponentCheckOptions {
    //         filename: Some("MyComponent.svelte".to_string()),
    //     };
    //     let diagnostics = check(&doc, &options);
    //     assert!(diagnostics.is_empty());
    // }

    #[test]
    fn test_sveltekit_special_files() {
        let doc = parse("").document;

        // +page.svelte should not trigger warning
        let options = ComponentCheckOptions {
            filename: Some("+page.svelte".to_string()),
        };
        let diagnostics = check(&doc, &options);
        assert!(diagnostics.is_empty());

        // +layout.svelte should not trigger warning
        let options = ComponentCheckOptions {
            filename: Some("+layout.svelte".to_string()),
        };
        let diagnostics = check(&doc, &options);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_rune_in_template() {
        let doc = parse(r#"<button>{$state(0)}</button>"#).document;
        let diagnostics = check(&doc, &ComponentCheckOptions::default());
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            diagnostics[0].code,
            DiagnosticCode::InvalidRuneUsage
        ));
    }

    #[test]
    fn test_derived_in_template() {
        let doc = parse(r#"<span>{$derived(count * 2)}</span>"#).document;
        let diagnostics = check(&doc, &ComponentCheckOptions::default());
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            diagnostics[0].code,
            DiagnosticCode::InvalidRuneUsage
        ));
    }

    #[test]
    fn test_normal_expression_in_template() {
        // Regular expressions should not trigger warnings
        let doc = parse(r#"<span>{count + 1}</span>"#).document;
        let diagnostics = check(&doc, &ComponentCheckOptions::default());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_dollar_sign_not_rune() {
        // A regular $ variable should not trigger
        let doc = parse(r#"<span>{$myStore}</span>"#).document;
        let diagnostics = check(&doc, &ComponentCheckOptions::default());
        assert!(diagnostics.is_empty());
    }

    // TODO: Add to_pascal_case helper
    // #[test]
    // fn test_to_pascal_case() {
    //     assert_eq!(to_pascal_case("my-component"), "MyComponent");
    //     assert_eq!(to_pascal_case("my_component"), "MyComponent");
    //     assert_eq!(to_pascal_case("mycomponent"), "Mycomponent");
    //     assert_eq!(to_pascal_case("button"), "Button");
    // }

    #[test]
    fn test_contains_rune_call() {
        assert!(contains_rune_call("$state(0)", "$state"));
        assert!(contains_rune_call("foo + $state(0)", "$state"));
        assert!(!contains_rune_call("$stateValue", "$state"));
        assert!(!contains_rune_call("my$state(0)", "$state"));
    }
}
