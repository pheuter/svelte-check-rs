//! Accessibility (a11y) checks.
//!
//! This module implements accessibility checks for Svelte templates,
//! matching the behavior of svelte-check.

pub mod aria_data;
pub mod rules;

use crate::{Diagnostic, DiagnosticCode};
use svelte_parser::{Attribute, Element, SvelteDocument, TemplateNode};

/// Runs all a11y checks on a document.
pub fn check(doc: &SvelteDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut heading_levels = Vec::new();
    check_fragment(&doc.fragment.nodes, &mut diagnostics, &mut heading_levels);
    diagnostics
}

/// Recursively checks template nodes.
fn check_fragment(
    nodes: &[TemplateNode],
    diagnostics: &mut Vec<Diagnostic>,
    heading_levels: &mut Vec<(u8, source_map::Span)>,
) {
    for node in nodes {
        match node {
            TemplateNode::Element(el) => {
                check_element(el, diagnostics, heading_levels);
                check_fragment(&el.children, diagnostics, heading_levels);
            }
            TemplateNode::Component(comp) => {
                check_fragment(&comp.children, diagnostics, heading_levels);
            }
            TemplateNode::SvelteElement(el) => {
                check_fragment(&el.children, diagnostics, heading_levels);
            }
            TemplateNode::IfBlock(block) => {
                check_fragment(&block.consequent.nodes, diagnostics, heading_levels);
                if let Some(alt) = &block.alternate {
                    match alt {
                        svelte_parser::ElseBranch::Else(frag) => {
                            check_fragment(&frag.nodes, diagnostics, heading_levels);
                        }
                        svelte_parser::ElseBranch::ElseIf(elif) => {
                            check_fragment(&elif.consequent.nodes, diagnostics, heading_levels);
                        }
                    }
                }
            }
            TemplateNode::EachBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics, heading_levels);
                if let Some(fallback) = &block.fallback {
                    check_fragment(&fallback.nodes, diagnostics, heading_levels);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                if let Some(pending) = &block.pending {
                    check_fragment(&pending.nodes, diagnostics, heading_levels);
                }
                if let Some(then) = &block.then {
                    check_fragment(&then.body.nodes, diagnostics, heading_levels);
                }
                if let Some(catch) = &block.catch {
                    check_fragment(&catch.body.nodes, diagnostics, heading_levels);
                }
            }
            TemplateNode::KeyBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics, heading_levels);
            }
            TemplateNode::SnippetBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics, heading_levels);
            }
            _ => {}
        }
    }
}

/// Checks a single element for a11y issues.
fn check_element(
    el: &Element,
    diagnostics: &mut Vec<Diagnostic>,
    heading_levels: &mut Vec<(u8, source_map::Span)>,
) {
    let tag = el.name.as_str();
    let attrs = collect_attributes(&el.attributes);

    // === Existing Rules ===

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

    // === New Priority Rules ===

    // a11y-aria-attributes: validate aria-* attribute names
    for (name, _) in &attrs {
        if name.starts_with("aria-") && !aria_data::is_valid_aria_attribute(name) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::A11yAriaAttributes,
                format!("A11y: Unknown ARIA attribute '{}'", name),
                el.span,
            ));
        }
    }

    // a11y-no-redundant-roles: check for redundant role attributes
    if let Some(role) = get_attribute_value(&el.attributes, "role") {
        if aria_data::is_redundant_role(tag, &role) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::A11yNoRedundantRoles,
                format!(
                    "A11y: <{}> has an implicit role of '{}'. Avoid redundant role",
                    tag, role
                ),
                el.span,
            ));
        }
    }

    // a11y-role-has-required-aria-props
    if let Some(role) = get_attribute_value(&el.attributes, "role") {
        let required_props = aria_data::get_required_aria_props(&role);
        for prop in required_props {
            if !has_attribute(&el.attributes, prop) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::A11yRoleHasRequiredAriaProps,
                    format!("A11y: Elements with role='{}' must have '{}'", role, prop),
                    el.span,
                ));
            }
        }
    }

    // a11y-click-events-have-key-events
    let has_click = attrs.iter().any(|(n, _)| n == "onclick" || n == "on:click");
    let has_key = attrs.iter().any(|(n, _)| {
        matches!(
            n.as_str(),
            "onkeydown" | "on:keydown" | "onkeyup" | "on:keyup" | "onkeypress" | "on:keypress"
        )
    });

    if has_click
        && !has_key
        && aria_data::is_non_interactive_element(tag)
        && !has_interactive_role(&el.attributes)
    {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yClickEventsHaveKeyEvents,
            "A11y: Click events must be accompanied by a keyboard handler (onkeydown or onkeyup)",
            el.span,
        ));
    }

    // a11y-mouse-events-have-key-events
    let has_mouse_over = attrs
        .iter()
        .any(|(n, _)| n == "onmouseover" || n == "on:mouseover");
    let has_mouse_out = attrs
        .iter()
        .any(|(n, _)| n == "onmouseout" || n == "on:mouseout");
    let has_focus = attrs.iter().any(|(n, _)| n == "onfocus" || n == "on:focus");
    let has_blur = attrs.iter().any(|(n, _)| n == "onblur" || n == "on:blur");

    if has_mouse_over && !has_focus {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yMouseEventsHaveKeyEvents,
            "A11y: onmouseover must be accompanied by onfocus",
            el.span,
        ));
    }

    if has_mouse_out && !has_blur {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yMouseEventsHaveKeyEvents,
            "A11y: onmouseout must be accompanied by onblur",
            el.span,
        ));
    }

    // a11y-no-noninteractive-tabindex
    if let Some(tabindex) = get_attribute_value(&el.attributes, "tabindex") {
        if let Ok(value) = tabindex.parse::<i32>() {
            if value >= 0
                && aria_data::is_non_interactive_element(tag)
                && !has_interactive_role(&el.attributes)
            {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::A11yNoNoninteractiveTabindex,
                    format!(
                        "A11y: Non-interactive elements like <{}> should not have tabindex",
                        tag
                    ),
                    el.span,
                ));
            }
        }
    }

    // a11y-no-static-element-interactions
    if has_click
        && !has_attribute(&el.attributes, "role")
        && matches!(
            tag,
            "div" | "span" | "section" | "article" | "main" | "aside" | "header" | "footer"
        )
    {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yNoStaticElementInteractions,
            format!(
                "A11y: <{}> with click handler must have a role attribute",
                tag
            ),
            el.span,
        ));
    }

    // a11y-interactive-supports-focus
    if has_click
        && !has_attribute(&el.attributes, "tabindex")
        && aria_data::is_non_interactive_element(tag)
        && has_interactive_role(&el.attributes)
    {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::A11yInteractiveSupportsFocus,
            "A11y: Interactive elements must be focusable (add tabindex)",
            el.span,
        ));
    }

    // a11y-label-has-associated-control
    if tag == "label" {
        let has_for =
            has_attribute(&el.attributes, "for") || has_attribute(&el.attributes, "htmlFor");
        let has_nested_control = el.children.iter().any(|child| {
            if let TemplateNode::Element(inner) = child {
                matches!(
                    inner.name.as_str(),
                    "input" | "select" | "textarea" | "button"
                )
            } else {
                false
            }
        });

        if !has_for && !has_nested_control {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::A11yLabelHasAssociatedControl,
                "A11y: <label> must have an associated control (use 'for' attribute or nest a control)",
                el.span,
            ));
        }
    }

    // a11y-structure: heading levels
    if let Some(level) = get_heading_level(tag) {
        heading_levels.push((level, el.span));

        // Check for skipped levels
        if let Some(&(prev_level, _)) = heading_levels.iter().rev().nth(1) {
            if level > prev_level + 1 {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::A11yStructure,
                    format!(
                        "A11y: Heading levels should not be skipped (h{} followed by h{})",
                        prev_level, level
                    ),
                    el.span,
                ));
            }
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

/// Collects all attributes as (name, value) pairs.
fn collect_attributes(attrs: &[Attribute]) -> Vec<(String, Option<String>)> {
    attrs
        .iter()
        .filter_map(|attr| match attr {
            Attribute::Normal(a) => {
                let value = match &a.value {
                    svelte_parser::AttributeValue::Text(t) => Some(t.value.clone()),
                    svelte_parser::AttributeValue::True => None,
                    _ => None,
                };
                Some((a.name.to_string(), value))
            }
            Attribute::Directive(d) => {
                let kind = match d.kind {
                    svelte_parser::DirectiveKind::On => "on",
                    svelte_parser::DirectiveKind::Bind => "bind",
                    svelte_parser::DirectiveKind::Class => "class",
                    svelte_parser::DirectiveKind::StyleDirective => "style",
                    svelte_parser::DirectiveKind::Use => "use",
                    svelte_parser::DirectiveKind::Transition => "transition",
                    svelte_parser::DirectiveKind::In => "in",
                    svelte_parser::DirectiveKind::Out => "out",
                    svelte_parser::DirectiveKind::Animate => "animate",
                    svelte_parser::DirectiveKind::Let => "let",
                };
                Some((format!("{}:{}", kind, d.name), None))
            }
            _ => None,
        })
        .collect()
}

/// Checks if element has an interactive role.
fn has_interactive_role(attrs: &[Attribute]) -> bool {
    get_attribute_value(attrs, "role")
        .map(|role| aria_data::is_interactive_role(&role))
        .unwrap_or(false)
}

/// Gets the heading level (1-6) for h1-h6 elements.
fn get_heading_level(tag: &str) -> Option<u8> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
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

    #[test]
    fn test_invalid_aria_attribute() {
        let doc = parse(r#"<div aria-foo="bar"></div>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yAriaAttributes)));
    }

    #[test]
    fn test_valid_aria_attribute() {
        let doc = parse(r#"<div aria-label="test"></div>"#).document;
        let diagnostics = check(&doc);
        assert!(!diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yAriaAttributes)));
    }

    #[test]
    fn test_redundant_role() {
        let doc = parse(r#"<button role="button">Click</button>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yNoRedundantRoles)));
    }

    #[test]
    fn test_role_missing_required_aria_props() {
        let doc = parse(r#"<div role="slider"></div>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yRoleHasRequiredAriaProps)));
    }

    #[test]
    fn test_role_with_required_aria_props() {
        let doc = parse(r#"<div role="slider" aria-valuenow="50"></div>"#).document;
        let diagnostics = check(&doc);
        assert!(!diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yRoleHasRequiredAriaProps)));
    }

    #[test]
    fn test_label_without_control() {
        let doc = parse(r#"<label>Name</label>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yLabelHasAssociatedControl)));
    }

    #[test]
    fn test_label_with_for() {
        let doc = parse(r#"<label for="name">Name</label>"#).document;
        let diagnostics = check(&doc);
        assert!(!diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yLabelHasAssociatedControl)));
    }

    #[test]
    fn test_label_with_nested_input() {
        let doc = parse(r#"<label>Name <input type="text"></label>"#).document;
        let diagnostics = check(&doc);
        assert!(!diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yLabelHasAssociatedControl)));
    }

    #[test]
    fn test_heading_structure_valid() {
        let doc = parse(r#"<h1>Title</h1><h2>Section</h2><h3>Subsection</h3>"#).document;
        let diagnostics = check(&doc);
        assert!(!diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yStructure)));
    }

    #[test]
    fn test_heading_structure_skipped() {
        let doc = parse(r#"<h1>Title</h1><h3>Skipped h2</h3>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::A11yStructure)));
    }
}
