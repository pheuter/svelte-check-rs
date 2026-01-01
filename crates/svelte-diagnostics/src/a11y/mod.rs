//! Accessibility (a11y) checks.
//!
//! This module implements accessibility checks for Svelte templates,
//! matching the behavior of svelte-check.

pub mod rules;

use crate::{Diagnostic, DiagnosticCode};
use svelte_parser::{Attribute, Element, SvelteDocument, TemplateNode};

/// Runs all a11y checks on a document.
pub fn check(doc: &SvelteDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_fragment(&doc.fragment.nodes, &mut diagnostics);
    diagnostics
}

/// Recursively checks template nodes.
fn check_fragment(nodes: &[TemplateNode], diagnostics: &mut Vec<Diagnostic>) {
    for node in nodes {
        match node {
            TemplateNode::Element(el) => {
                check_element(el, diagnostics);
                check_fragment(&el.children, diagnostics);
            }
            TemplateNode::Component(comp) => {
                check_fragment(&comp.children, diagnostics);
            }
            TemplateNode::SvelteElement(el) => {
                check_fragment(&el.children, diagnostics);
            }
            TemplateNode::IfBlock(block) => {
                check_fragment(&block.consequent.nodes, diagnostics);
                if let Some(alt) = &block.alternate {
                    match alt {
                        svelte_parser::ElseBranch::Else(frag) => {
                            check_fragment(&frag.nodes, diagnostics);
                        }
                        svelte_parser::ElseBranch::ElseIf(elif) => {
                            check_fragment(&elif.consequent.nodes, diagnostics);
                        }
                    }
                }
            }
            TemplateNode::EachBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics);
                if let Some(fallback) = &block.fallback {
                    check_fragment(&fallback.nodes, diagnostics);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                if let Some(pending) = &block.pending {
                    check_fragment(&pending.nodes, diagnostics);
                }
                if let Some(then) = &block.then {
                    check_fragment(&then.body.nodes, diagnostics);
                }
                if let Some(catch) = &block.catch {
                    check_fragment(&catch.body.nodes, diagnostics);
                }
            }
            TemplateNode::KeyBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics);
            }
            TemplateNode::SnippetBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics);
            }
            _ => {}
        }
    }
}

/// Checks a single element for a11y issues.
fn check_element(el: &Element, diagnostics: &mut Vec<Diagnostic>) {
    let tag = el.name.as_str();

    // a11y-missing-attribute: img requires alt
    if tag == "img" && !has_attribute(&el.attributes, "alt") {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yMissingAttribute,
            "A11y: <img> element should have an alt attribute",
            el.span,
        ));
    }

    // a11y-missing-attribute: area requires alt
    if tag == "area" && !has_attribute(&el.attributes, "alt") {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yMissingAttribute,
            "A11y: <area> element should have an alt attribute",
            el.span,
        ));
    }

    // a11y-missing-content: anchor needs content or aria-label
    if tag == "a"
        && el.children.is_empty()
        && !has_attribute(&el.attributes, "aria-label")
        && !has_attribute(&el.attributes, "aria-labelledby")
    {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yMissingContent,
            "A11y: <a> element should have child content",
            el.span,
        ));
    }

    // a11y-distracting-elements
    if tag == "marquee" || tag == "blink" {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yDistractingElements,
            format!("A11y: Avoid using <{}>", tag),
            el.span,
        ));
    }

    // a11y-autofocus
    if has_attribute(&el.attributes, "autofocus") {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yAutofocus,
            "A11y: Avoid using autofocus",
            el.span,
        ));
    }

    // a11y-accesskey
    if has_attribute(&el.attributes, "accesskey") {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yAccesskey,
            "A11y: Avoid using accesskey",
            el.span,
        ));
    }

    // a11y-positive-tabindex
    if let Some(tabindex) = get_attribute_value(&el.attributes, "tabindex") {
        if let Ok(value) = tabindex.parse::<i32>() {
            if value > 0 {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::A11yPositiveTabindex,
                    "A11y: Avoid positive tabindex values",
                    el.span,
                ));
            }
        }
    }

    // a11y-hidden: aria-hidden on focusable elements
    if has_attribute(&el.attributes, "aria-hidden") {
        let is_focusable = matches!(tag, "a" | "button" | "input" | "select" | "textarea")
            || has_attribute(&el.attributes, "tabindex");

        if is_focusable {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::A11yHidden,
                "A11y: aria-hidden should not be used on focusable elements",
                el.span,
            ));
        }
    }

    // a11y-img-redundant-alt
    if tag == "img" {
        if let Some(alt) = get_attribute_value(&el.attributes, "alt") {
            let alt_lower = alt.to_lowercase();
            if alt_lower.contains("image")
                || alt_lower.contains("picture")
                || alt_lower.contains("photo")
            {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::A11yImgRedundantAlt,
                    "A11y: Redundant alt text (avoid words like 'image', 'picture', or 'photo')",
                    el.span,
                ));
            }
        }
    }

    // a11y-media-has-caption
    if tag == "video" && !has_attribute(&el.attributes, "muted") {
        // Check for track element with kind="captions"
        let has_captions = el.children.iter().any(|child| {
            if let TemplateNode::Element(track) = child {
                track.name.as_str() == "track"
                    && get_attribute_value(&track.attributes, "kind")
                        .map(|k| k == "captions")
                        .unwrap_or(false)
            } else {
                false
            }
        });

        if !has_captions {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::A11yMediaHasCaption,
                "A11y: <video> elements should have a <track kind=\"captions\">",
                el.span,
            ));
        }
    }
}

/// Checks if an element has an attribute.
fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| match attr {
        Attribute::Normal(a) => a.name.as_str() == name,
        Attribute::Directive(d) => d.name.as_str() == name,
        _ => false,
    })
}

/// Gets the text value of an attribute.
fn get_attribute_value(attrs: &[Attribute], name: &str) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if let Attribute::Normal(a) = attr {
            if a.name.as_str() == name {
                match &a.value {
                    svelte_parser::AttributeValue::Text(t) => Some(t.value.clone()),
                    svelte_parser::AttributeValue::True => Some(String::new()),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_img_missing_alt() {
        let doc = parse(r#"<img src="photo.jpg">"#).document;
        let diagnostics = check(&doc);
        assert!(!diagnostics.is_empty());
        assert!(matches!(
            diagnostics[0].code,
            DiagnosticCode::A11yMissingAttribute
        ));
    }

    #[test]
    fn test_img_with_alt() {
        let doc = parse(r#"<img src="photo.jpg" alt="A photo">"#).document;
        let diagnostics = check(&doc);
        // Should not have missing attribute error
        assert!(!diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yMissingAttribute)));
    }

    #[test]
    fn test_distracting_elements() {
        let doc = parse("<marquee>text</marquee>").document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yDistractingElements)));
    }

    #[test]
    fn test_autofocus() {
        let doc = parse("<input autofocus>").document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yAutofocus)));
    }

    #[test]
    fn test_positive_tabindex() {
        let doc = parse(r#"<div tabindex="5"></div>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yPositiveTabindex)));
    }
}
