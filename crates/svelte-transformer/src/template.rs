//! Template to TSX transformation.
//!
//! Converts Svelte template nodes to TSX for type-checking expressions.

use svelte_parser::*;

/// Generates a TSX type-checking block for the template.
///
/// This generates a function that includes all expressions from the template
/// so they can be type-checked by TypeScript.
pub fn generate_template_check(fragment: &Fragment) -> String {
    let mut expressions = Vec::new();
    collect_expressions(&fragment.nodes, &mut expressions);

    if expressions.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    output.push_str("\n// === TEMPLATE TYPE-CHECK BLOCK ===\n");
    output.push_str("// This is never executed, just type-checked\n");
    output.push_str("function __svelte_template_check__() {\n");

    for expr in expressions {
        output.push_str("  ");
        output.push_str(&expr);
        output.push_str(";\n");
    }

    output.push_str("}\n");
    output
}

/// Collects all expressions from template nodes.
fn collect_expressions(nodes: &[TemplateNode], expressions: &mut Vec<String>) {
    for node in nodes {
        match node {
            TemplateNode::Expression(expr) => {
                expressions.push(expr.expression.clone());
            }
            TemplateNode::Element(el) => {
                collect_attribute_expressions(&el.attributes, expressions);
                collect_expressions(&el.children, expressions);
            }
            TemplateNode::Component(comp) => {
                collect_attribute_expressions(&comp.attributes, expressions);
                collect_expressions(&comp.children, expressions);
            }
            TemplateNode::SvelteElement(el) => {
                collect_attribute_expressions(&el.attributes, expressions);
                collect_expressions(&el.children, expressions);
            }
            TemplateNode::HtmlTag(tag) => {
                expressions.push(tag.expression.clone());
            }
            TemplateNode::RenderTag(tag) => {
                expressions.push(tag.expression.clone());
            }
            TemplateNode::IfBlock(block) => {
                expressions.push(block.condition.clone());
                collect_expressions(&block.consequent.nodes, expressions);
                if let Some(alt) = &block.alternate {
                    match alt {
                        ElseBranch::Else(frag) => {
                            collect_expressions(&frag.nodes, expressions);
                        }
                        ElseBranch::ElseIf(elif) => {
                            expressions.push(elif.condition.clone());
                            collect_expressions(&elif.consequent.nodes, expressions);
                        }
                    }
                }
            }
            TemplateNode::EachBlock(block) => {
                expressions.push(block.expression.clone());
                if let Some(key) = &block.key {
                    expressions.push(key.expression.clone());
                }
                collect_expressions(&block.body.nodes, expressions);
                if let Some(fallback) = &block.fallback {
                    collect_expressions(&fallback.nodes, expressions);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                expressions.push(block.expression.clone());
                if let Some(pending) = &block.pending {
                    collect_expressions(&pending.nodes, expressions);
                }
                if let Some(then) = &block.then {
                    collect_expressions(&then.body.nodes, expressions);
                }
                if let Some(catch) = &block.catch {
                    collect_expressions(&catch.body.nodes, expressions);
                }
            }
            TemplateNode::KeyBlock(block) => {
                expressions.push(block.expression.clone());
                collect_expressions(&block.body.nodes, expressions);
            }
            TemplateNode::SnippetBlock(block) => {
                collect_expressions(&block.body.nodes, expressions);
            }
            TemplateNode::Text(_)
            | TemplateNode::Comment(_)
            | TemplateNode::ConstTag(_)
            | TemplateNode::DebugTag(_) => {}
        }
    }
}

/// Collects expressions from attributes.
fn collect_attribute_expressions(attrs: &[Attribute], expressions: &mut Vec<String>) {
    for attr in attrs {
        match attr {
            Attribute::Normal(a) => {
                if let AttributeValue::Expression(expr) = &a.value {
                    expressions.push(expr.expression.clone());
                } else if let AttributeValue::Concat(parts) = &a.value {
                    for part in parts {
                        if let AttributeValuePart::Expression(expr) = part {
                            expressions.push(expr.expression.clone());
                        }
                    }
                }
            }
            Attribute::Spread(s) => {
                expressions.push(s.expression.clone());
            }
            Attribute::Directive(d) => {
                if let Some(expr) = &d.expression {
                    expressions.push(expr.expression.clone());
                }
            }
            Attribute::Shorthand(s) => {
                expressions.push(s.name.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_collect_simple_expression() {
        let result = parse("<div>{value}</div>");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("value"));
    }

    #[test]
    fn test_collect_if_condition() {
        let result = parse("{#if condition}yes{/if}");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("condition"));
    }

    #[test]
    fn test_collect_each_expression() {
        let result = parse("{#each items as item}{item}{/each}");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("items"));
        assert!(output.contains("item"));
    }
}
