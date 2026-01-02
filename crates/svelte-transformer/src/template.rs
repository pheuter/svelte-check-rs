//! Template to TSX transformation.
//!
//! Converts Svelte template nodes to TSX for type-checking expressions.
//! This module generates proper TypeScript that preserves type narrowing
//! for control flow blocks and tracks spans for source mapping.

use smol_str::SmolStr;
use source_map::{ByteOffset, Span};
use std::collections::HashSet;
use svelte_parser::*;

/// Transform store subscriptions in an expression.
///
/// In Svelte, `$storeName` is shorthand for subscribing to a store and getting its value.
/// We transform `$storeName` to `__svelte_store_get(storeName)` so TypeScript sees the
/// dereferenced value type, not the store type.
///
/// Special case: `typeof $store` becomes `__StoreValue<typeof store>` because
/// TypeScript's typeof operator doesn't work with function calls.
///
/// This only applies to store subscriptions (identifier after $), not to:
/// - Runes like `$state()`, `$derived()` (have parentheses)
/// - Special variables like `$$props`, `$$slots`
pub(crate) fn transform_store_subscriptions(expr: &str) -> String {
    let mut result = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();
    // Track typeof context: whitespace accumulated after "typeof"
    let mut typeof_state: Option<String> = None;
    // Track when we've just emitted __StoreValue<typeof ...> and need indexed property access
    let mut in_storevalue_access = false;

    while let Some(ch) = chars.next() {
        // When in storevalue access mode, convert .prop to ["prop"]
        if in_storevalue_access {
            if ch == '.' {
                // Check if followed by identifier
                if chars
                    .peek()
                    .is_some_and(|&c| c.is_ascii_alphabetic() || c == '_')
                {
                    // Collect the property name
                    let mut prop = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            prop.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Emit as indexed access
                    result.push_str("[\"");
                    result.push_str(&prop);
                    result.push_str("\"]");
                    // Stay in storevalue_access mode for chained access
                    continue;
                }
            }
            // Not a property access, exit the mode
            in_storevalue_access = false;
        }

        if ch == '$' {
            // Check if this is a store subscription
            if let Some(&next) = chars.peek() {
                // Skip $$ patterns ($$props, $$slots, etc.)
                if next == '$' {
                    // Restore typeof if we were tracking it
                    if let Some(ws) = typeof_state.take() {
                        result.push_str("typeof");
                        result.push_str(&ws);
                    }
                    result.push(ch);
                    continue;
                }

                // Check if followed by valid identifier start
                if next.is_ascii_alphabetic() || next == '_' {
                    // Collect the identifier
                    let mut identifier = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            identifier.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Peek ahead to see what follows
                    let rest: String = chars.clone().collect();
                    let next_non_ws = rest.chars().find(|c| !c.is_whitespace());

                    // If followed by ( or <, it's a rune - keep the $identifier
                    // If followed by :, it's an object property key (e.g., { $or: ... })
                    if next_non_ws == Some('(')
                        || next_non_ws == Some('<')
                        || next_non_ws == Some(':')
                    {
                        // Restore typeof if we were tracking it
                        if let Some(ws) = typeof_state.take() {
                            result.push_str("typeof");
                            result.push_str(&ws);
                        }
                        result.push(ch);
                        result.push_str(&identifier);
                    } else if typeof_state.is_some() {
                        // In typeof context, use type helper instead of function call
                        // typeof $store.prop -> __StoreValue<typeof store>["prop"]
                        // Don't restore typeof - we're replacing it
                        typeof_state = None;
                        result.push_str("__StoreValue<typeof ");
                        result.push_str(&identifier);
                        result.push('>');
                        // Enter storevalue access mode for property access conversion
                        in_storevalue_access = true;
                    } else {
                        // It's a store subscription - wrap with helper function
                        result.push_str("__svelte_store_get(");
                        result.push_str(&identifier);
                        result.push(')');
                    }
                } else {
                    // Restore typeof if we were tracking it
                    if let Some(ws) = typeof_state.take() {
                        result.push_str("typeof");
                        result.push_str(&ws);
                    }
                    result.push(ch);
                }
            } else {
                // Restore typeof if we were tracking it
                if let Some(ws) = typeof_state.take() {
                    result.push_str("typeof");
                    result.push_str(&ws);
                }
                result.push(ch);
            }
        } else {
            // Not a $ - check if we're tracking typeof
            if let Some(ref mut ws) = typeof_state {
                if ch.is_whitespace() {
                    // Accumulate whitespace after typeof
                    ws.push(ch);
                } else {
                    // Non-whitespace, non-$ after typeof - restore typeof and continue
                    result.push_str("typeof");
                    result.push_str(ws);
                    result.push(ch);
                    typeof_state = None;
                }
            } else {
                result.push(ch);
                // Check if we just completed the word "typeof"
                if result.ends_with("typeof") {
                    // Peek at next char to ensure it's whitespace (typeof is followed by space)
                    if chars.peek().is_some_and(|&c| c.is_whitespace()) {
                        // Remove the "typeof" we just added - we'll track it separately
                        result.truncate(result.len() - 6);
                        typeof_state = Some(String::new());
                    }
                }
            }
        }
    }

    // Handle case where expression ends with "typeof " (restore it)
    if let Some(ws) = typeof_state {
        result.push_str("typeof");
        result.push_str(&ws);
    }

    result
}

/// Collect store subscriptions in a template expression without rewriting them.
///
/// This preserves `$store` syntax (so control flow narrowing works),
/// and records referenced store names for alias declarations.
fn transform_store_subscriptions_in_template(
    expr: &str,
    store_names: &mut HashSet<SmolStr>,
) -> String {
    let mut result = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();

    let mut in_string: Option<char> = None;
    let mut prev_was_escape = false;
    let mut template_brace_depth: Vec<usize> = Vec::new();
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            result.push(ch);
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }

        if in_block_comment {
            result.push(ch);
            if ch == '*' && chars.peek() == Some(&'/') {
                result.push(chars.next().unwrap());
                in_block_comment = false;
            }
            continue;
        }

        if let Some(quote) = in_string {
            if quote != '`' {
                result.push(ch);
                if prev_was_escape {
                    prev_was_escape = false;
                } else if ch == '\\' {
                    prev_was_escape = true;
                } else if ch == quote {
                    in_string = None;
                }
                continue;
            } else {
                if prev_was_escape {
                    result.push(ch);
                    prev_was_escape = false;
                    continue;
                }
                if ch == '\\' {
                    result.push(ch);
                    prev_was_escape = true;
                    continue;
                }
                if ch == '`' {
                    result.push(ch);
                    in_string = None;
                    continue;
                }
                if ch == '$' && chars.peek() == Some(&'{') {
                    result.push(ch);
                    result.push(chars.next().unwrap());
                    template_brace_depth.push(0);
                    in_string = None;
                    continue;
                }
                result.push(ch);
                continue;
            }
        }

        if !template_brace_depth.is_empty() {
            if ch == '{' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    *depth += 1;
                }
            } else if ch == '}' {
                if let Some(depth) = template_brace_depth.last_mut() {
                    if *depth == 0 {
                        template_brace_depth.pop();
                        result.push(ch);
                        in_string = Some('`');
                        continue;
                    } else {
                        *depth -= 1;
                    }
                }
            }
        }

        if ch == '/' {
            if chars.peek() == Some(&'/') {
                result.push(ch);
                result.push(chars.next().unwrap());
                in_line_comment = true;
                continue;
            } else if chars.peek() == Some(&'*') {
                result.push(ch);
                result.push(chars.next().unwrap());
                in_block_comment = true;
                continue;
            }
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            in_string = Some(ch);
            result.push(ch);
            continue;
        }

        if ch == '$' {
            if let Some(&next) = chars.peek() {
                if next == '$' {
                    result.push(ch);
                    continue;
                }
                if next.is_ascii_alphabetic() || next == '_' {
                    let mut identifier = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            identifier.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    let rest: String = chars.clone().collect();
                    let trimmed_rest = rest.trim_start();
                    let next_non_ws = trimmed_rest.chars().next();

                    if next_non_ws == Some('(')
                        || next_non_ws == Some('<')
                        || next_non_ws == Some(':')
                    {
                        result.push(ch);
                        result.push_str(&identifier);
                    } else {
                        store_names.insert(SmolStr::new(&identifier));
                        result.push(ch);
                        result.push_str(&identifier);
                    }
                    continue;
                }
            }
        }

        result.push(ch);
    }

    result
}

/// An expression collected from the template with its original span.
#[derive(Debug, Clone)]
pub struct TemplateExpression {
    /// The expression text.
    pub expression: String,
    /// The span in the original source.
    pub span: Span,
    /// The context in which this expression appears.
    pub context: ExpressionContext,
}

/// The context in which an expression appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionContext {
    /// `{expression}` - mustache interpolation.
    Interpolation,
    /// `attr={expression}` - attribute value.
    Attribute,
    /// `onclick={handler}` or `on:click={handler}` - event handler.
    EventHandler,
    /// `bind:value={x}` - binding.
    Binding,
    /// `{...props}` - spread attribute.
    Spread,
    /// `{#if condition}` - block condition.
    IfCondition,
    /// `{#each items as item}` - each iterable.
    EachIterable,
    /// `(key)` expression in each block.
    EachKey,
    /// `{#await promise}` - await expression.
    AwaitPromise,
    /// `{#key expr}` - key expression.
    KeyExpression,
    /// `{@html expr}` - html tag.
    HtmlTag,
    /// `{@render snippet()}` - render tag.
    RenderTag,
    /// `{@const x = ...}` - const tag.
    ConstTag,
    /// `{@debug ...}` - debug tag.
    DebugTag,
}

/// A mapping from generated position to original span.
#[derive(Debug, Clone)]
pub struct GeneratedMapping {
    /// Start offset in the generated code (relative to template block start).
    pub generated_start: usize,
    /// End offset in the generated code.
    pub generated_end: usize,
    /// The original span in the source file.
    pub original_span: Span,
}

/// Result of template TSX generation.
#[derive(Debug)]
pub struct TemplateCheckResult {
    /// The generated TSX code.
    pub code: String,
    /// Expressions with their spans for source mapping.
    pub expressions: Vec<TemplateExpression>,
    /// Mappings from generated positions to original spans.
    pub mappings: Vec<GeneratedMapping>,
}

/// Generates a TSX type-checking block for the template.
///
/// This generates code that includes all expressions from the template
/// with proper control flow to preserve TypeScript type narrowing.
pub fn generate_template_check(fragment: &Fragment) -> String {
    let result = generate_template_check_with_spans(fragment);
    result.code
}

/// Generates a TSX type-checking block with span information.
pub fn generate_template_check_with_spans(fragment: &Fragment) -> TemplateCheckResult {
    let mut ctx = TemplateContext::new();
    ctx.generate_fragment(fragment);

    if ctx.expressions.is_empty() && ctx.output.is_empty() {
        return TemplateCheckResult {
            code: String::new(),
            expressions: Vec::new(),
            mappings: Vec::new(),
        };
    }

    let mut code = String::new();
    code.push_str("\n// === TEMPLATE TYPE-CHECK BLOCK ===\n");
    code.push_str("// This is never executed, just type-checked\n");
    code.push_str("async function __svelte_template_check__() {\n");

    // Track the offset where ctx.output will start in the final code
    let mut preamble_len = code.len();

    if !ctx.store_names.is_empty() {
        let mut stores: Vec<_> = ctx.store_names.iter().collect();
        stores.sort();
        for store in stores {
            let store_decl = format!("  let ${} = __svelte_store_get({});\n", store, store);
            code.push_str(&store_decl);
        }
        preamble_len = code.len();
    }

    code.push_str(&ctx.output);
    code.push_str("}\n");

    // Adjust mapping offsets to account for the preamble
    let mappings = ctx
        .mappings
        .into_iter()
        .map(|m| GeneratedMapping {
            generated_start: m.generated_start + preamble_len,
            generated_end: m.generated_end + preamble_len,
            original_span: m.original_span,
        })
        .collect();

    TemplateCheckResult {
        code,
        expressions: ctx.expressions,
        mappings,
    }
}

/// Checks if a property name needs to be quoted in JavaScript.
/// Property names containing hyphens, colons, or other non-identifier characters need quotes.
fn needs_quote(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // First character must be a letter, underscore, or $
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
        return true;
    }
    // Rest can be letters, digits, underscores, or $
    name.chars()
        .skip(1)
        .any(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
}

/// Formats a property name for JavaScript object literal.
fn format_prop_name(name: &str) -> String {
    if needs_quote(name) {
        format!("\"{}\"", name)
    } else {
        name.to_string()
    }
}

/// Context for template transformation.
struct TemplateContext {
    output: String,
    expressions: Vec<TemplateExpression>,
    /// Mappings from generated positions to original spans.
    mappings: Vec<GeneratedMapping>,
    indent: usize,
    /// Counter for generating unique variable names.
    counter: usize,
    /// Store names referenced via $store syntax inside the template.
    store_names: HashSet<SmolStr>,
}

impl TemplateContext {
    fn new() -> Self {
        Self {
            output: String::new(),
            expressions: Vec::new(),
            mappings: Vec::new(),
            indent: 1,
            counter: 0,
            store_names: HashSet::new(),
        }
    }

    fn indent_str(&self) -> String {
        "  ".repeat(self.indent)
    }

    fn next_id(&mut self) -> usize {
        let id = self.counter;
        self.counter += 1;
        id
    }

    fn emit(&mut self, s: &str) {
        self.output.push_str(&self.indent_str());
        self.output.push_str(s);
        self.output.push('\n');
    }

    fn transform_expr(&mut self, expr: &str) -> String {
        transform_store_subscriptions_in_template(expr, &mut self.store_names)
    }

    fn emit_expression(&mut self, expr: &str, span: Span, context: ExpressionContext) {
        // Keep $store syntax but record store usage for alias declarations.
        let transformed = self.transform_expr(expr);

        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span,
            context,
        });

        // Track where the expression starts in the generated output
        let indent_str = self.indent_str();
        let generated_start = self.output.len() + indent_str.len();

        // Wrap object literals in parentheses to prevent TypeScript from
        // interpreting them as blocks (e.g., `{foo: bar}` would be a label)
        let trimmed = transformed.trim_start();
        let (prefix, suffix) = if trimmed.starts_with('{') && !trimmed.starts_with("{{") {
            ("(", ");")
        } else {
            ("", ";")
        };

        // Calculate the actual expression position (after any prefix)
        let expr_start = generated_start + prefix.len();
        let expr_end = expr_start + transformed.len();

        // Record the mapping
        self.mappings.push(GeneratedMapping {
            generated_start: expr_start,
            generated_end: expr_end,
            original_span: span,
        });

        self.emit(&format!("{}{}{}", prefix, transformed, suffix));
    }

    /// Records a mapping for an expression that will be emitted inline (not as a standalone statement).
    /// Returns the transformed expression.
    fn track_inline_expression(
        &mut self,
        expr: &str,
        span: Span,
        context: ExpressionContext,
    ) -> String {
        let transformed = self.transform_expr(expr);

        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span,
            context,
        });

        transformed
    }

    /// Records a mapping at the current output position for the given expression text and span.
    fn record_mapping_at_current_pos(&mut self, expr_text: &str, original_span: Span) {
        let generated_start = self.output.len();
        let generated_end = generated_start + expr_text.len();

        self.mappings.push(GeneratedMapping {
            generated_start,
            generated_end,
            original_span,
        });
    }

    /// Emits a property name with source mapping.
    ///
    /// This records a mapping from the generated prop name to the original attribute name,
    /// which is important for TypeScript errors that point to property names.
    fn emit_prop_name_with_mapping(&mut self, name: &str, name_span: Span) {
        let formatted = format_prop_name(name);
        self.record_mapping_at_current_pos(&formatted, name_span);
        self.output.push_str(&formatted);
        self.output.push_str(": ");
    }

    fn generate_fragment(&mut self, fragment: &Fragment) {
        for node in &fragment.nodes {
            self.generate_node(node);
        }
    }

    fn generate_node(&mut self, node: &TemplateNode) {
        match node {
            TemplateNode::Expression(expr) => {
                self.emit_expression(
                    &expr.expression,
                    expr.expression_span,
                    ExpressionContext::Interpolation,
                );
            }
            TemplateNode::Element(el) => {
                self.generate_element_attributes(&el.name, &el.attributes);
                self.generate_fragment_nodes(&el.children);
            }
            TemplateNode::Component(comp) => {
                self.generate_component(&comp.name, comp.span, &comp.attributes, &comp.children);
            }
            TemplateNode::SvelteElement(el) => {
                // Special svelte:* elements
                let element_name = match el.kind {
                    SvelteElementKind::Self_ => "svelte:self",
                    SvelteElementKind::Component => "svelte:component",
                    SvelteElementKind::Element => "svelte:element",
                    SvelteElementKind::Window => "svelte:window",
                    SvelteElementKind::Document => "svelte:document",
                    SvelteElementKind::Body => "svelte:body",
                    SvelteElementKind::Head => "svelte:head",
                    SvelteElementKind::Options => "svelte:options",
                    SvelteElementKind::Fragment => "svelte:fragment",
                    SvelteElementKind::Boundary => "svelte:boundary",
                };
                self.generate_element_attributes(element_name, &el.attributes);
                self.generate_fragment_nodes(&el.children);
            }
            TemplateNode::HtmlTag(tag) => {
                self.emit_expression(
                    &tag.expression,
                    tag.expression_span,
                    ExpressionContext::HtmlTag,
                );
            }
            TemplateNode::RenderTag(tag) => {
                self.emit_expression(
                    &tag.expression,
                    tag.expression_span,
                    ExpressionContext::RenderTag,
                );
            }
            TemplateNode::ConstTag(tag) => {
                // @const creates a local binding - emit as-is (with store transformation)
                let transformed = self.transform_expr(&tag.declaration);
                self.emit(&format!("const {};", transformed));
            }
            TemplateNode::DebugTag(tag) => {
                for ident in &tag.identifiers {
                    let transformed = self.transform_expr(ident);
                    self.expressions.push(TemplateExpression {
                        expression: transformed.clone(),
                        span: tag.span,
                        context: ExpressionContext::DebugTag,
                    });
                    self.emit(&format!("{};", transformed));
                }
            }
            TemplateNode::IfBlock(block) => {
                self.generate_if_block(block);
            }
            TemplateNode::EachBlock(block) => {
                self.generate_each_block(block);
            }
            TemplateNode::AwaitBlock(block) => {
                self.generate_await_block(block);
            }
            TemplateNode::KeyBlock(block) => {
                self.emit_expression(
                    &block.expression,
                    block.expression_span,
                    ExpressionContext::KeyExpression,
                );
                self.generate_fragment(&block.body);
            }
            TemplateNode::SnippetBlock(block) => {
                self.generate_snippet_block(block);
            }
            TemplateNode::Text(_) | TemplateNode::Comment(_) => {
                // No expressions to type-check
            }
        }
    }

    fn generate_fragment_nodes(&mut self, nodes: &[TemplateNode]) {
        for node in nodes {
            self.generate_node(node);
        }
    }

    fn generate_element_attributes(&mut self, element_name: &str, attrs: &[Attribute]) {
        for attr in attrs {
            match attr {
                Attribute::Normal(a) => {
                    if let Some(event_name) = event_attribute_name(&a.name) {
                        if let AttributeValue::Expression(expr) = &a.value {
                            let context = ExpressionContext::EventHandler;
                            let event_type = get_event_type(element_name, event_name);
                            let id = self.next_id();
                            let transformed = self.transform_expr(&expr.expression);
                            self.expressions.push(TemplateExpression {
                                expression: transformed.clone(),
                                span: expr.expression_span,
                                context,
                            });
                            self.emit(&format!(
                                "const __event_{}: ((e: {}) => void) | null | undefined = {};",
                                id, event_type, transformed
                            ));
                        } else {
                            self.generate_attribute_value(&a.value);
                        }
                    } else {
                        self.generate_attribute_value(&a.value);
                    }
                }
                Attribute::Spread(s) => {
                    self.emit_expression(
                        &s.expression,
                        s.expression_span,
                        ExpressionContext::Spread,
                    );
                }
                Attribute::Directive(d) => {
                    self.generate_directive(element_name, d);
                }
                Attribute::Shorthand(s) => {
                    // {name} is shorthand for name={name}
                    self.emit_expression(s.name.as_ref(), s.span, ExpressionContext::Attribute);
                }
            }
        }
    }

    fn generate_attribute_value(&mut self, value: &AttributeValue) {
        match value {
            AttributeValue::Expression(expr) => {
                self.emit_expression(
                    &expr.expression,
                    expr.expression_span,
                    ExpressionContext::Attribute,
                );
            }
            AttributeValue::Concat(parts) => {
                for part in parts {
                    if let AttributeValuePart::Expression(expr) = part {
                        self.emit_expression(
                            &expr.expression,
                            expr.expression_span,
                            ExpressionContext::Attribute,
                        );
                    }
                }
            }
            AttributeValue::Text(_) | AttributeValue::True => {
                // No expression to type-check
            }
        }
    }

    fn generate_directive(&mut self, element_name: &str, directive: &Directive) {
        if let Some(expr) = &directive.expression {
            let context = match directive.kind {
                DirectiveKind::On => ExpressionContext::EventHandler,
                DirectiveKind::Bind => ExpressionContext::Binding,
                _ => ExpressionContext::Attribute,
            };

            // For event handlers, we can add type annotations
            if directive.kind == DirectiveKind::On {
                let event_type = get_event_type(element_name, &directive.name);
                let id = self.next_id();
                let transformed = self.transform_expr(&expr.expression);
                self.expressions.push(TemplateExpression {
                    expression: transformed.clone(),
                    span: expr.expression_span,
                    context,
                });
                // Generate typed event handler check
                self.emit(&format!(
                    "const __event_{}: ((e: {}) => void) | null | undefined = {};",
                    id, event_type, transformed
                ));
            } else if directive.kind == DirectiveKind::Use {
                let id = self.next_id();
                let action_target = action_target_type(element_name);
                let transformed = self.transform_expr(&expr.expression);
                self.expressions.push(TemplateExpression {
                    expression: transformed.clone(),
                    span: expr.expression_span,
                    context,
                });
                // Call the action with a typed element to contextually type the parameter.
                self.emit(&format!(
                    "const __action_result_{} = {}(null as unknown as {}, {});",
                    id, directive.name, action_target, transformed
                ));
                self.emit(&format!("void __action_result_{};", id));
            } else if directive.kind == DirectiveKind::Bind {
                if directive.name == "this" {
                    let transformed = self.transform_expr(&expr.expression);
                    self.expressions.push(TemplateExpression {
                        expression: transformed.clone(),
                        span: expr.expression_span,
                        context,
                    });
                    let id = self.next_id();
                    let bind_type = bind_this_type(element_name);
                    self.emit(&format!(
                        "const __bind_this_{} = null as unknown as {};",
                        id, bind_type
                    ));
                    self.emit(&format!("{} = __bind_this_{};", transformed, id));
                } else if let Some((getter, setter)) = split_top_level_comma(&expr.expression) {
                    let getter = self.transform_expr(&getter);
                    let setter = self.transform_expr(&setter);
                    self.expressions.push(TemplateExpression {
                        expression: getter.clone(),
                        span: expr.expression_span,
                        context,
                    });
                    self.expressions.push(TemplateExpression {
                        expression: setter.clone(),
                        span: expr.expression_span,
                        context,
                    });
                    let id = self.next_id();
                    self.emit(&format!(
                        "const __bind_pair_{}: [() => any, (value: any) => void] = [{}, {}];",
                        id, getter, setter
                    ));
                } else {
                    // For bindings, check the variable
                    self.emit_expression(&expr.expression, expr.expression_span, context);
                }
            } else {
                self.emit_expression(&expr.expression, expr.expression_span, context);
            }
        } else if directive.kind == DirectiveKind::Use {
            let id = self.next_id();
            let action_target = action_target_type(element_name);
            // Call the action with only the element when no parameter is provided.
            self.emit(&format!(
                "const __action_result_{} = {}(null as unknown as {});",
                id, directive.name, action_target
            ));
            self.emit(&format!("void __action_result_{};", id));
        }
    }

    fn generate_component(
        &mut self,
        name: &str,
        component_span: Span,
        attrs: &[Attribute],
        children: &[TemplateNode],
    ) {
        // Collect props for the component
        // First pass: collect all prop-like attributes (Normal, Shorthand, Spread)
        // into a props object, then close it before handling directives

        // Separate snippets from other children - snippets become inline props
        let (snippets, other_children): (Vec<_>, Vec<_>) = children
            .iter()
            .partition(|node| matches!(node, TemplateNode::SnippetBlock(_)));

        // Calculate approximate name span from component span
        // Component span starts at '<', so name starts at span.start (after '<' is parsed)
        // We use the component span's start as an approximation for the name position
        let name_span = Span::new(
            component_span.start,
            component_span.start + ByteOffset::from(name.len() as u32),
        );

        // Track the component name position for source mapping
        // Emit indent first, then record the mapping at the current position
        let indent_str = self.indent_str();
        self.output.push_str(&indent_str);
        self.record_mapping_at_current_pos(name, name_span);
        self.output.push_str(&format!("{}(null as any, {{\n", name));
        self.indent += 1;

        // First pass: build the props object with Normal, Shorthand, Spread, and bind directives
        for attr in attrs {
            match attr {
                Attribute::Normal(a) => {
                    // Compute name span from attribute span
                    let name_span = Span::new(
                        a.span.start,
                        a.span.start + ByteOffset::from(a.name.len() as u32),
                    );

                    match &a.value {
                        AttributeValue::Expression(expr) => {
                            let transformed = self.track_inline_expression(
                                &expr.expression,
                                expr.expression_span,
                                ExpressionContext::Attribute,
                            );
                            // Emit with mapping for both name and expression value
                            let indent_str = self.indent_str();
                            self.output.push_str(&indent_str);
                            self.emit_prop_name_with_mapping(&a.name, name_span);
                            self.record_mapping_at_current_pos(&transformed, expr.expression_span);
                            self.output.push_str(&transformed);
                            self.output.push_str(",\n");
                        }
                        AttributeValue::Text(t) => {
                            let indent_str = self.indent_str();
                            self.output.push_str(&indent_str);
                            self.emit_prop_name_with_mapping(&a.name, name_span);
                            self.output.push_str(&format!("\"{}\",\n", t.value));
                        }
                        AttributeValue::True => {
                            let indent_str = self.indent_str();
                            self.output.push_str(&indent_str);
                            self.emit_prop_name_with_mapping(&a.name, name_span);
                            self.output.push_str("true,\n");
                        }
                        AttributeValue::Concat(parts) => {
                            // Build a template literal for concatenated values
                            let mut template = String::from("`");
                            for part in parts {
                                match part {
                                    AttributeValuePart::Text(t) => {
                                        // Escape backticks and ${ in text
                                        let escaped =
                                            t.value.replace('`', "\\`").replace("${", "\\${");
                                        template.push_str(&escaped);
                                    }
                                    AttributeValuePart::Expression(expr) => {
                                        let transformed = self.track_inline_expression(
                                            &expr.expression,
                                            expr.expression_span,
                                            ExpressionContext::Attribute,
                                        );
                                        // Normalize multiline expressions to single line
                                        // to avoid issues with template literals.
                                        // Remove leading/trailing whitespace on each line and join.
                                        let normalized: String = transformed
                                            .lines()
                                            .map(|line| line.trim())
                                            .collect::<Vec<_>>()
                                            .join("");
                                        template.push_str("${");
                                        template.push_str(&normalized);
                                        template.push('}');
                                    }
                                }
                            }
                            template.push('`');
                            let indent_str = self.indent_str();
                            self.output.push_str(&indent_str);
                            self.emit_prop_name_with_mapping(&a.name, name_span);
                            self.output.push_str(&format!("{},\n", template));
                        }
                    }
                }
                Attribute::Spread(s) => {
                    // Include spreads in the props object with mapping
                    let transformed = self.track_inline_expression(
                        &s.expression,
                        s.expression_span,
                        ExpressionContext::Spread,
                    );
                    let indent_str = self.indent_str();
                    self.output.push_str(&indent_str);
                    self.output.push_str("...");
                    self.record_mapping_at_current_pos(&transformed, s.expression_span);
                    self.output.push_str(&transformed);
                    self.output.push_str(",\n");
                }
                Attribute::Shorthand(s) => {
                    let transformed =
                        self.track_inline_expression(&s.name, s.span, ExpressionContext::Attribute);
                    let indent_str = self.indent_str();
                    self.output.push_str(&indent_str);
                    self.record_mapping_at_current_pos(&transformed, s.span);
                    self.output.push_str(&transformed);
                    self.output.push_str(",\n");
                }
                Attribute::Directive(d) if d.kind == DirectiveKind::Bind && d.name != "this" => {
                    // For bind:name, the name starts after "bind:" (5 chars)
                    let name_offset = match d.kind {
                        DirectiveKind::Bind => 5, // "bind:"
                        _ => 0,
                    };
                    let name_span = Span::new(
                        d.span.start + ByteOffset::from(name_offset),
                        d.span.start + ByteOffset::from(name_offset + d.name.len() as u32),
                    );
                    let indent_str = self.indent_str();
                    self.output.push_str(&indent_str);
                    self.emit_prop_name_with_mapping(&d.name, name_span);
                    self.output.push_str("undefined as any,\n");
                }
                Attribute::Directive(_) => {
                    // Directives handled in second pass
                }
            }
        }

        // Add snippets as inline arrow function props
        // This enables TypeScript to infer parameter types from the component's expected snippet type
        for snippet_node in &snippets {
            if let TemplateNode::SnippetBlock(block) = snippet_node {
                // Generate snippet as arrow function: name: (params) => { body }
                // Transform store subscriptions in parameter types
                let transformed_params = self.transform_expr(&block.parameters);
                let trimmed_params = transformed_params.trim();
                if trimmed_params.is_empty() {
                    self.emit(&format!("{}: () => {{", block.name));
                    self.indent += 1;
                    self.generate_fragment(&block.body);
                    self.emit("return __svelte_snippet_return;");
                    self.indent -= 1;
                    self.emit("},");
                } else {
                    let snippet_param = format!("__snippet_params_{}", self.next_id());
                    self.emit(&format!("{}: ({}) => {{", block.name, snippet_param));
                    self.indent += 1;
                    // Emit the destructuring with source mapping for the parameters
                    let indent_str = self.indent_str();
                    let prefix = format!("{}const ", indent_str);
                    let generated_start = self.output.len() + prefix.len();
                    let generated_end = generated_start + trimmed_params.len();
                    self.mappings.push(GeneratedMapping {
                        generated_start,
                        generated_end,
                        original_span: block.parameters_span,
                    });
                    self.emit(&format!("const {} = {};", trimmed_params, snippet_param));
                    self.generate_fragment(&block.body);
                    self.emit("return __svelte_snippet_return;");
                    self.indent -= 1;
                    self.emit("},");
                }
            }
        }

        // Add default children as a snippet prop when present
        if !other_children.is_empty() {
            self.emit("children: () => {");
            self.indent += 1;
            self.emit("return __svelte_snippet_return;");
            self.indent -= 1;
            self.emit("},");
        }

        // Close the props object and component call
        self.indent -= 1;
        self.emit("});");

        // Second pass: handle directives separately (bindings, events, etc.)
        for attr in attrs {
            if let Attribute::Directive(d) = attr {
                self.generate_directive(name, d);
            }
        }

        // Process non-snippet children in the parent scope to preserve narrowing.
        for child in &other_children {
            self.generate_node(child);
        }
    }

    fn generate_if_block(&mut self, block: &IfBlock) {
        // Use real if statements to preserve type narrowing
        let transformed = self.track_inline_expression(
            &block.condition,
            block.condition_span,
            ExpressionContext::IfCondition,
        );

        // Emit with mapping for the condition expression
        let indent_str = self.indent_str();
        self.output.push_str(&indent_str);
        self.output.push_str("if (");
        self.record_mapping_at_current_pos(&transformed, block.condition_span);
        self.output.push_str(&transformed);
        self.output.push_str(") {\n");
        self.indent += 1;
        self.generate_fragment(&block.consequent);
        self.indent -= 1;

        if let Some(alt) = &block.alternate {
            match alt {
                ElseBranch::Else(frag) => {
                    self.emit("} else {");
                    self.indent += 1;
                    self.generate_fragment(frag);
                    self.indent -= 1;
                    self.emit("}");
                }
                ElseBranch::ElseIf(elif) => {
                    self.output.push_str(&self.indent_str());
                    self.output.push_str("} else ");
                    // Continue with else-if - use the boxed IfBlock directly
                    self.generate_if_block_continuation(elif);
                }
            }
        } else {
            self.emit("}");
        }
    }

    fn generate_if_block_continuation(&mut self, block: &IfBlock) {
        let transformed = self.track_inline_expression(
            &block.condition,
            block.condition_span,
            ExpressionContext::IfCondition,
        );

        // Emit with mapping for the condition expression
        self.output.push_str("if (");
        self.record_mapping_at_current_pos(&transformed, block.condition_span);
        self.output.push_str(&transformed);
        self.output.push_str(") {\n");
        self.indent += 1;
        self.generate_fragment(&block.consequent);
        self.indent -= 1;

        if let Some(alt) = &block.alternate {
            match alt {
                ElseBranch::Else(frag) => {
                    self.emit("} else {");
                    self.indent += 1;
                    self.generate_fragment(frag);
                    self.indent -= 1;
                    self.emit("}");
                }
                ElseBranch::ElseIf(elif) => {
                    self.output.push_str(&self.indent_str());
                    self.output.push_str("} else ");
                    self.generate_if_block_continuation(elif);
                }
            }
        } else {
            self.emit("}");
        }
    }

    fn generate_each_block(&mut self, block: &EachBlock) {
        let id = self.next_id();

        // Emit the iterable expression with mapping
        let transformed = self.track_inline_expression(
            &block.expression,
            block.expression_span,
            ExpressionContext::EachIterable,
        );

        // Generate a for loop that introduces the loop variable
        let indent_str = self.indent_str();
        self.output.push_str(&indent_str);
        self.output.push_str(&format!("const __each_{} = ", id));
        self.record_mapping_at_current_pos(&transformed, block.expression_span);
        self.output.push_str(&transformed);
        self.output.push_str(";\n");

        // Determine the loop variable pattern
        let item_pattern = &block.context;
        let index_var = block.index.as_ref().map(|i| i.to_string());

        if let Some(ref idx) = index_var {
            self.emit(&format!(
                "for (const [{}, {}] of __svelte_each_indexed(__each_{})) {{",
                idx, item_pattern, id
            ));
        } else {
            self.emit(&format!("for (const {} of __each_{}) {{", item_pattern, id));
        }

        self.indent += 1;

        // Key expression if present
        if let Some(key) = &block.key {
            self.emit_expression(&key.expression, key.span, ExpressionContext::EachKey);
        }

        self.generate_fragment(&block.body);
        self.indent -= 1;
        self.emit("}");

        // Fallback (else) block
        if let Some(fallback) = &block.fallback {
            self.emit(&format!("if (__svelte_is_empty(__each_{})) {{", id));
            self.indent += 1;
            self.generate_fragment(fallback);
            self.indent -= 1;
            self.emit("}");
        }
    }

    fn generate_await_block(&mut self, block: &AwaitBlock) {
        let id = self.next_id();

        let transformed = self.track_inline_expression(
            &block.expression,
            block.expression_span,
            ExpressionContext::AwaitPromise,
        );

        // Emit with mapping
        let indent_str = self.indent_str();
        self.output.push_str(&indent_str);
        self.output.push_str(&format!("const __await_{} = ", id));
        self.record_mapping_at_current_pos(&transformed, block.expression_span);
        self.output.push_str(&transformed);
        self.output.push_str(";\n");

        // Pending block
        if let Some(pending) = &block.pending {
            self.emit("{");
            self.indent += 1;
            self.emit("// pending");
            self.generate_fragment(pending);
            self.indent -= 1;
            self.emit("}");
        }

        // Then block
        if let Some(then) = &block.then {
            self.emit("{");
            self.indent += 1;
            self.emit(&format!("const __then_{} = async () => {{", id));
            self.indent += 1;
            if let Some(ref value) = then.value {
                self.emit(&format!(
                    "const {}: Awaited<typeof __await_{}> = await __await_{};",
                    value, id, id
                ));
            }
            self.generate_fragment(&then.body);
            self.indent -= 1;
            self.emit("};");
            self.emit(&format!("void __then_{}();", id));
            self.indent -= 1;
            self.emit("}");
        }

        // Catch block
        if let Some(catch) = &block.catch {
            self.emit("{");
            self.indent += 1;
            if let Some(ref error) = catch.error {
                self.emit(&format!(
                    "const {}: unknown = __svelte_catch_error(__await_{});",
                    error, id
                ));
            }
            self.generate_fragment(&catch.body);
            self.indent -= 1;
            self.emit("}");
        }
    }

    fn generate_snippet_block(&mut self, block: &SnippetBlock) {
        // Snippets are local functions, use original names so @render tags can call them
        // JavaScript's block scoping handles uniqueness when snippets appear in different scopes
        // Transform store subscriptions in parameter types (e.g., typeof $formData.prop)
        let transformed_params = self.transform_expr(&block.parameters);
        let trimmed_params = transformed_params.trim();
        if trimmed_params.is_empty() {
            self.emit(&format!("function {}() {{", block.name));
        } else if trimmed_params.starts_with('{') || trimmed_params.starts_with('[') {
            let param_name = format!("__snippet_params_{}", self.next_id());
            self.emit(&format!("function {}({}: any) {{", block.name, param_name));
            self.indent += 1;
            self.emit(&format!("const {} = {};", trimmed_params, param_name));
            self.indent -= 1;
        } else {
            self.emit(&format!(
                "function {}({}) {{",
                block.name, transformed_params
            ));
        }
        self.indent += 1;
        self.generate_fragment(&block.body);
        self.emit("return __svelte_snippet_return;");
        self.indent -= 1;
        self.emit("}");
    }
}

/// Get the TypeScript event type for a given element and event name.
fn is_component_tag(element: &str) -> bool {
    element
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
        || element.contains('.')
}

fn event_target_type(element: &str) -> Option<&'static str> {
    match element {
        "input" => Some("HTMLInputElement"),
        "textarea" => Some("HTMLTextAreaElement"),
        "select" => Some("HTMLSelectElement"),
        "option" => Some("HTMLOptionElement"),
        "button" => Some("HTMLButtonElement"),
        "form" => Some("HTMLFormElement"),
        "a" => Some("HTMLAnchorElement"),
        "img" => Some("HTMLImageElement"),
        "video" => Some("HTMLVideoElement"),
        "audio" => Some("HTMLAudioElement"),
        "canvas" => Some("HTMLCanvasElement"),
        "svg" => Some("SVGElement"),
        "svelte:window" => Some("Window"),
        "svelte:document" => Some("Document"),
        "svelte:body" => Some("HTMLElement"),
        "svelte:element" => Some("HTMLElement"),
        _ => {
            if is_component_tag(element) {
                None
            } else {
                Some("HTMLElement")
            }
        }
    }
}

/// Get the TypeScript event type for a given element and event name.
fn get_event_type(element: &str, event: &str) -> String {
    let base = match (element, event) {
        // Mouse events
        (_, "click" | "dblclick" | "contextmenu") => "MouseEvent",
        (
            _,
            "mousedown" | "mouseup" | "mouseenter" | "mouseleave" | "mousemove" | "mouseover"
            | "mouseout",
        ) => "MouseEvent",

        // Keyboard events
        (_, "keydown" | "keyup" | "keypress") => "KeyboardEvent",

        // Input events
        ("input" | "textarea", "input") => "InputEvent",
        ("input" | "textarea" | "select", "change") => "Event",
        (_, "input") => "InputEvent",

        // Focus events
        (_, "focus" | "blur" | "focusin" | "focusout") => "FocusEvent",

        // Form events
        ("form", "submit") => "SubmitEvent",
        ("form", "reset") => "Event",

        // Drag events
        (_, "drag" | "dragstart" | "dragend" | "dragover" | "dragenter" | "dragleave" | "drop") => {
            "DragEvent"
        }

        // Touch events
        (_, "touchstart" | "touchend" | "touchmove" | "touchcancel") => "TouchEvent",

        // Wheel events
        (_, "wheel") => "WheelEvent",

        // Animation events
        (_, "animationstart" | "animationend" | "animationiteration") => "AnimationEvent",

        // Transition events
        (_, "transitionstart" | "transitionend" | "transitionrun" | "transitioncancel") => {
            "TransitionEvent"
        }

        // Pointer events
        (
            _,
            "pointerdown" | "pointerup" | "pointermove" | "pointerenter" | "pointerleave"
            | "pointerover" | "pointerout" | "pointercancel",
        ) => "PointerEvent",

        // Clipboard events
        (_, "copy" | "cut" | "paste") => "ClipboardEvent",

        // Media events
        (
            "audio" | "video",
            "play" | "pause" | "ended" | "volumechange" | "timeupdate" | "seeking" | "seeked",
        ) => "Event",
        ("audio" | "video", "error") => "ErrorEvent",

        // Default
        _ => "Event",
    };

    if let Some(target) = event_target_type(element) {
        format!("__SvelteEvent<{}, {}>", target, base)
    } else {
        base.to_string()
    }
}

fn event_attribute_name(attr_name: &str) -> Option<&str> {
    if !attr_name.starts_with("on") || attr_name.len() <= 2 {
        return None;
    }
    let event = &attr_name[2..];
    if event.is_empty() {
        None
    } else {
        Some(event)
    }
}

fn split_top_level_comma(expr: &str) -> Option<(String, String)> {
    let mut depth = 0usize;
    let mut in_string: Option<char> = None;
    let mut prev_was_escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut chars = expr.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some((_, '/'))) {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if let Some(quote) = in_string {
            if quote != '`' {
                if prev_was_escape {
                    prev_was_escape = false;
                } else if ch == '\\' {
                    prev_was_escape = true;
                } else if ch == quote {
                    in_string = None;
                }
            } else if prev_was_escape {
                prev_was_escape = false;
            } else if ch == '\\' {
                prev_was_escape = true;
            } else if ch == '`' {
                in_string = None;
            }
            continue;
        }

        if ch == '/' {
            if matches!(chars.peek(), Some((_, '/'))) {
                chars.next();
                in_line_comment = true;
                continue;
            } else if matches!(chars.peek(), Some((_, '*'))) {
                chars.next();
                in_block_comment = true;
                continue;
            }
        }

        if matches!(ch, '\'' | '"' | '`') {
            in_string = Some(ch);
            continue;
        }

        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
            }
            ',' if depth == 0 => {
                let left = expr[..i].trim();
                let right = expr[i + 1..].trim();
                if left.is_empty() || right.is_empty() {
                    return None;
                }
                return Some((left.to_string(), right.to_string()));
            }
            _ => {}
        }
    }

    None
}

fn bind_this_type(element_name: &str) -> String {
    match element_name {
        "svelte:window" => "Window".to_string(),
        "svelte:document" => "Document".to_string(),
        "svelte:body" => "HTMLBodyElement".to_string(),
        "svelte:head" => "HTMLHeadElement".to_string(),
        "svelte:element" => "HTMLElement".to_string(),
        "svelte:component" | "svelte:self" => "any".to_string(),
        _ => {
            if is_component_name(element_name) {
                "any".to_string()
            } else if element_name.contains('-') {
                "HTMLElement".to_string()
            } else {
                format!("ElementTagNameMap[\"{}\"]", element_name)
            }
        }
    }
}

fn action_target_type(element_name: &str) -> String {
    match element_name {
        "svelte:window" => "Window".to_string(),
        "svelte:document" => "Document".to_string(),
        "svelte:body" => "HTMLBodyElement".to_string(),
        "svelte:head" => "HTMLHeadElement".to_string(),
        "svelte:element" => "HTMLElement".to_string(),
        "svelte:component" | "svelte:self" => "HTMLElement".to_string(),
        _ => {
            if is_component_name(element_name) || element_name.contains('-') {
                "HTMLElement".to_string()
            } else {
                format!("ElementTagNameMap[\"{}\"]", element_name)
            }
        }
    }
}

fn is_component_name(name: &str) -> bool {
    name.contains('.')
        || name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
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
        assert!(output.contains("if (condition)"));
    }

    #[test]
    fn test_collect_each_expression() {
        let result = parse("{#each items as item}{item}{/each}");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("items"));
        assert!(output.contains("item"));
    }

    #[test]
    fn test_if_preserves_type_narrowing() {
        let result = parse("{#if user}{user.name}{/if}");
        let output = generate_template_check(&result.document.fragment);
        // Should generate real if statement
        assert!(output.contains("if (user) {"));
        assert!(output.contains("user.name"));
    }

    #[test]
    fn test_each_with_index() {
        let result = parse("{#each items as item, i}{i}: {item}{/each}");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("__svelte_each_indexed"));
    }

    #[test]
    fn test_await_block() {
        let result = parse("{#await promise then value}{value}{/await}");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("Awaited<typeof"));
    }

    #[test]
    fn test_snippet_block() {
        let result = parse("{#snippet button(text)}<button>{text}</button>{/snippet}");
        let output = generate_template_check(&result.document.fragment);
        // Snippet functions use original names so @render tags can call them
        assert!(output.contains("function button(text)"));
    }

    #[test]
    fn test_event_handler_typed() {
        let result = parse("<button on:click={handleClick}>Click</button>");
        let output = generate_template_check(&result.document.fragment);
        assert!(output.contains("MouseEvent"));
        assert!(output.contains("handleClick"));
    }

    #[test]
    fn test_expressions_have_spans() {
        let result = parse("{#if x}{y}{/if}");
        let check = generate_template_check_with_spans(&result.document.fragment);
        assert_eq!(check.expressions.len(), 2);
        assert_eq!(check.expressions[0].expression, "x");
        assert_eq!(check.expressions[0].context, ExpressionContext::IfCondition);
        assert_eq!(check.expressions[1].expression, "y");
        assert_eq!(
            check.expressions[1].context,
            ExpressionContext::Interpolation
        );
    }
}
