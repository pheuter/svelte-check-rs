//! Accessibility (a11y) checks.
//!
//! Currently only heading structure checks are implemented. All other a11y
//! warnings are provided by the Svelte compiler.

use crate::{Diagnostic, DiagnosticCode};
use svelte_parser::{ElseBranch, SvelteDocument, TemplateNode};

/// Runs a11y checks on a document.
pub fn check(doc: &SvelteDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut last_heading: Option<u8> = None;
    check_fragment(&doc.fragment.nodes, &mut diagnostics, &mut last_heading);
    diagnostics
}

fn check_fragment(
    nodes: &[TemplateNode],
    diagnostics: &mut Vec<Diagnostic>,
    last_heading: &mut Option<u8>,
) {
    for node in nodes {
        match node {
            TemplateNode::Element(el) => {
                if let Some(level) = get_heading_level(el.name.as_str()) {
                    if let Some(prev_level) = *last_heading {
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
                    *last_heading = Some(level);
                }

                check_fragment(&el.children, diagnostics, last_heading);
            }
            TemplateNode::Component(comp) => {
                check_fragment(&comp.children, diagnostics, last_heading);
            }
            TemplateNode::SvelteElement(el) => {
                check_fragment(&el.children, diagnostics, last_heading);
            }
            TemplateNode::IfBlock(block) => {
                check_if_block(block, diagnostics, last_heading);
            }
            TemplateNode::EachBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics, last_heading);
                if let Some(fallback) = &block.fallback {
                    check_fragment(&fallback.nodes, diagnostics, last_heading);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                if let Some(pending) = &block.pending {
                    check_fragment(&pending.nodes, diagnostics, last_heading);
                }
                if let Some(then) = &block.then {
                    check_fragment(&then.body.nodes, diagnostics, last_heading);
                }
                if let Some(catch) = &block.catch {
                    check_fragment(&catch.body.nodes, diagnostics, last_heading);
                }
            }
            TemplateNode::KeyBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics, last_heading);
            }
            TemplateNode::SnippetBlock(block) => {
                check_fragment(&block.body.nodes, diagnostics, last_heading);
            }
            TemplateNode::Text(_)
            | TemplateNode::Comment(_)
            | TemplateNode::Expression(_)
            | TemplateNode::HtmlTag(_)
            | TemplateNode::ConstTag(_)
            | TemplateNode::DebugTag(_)
            | TemplateNode::RenderTag(_) => {}
        }
    }
}

fn check_if_block(
    block: &svelte_parser::IfBlock,
    diagnostics: &mut Vec<Diagnostic>,
    last_heading: &mut Option<u8>,
) {
    check_fragment(&block.consequent.nodes, diagnostics, last_heading);
    if let Some(alternate) = &block.alternate {
        match alternate {
            ElseBranch::Else(fragment) => {
                check_fragment(&fragment.nodes, diagnostics, last_heading);
            }
            ElseBranch::ElseIf(nested) => {
                check_if_block(nested, diagnostics, last_heading);
            }
        }
    }
}

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
    fn test_heading_structure_valid() {
        let doc = parse(r#"<h1>Title</h1><h2>Section</h2><h3>Subsection</h3>"#).document;
        let diagnostics = check(&doc);
        assert!(diagnostics.is_empty());
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
