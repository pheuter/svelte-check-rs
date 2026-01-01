//! Template to TSX transformation.
//!
//! Converts Svelte template nodes to TSX for type-checking expressions.
//! This module generates proper TypeScript that preserves type narrowing
//! for control flow blocks and tracks spans for source mapping.

use source_map::Span;
use svelte_parser::*;

/// Transform store subscriptions in an expression.
///
/// In Svelte, `$storeName` is shorthand for subscribing to a store and getting its value.
/// We transform `$storeName` to `__svelte_store_get(storeName)` so TypeScript sees the
/// dereferenced value type, not the store type.
///
/// This only applies to store subscriptions (identifier after $), not to:
/// - Runes like `$state()`, `$derived()` (have parentheses)
/// - Special variables like `$$props`, `$$slots`
fn transform_store_subscriptions(expr: &str) -> String {
    let mut result = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            // Check if this is a store subscription
            if let Some(&next) = chars.peek() {
                // Skip $$ patterns ($$props, $$slots, etc.)
                if next == '$' {
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
                        result.push(ch);
                        result.push_str(&identifier);
                    } else {
                        // It's a store subscription - wrap with helper function
                        result.push_str("__svelte_store_get(");
                        result.push_str(&identifier);
                        result.push(')');
                    }
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
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

/// Result of template TSX generation.
#[derive(Debug)]
pub struct TemplateCheckResult {
    /// The generated TSX code.
    pub code: String,
    /// Expressions with their spans for source mapping.
    pub expressions: Vec<TemplateExpression>,
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
        };
    }

    let mut code = String::new();
    code.push_str("\n// === TEMPLATE TYPE-CHECK BLOCK ===\n");
    code.push_str("// This is never executed, just type-checked\n");
    code.push_str("function __svelte_template_check__() {\n");
    code.push_str(&ctx.output);
    code.push_str("}\n");

    TemplateCheckResult {
        code,
        expressions: ctx.expressions,
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
    indent: usize,
    /// Counter for generating unique variable names.
    counter: usize,
}

impl TemplateContext {
    fn new() -> Self {
        Self {
            output: String::new(),
            expressions: Vec::new(),
            indent: 1,
            counter: 0,
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

    fn emit_expression(&mut self, expr: &str, span: Span, context: ExpressionContext) {
        // Transform store subscriptions: $storeName -> storeName
        let transformed = transform_store_subscriptions(expr);

        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span,
            context,
        });
        // Wrap object literals in parentheses to prevent TypeScript from
        // interpreting them as blocks (e.g., `{foo: bar}` would be a label)
        let trimmed = transformed.trim_start();
        if trimmed.starts_with('{') && !trimmed.starts_with("{{") {
            self.emit(&format!("({});", transformed));
        } else {
            self.emit(&format!("{};", transformed));
        }
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
                self.generate_component(&comp.name, &comp.attributes, &comp.children);
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
                let transformed = transform_store_subscriptions(&tag.declaration);
                self.emit(&format!("const {};", transformed));
            }
            TemplateNode::DebugTag(tag) => {
                for ident in &tag.identifiers {
                    let transformed = transform_store_subscriptions(ident);
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
                    self.generate_attribute_value(&a.value);
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
                let transformed = transform_store_subscriptions(&expr.expression);
                self.expressions.push(TemplateExpression {
                    expression: transformed.clone(),
                    span: expr.expression_span,
                    context,
                });
                // Generate typed event handler check
                self.emit(&format!(
                    "const __event_{}: (e: {}) => void = {};",
                    id, event_type, transformed
                ));
            } else if directive.kind == DirectiveKind::Bind {
                // For bindings, check the variable
                self.emit_expression(&expr.expression, expr.expression_span, context);
            } else {
                self.emit_expression(&expr.expression, expr.expression_span, context);
            }
        }
    }

    fn generate_component(&mut self, name: &str, attrs: &[Attribute], children: &[TemplateNode]) {
        // Collect props for the component
        // First pass: collect all prop-like attributes (Normal, Shorthand, Spread)
        // into a props object, then close it before handling directives
        let id = self.next_id();
        let mut has_props = false;

        // Separate snippets from other children - snippets become inline props
        let (snippets, other_children): (Vec<_>, Vec<_>) = children
            .iter()
            .partition(|node| matches!(node, TemplateNode::SnippetBlock(_)));

        // First pass: build the props object with Normal, Shorthand, and Spread attributes
        for attr in attrs {
            match attr {
                Attribute::Normal(a) => {
                    if !has_props {
                        self.emit(&format!("const __props_{} = {{", id));
                        has_props = true;
                    }
                    self.indent += 1;
                    match &a.value {
                        AttributeValue::Expression(expr) => {
                            let transformed = transform_store_subscriptions(&expr.expression);
                            self.expressions.push(TemplateExpression {
                                expression: transformed.clone(),
                                span: expr.expression_span,
                                context: ExpressionContext::Attribute,
                            });
                            self.emit(&format!("{}: {},", format_prop_name(&a.name), transformed));
                        }
                        AttributeValue::Text(t) => {
                            self.emit(&format!("{}: \"{}\",", format_prop_name(&a.name), t.value));
                        }
                        AttributeValue::True => {
                            self.emit(&format!("{}: true,", format_prop_name(&a.name)));
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
                                        let transformed =
                                            transform_store_subscriptions(&expr.expression);
                                        self.expressions.push(TemplateExpression {
                                            expression: transformed.clone(),
                                            span: expr.expression_span,
                                            context: ExpressionContext::Attribute,
                                        });
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
                            self.emit(&format!("{}: {},", format_prop_name(&a.name), template));
                        }
                    }
                    self.indent -= 1;
                }
                Attribute::Spread(s) => {
                    // Include spreads in the props object
                    if !has_props {
                        self.emit(&format!("const __props_{} = {{", id));
                        has_props = true;
                    }
                    self.indent += 1;
                    let transformed = transform_store_subscriptions(&s.expression);
                    self.expressions.push(TemplateExpression {
                        expression: transformed.clone(),
                        span: s.expression_span,
                        context: ExpressionContext::Spread,
                    });
                    self.emit(&format!("...{},", transformed));
                    self.indent -= 1;
                }
                Attribute::Shorthand(s) => {
                    if !has_props {
                        self.emit(&format!("const __props_{} = {{", id));
                        has_props = true;
                    }
                    self.indent += 1;
                    let transformed = transform_store_subscriptions(&s.name);
                    self.expressions.push(TemplateExpression {
                        expression: transformed.clone(),
                        span: s.span,
                        context: ExpressionContext::Attribute,
                    });
                    self.emit(&format!("{},", transformed));
                    self.indent -= 1;
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
                if !has_props {
                    self.emit(&format!("const __props_{} = {{", id));
                    has_props = true;
                }
                self.indent += 1;
                // Generate snippet as arrow function: name: (params) => { body }
                self.emit(&format!("{}: ({}) => {{", block.name, block.parameters));
                self.indent += 1;
                self.generate_fragment(&block.body);
                self.indent -= 1;
                self.emit("},");
                self.indent -= 1;
            }
        }

        // Close the props object if we started one
        if has_props {
            // Use type assertion to enable proper type inference for callbacks
            // The 'as' assertion allows TypeScript to infer parameter types in arrow functions
            // while not requiring all props to be present
            self.emit(&format!("}} as ComponentProps<typeof {}>;", name));
        }

        // Second pass: handle directives separately (bindings, events, etc.)
        for attr in attrs {
            if let Attribute::Directive(d) = attr {
                self.generate_directive(name, d);
            }
        }

        // Process non-snippet children
        for child in &other_children {
            self.generate_node(child);
        }
    }

    fn generate_if_block(&mut self, block: &IfBlock) {
        // Use real if statements to preserve type narrowing
        let transformed = transform_store_subscriptions(&block.condition);
        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span: block.condition_span,
            context: ExpressionContext::IfCondition,
        });
        self.emit(&format!("if ({}) {{", transformed));
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
        let transformed = transform_store_subscriptions(&block.condition);
        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span: block.condition_span,
            context: ExpressionContext::IfCondition,
        });
        self.output.push_str(&format!("if ({}) {{\n", transformed));
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

        // Emit the iterable expression
        let transformed = transform_store_subscriptions(&block.expression);
        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span: block.expression_span,
            context: ExpressionContext::EachIterable,
        });

        // Generate a for loop that introduces the loop variable
        self.emit(&format!("const __each_{} = {};", id, transformed));

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

        let transformed = transform_store_subscriptions(&block.expression);
        self.expressions.push(TemplateExpression {
            expression: transformed.clone(),
            span: block.expression_span,
            context: ExpressionContext::AwaitPromise,
        });

        self.emit(&format!("const __await_{} = {};", id, transformed));

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
            if let Some(ref value) = then.value {
                self.emit(&format!(
                    "const {}: Awaited<typeof __await_{}> = await __await_{};",
                    value, id, id
                ));
            }
            self.generate_fragment(&then.body);
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
        self.emit(&format!("function {}({}) {{", block.name, block.parameters));
        self.indent += 1;
        self.generate_fragment(&block.body);
        self.indent -= 1;
        self.emit("}");
    }
}

/// Get the TypeScript event type for a given element and event name.
fn get_event_type(element: &str, event: &str) -> &'static str {
    match (element, event) {
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
