//! AST types for Svelte 5.
//!
//! This module defines all AST node types for representing parsed Svelte components.

use smol_str::SmolStr;
use source_map::Span;

/// A complete Svelte document.
#[derive(Debug, Clone, Default)]
pub struct SvelteDocument {
    /// The module-level script (`<script context="module">`).
    pub module_script: Option<Script>,
    /// The instance script (`<script>`).
    pub instance_script: Option<Script>,
    /// The style block (`<style>`).
    pub style: Option<Style>,
    /// The template fragment.
    pub fragment: Fragment,
    /// The span of the entire document.
    pub span: Span,
}

/// A script block.
#[derive(Debug, Clone)]
pub struct Script {
    /// The span of the entire script block including tags.
    pub span: Span,
    /// The span of just the script content.
    pub content_span: Span,
    /// The raw content of the script.
    pub content: String,
    /// The script language (js or ts).
    pub lang: ScriptLang,
    /// The script context (module or default).
    pub context: ScriptContext,
    /// Attributes on the script tag.
    pub attributes: Vec<Attribute>,
}

/// The language of a script block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptLang {
    /// JavaScript (default).
    #[default]
    JavaScript,
    /// TypeScript.
    TypeScript,
}

/// The context of a script block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptContext {
    /// Default instance context.
    #[default]
    Default,
    /// Module context (`context="module"`).
    Module,
}

/// A style block.
#[derive(Debug, Clone)]
pub struct Style {
    /// The span of the entire style block including tags.
    pub span: Span,
    /// The span of just the style content.
    pub content_span: Span,
    /// The raw content of the style.
    pub content: String,
    /// Whether this is a global style.
    pub global: bool,
    /// Attributes on the style tag.
    pub attributes: Vec<Attribute>,
}

/// A template fragment containing child nodes.
#[derive(Debug, Clone, Default)]
pub struct Fragment {
    /// The child nodes.
    pub nodes: Vec<TemplateNode>,
    /// The span of the fragment.
    pub span: Span,
}

/// A node in the template.
#[derive(Debug, Clone)]
pub enum TemplateNode {
    /// An HTML element.
    Element(Element),
    /// A Svelte component.
    Component(Component),
    /// A special Svelte element (`svelte:*`).
    SvelteElement(SvelteElement),
    /// Text content.
    Text(Text),
    /// A comment.
    Comment(Comment),
    /// An expression `{expr}`.
    Expression(ExpressionTag),
    /// An `{@html expr}` tag.
    HtmlTag(HtmlTag),
    /// An `{@const name = expr}` tag.
    ConstTag(ConstTag),
    /// An `{@debug vars}` tag.
    DebugTag(DebugTag),
    /// An `{@render snippet()}` tag.
    RenderTag(RenderTag),
    /// An `{#if}` block.
    IfBlock(IfBlock),
    /// An `{#each}` block.
    EachBlock(EachBlock),
    /// An `{#await}` block.
    AwaitBlock(AwaitBlock),
    /// An `{#key}` block.
    KeyBlock(KeyBlock),
    /// An `{#snippet}` block.
    SnippetBlock(SnippetBlock),
}

impl TemplateNode {
    /// Returns the span of this node.
    pub fn span(&self) -> Span {
        match self {
            TemplateNode::Element(n) => n.span,
            TemplateNode::Component(n) => n.span,
            TemplateNode::SvelteElement(n) => n.span,
            TemplateNode::Text(n) => n.span,
            TemplateNode::Comment(n) => n.span,
            TemplateNode::Expression(n) => n.span,
            TemplateNode::HtmlTag(n) => n.span,
            TemplateNode::ConstTag(n) => n.span,
            TemplateNode::DebugTag(n) => n.span,
            TemplateNode::RenderTag(n) => n.span,
            TemplateNode::IfBlock(n) => n.span,
            TemplateNode::EachBlock(n) => n.span,
            TemplateNode::AwaitBlock(n) => n.span,
            TemplateNode::KeyBlock(n) => n.span,
            TemplateNode::SnippetBlock(n) => n.span,
        }
    }
}

/// An HTML element.
#[derive(Debug, Clone)]
pub struct Element {
    /// The span of the element.
    pub span: Span,
    /// The tag name.
    pub name: SmolStr,
    /// The attributes.
    pub attributes: Vec<Attribute>,
    /// The child nodes.
    pub children: Vec<TemplateNode>,
    /// Whether this is a self-closing tag.
    pub self_closing: bool,
}

/// A Svelte component.
#[derive(Debug, Clone)]
pub struct Component {
    /// The span of the component.
    pub span: Span,
    /// The component name (PascalCase).
    pub name: SmolStr,
    /// The attributes/props.
    pub attributes: Vec<Attribute>,
    /// The child nodes (slot content).
    pub children: Vec<TemplateNode>,
    /// Whether this is a self-closing tag.
    pub self_closing: bool,
}

/// A special Svelte element (`svelte:*`).
#[derive(Debug, Clone)]
pub struct SvelteElement {
    /// The span of the element.
    pub span: Span,
    /// The kind of special element.
    pub kind: SvelteElementKind,
    /// The attributes.
    pub attributes: Vec<Attribute>,
    /// The child nodes.
    pub children: Vec<TemplateNode>,
}

/// The kind of special Svelte element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvelteElementKind {
    /// `<svelte:self>`
    Self_,
    /// `<svelte:component>`
    Component,
    /// `<svelte:element>`
    Element,
    /// `<svelte:window>`
    Window,
    /// `<svelte:document>`
    Document,
    /// `<svelte:body>`
    Body,
    /// `<svelte:head>`
    Head,
    /// `<svelte:options>`
    Options,
    /// `<svelte:fragment>`
    Fragment,
    /// `<svelte:boundary>`
    Boundary,
}

/// Text content.
#[derive(Debug, Clone)]
pub struct Text {
    /// The span of the text.
    pub span: Span,
    /// The text content.
    pub data: String,
    /// Whether this text is only whitespace.
    pub is_whitespace: bool,
}

/// A comment.
#[derive(Debug, Clone)]
pub struct Comment {
    /// The span of the comment.
    pub span: Span,
    /// The comment content (without `<!--` and `-->`).
    pub data: String,
}

/// An expression tag `{expr}`.
#[derive(Debug, Clone)]
pub struct ExpressionTag {
    /// The span of the tag.
    pub span: Span,
    /// The span of just the expression.
    pub expression_span: Span,
    /// The raw expression text.
    pub expression: String,
}

/// An `{@html expr}` tag.
#[derive(Debug, Clone)]
pub struct HtmlTag {
    /// The span of the tag.
    pub span: Span,
    /// The span of just the expression.
    pub expression_span: Span,
    /// The raw expression text.
    pub expression: String,
}

/// An `{@const name = expr}` tag.
#[derive(Debug, Clone)]
pub struct ConstTag {
    /// The span of the tag.
    pub span: Span,
    /// The span of the declaration.
    pub declaration_span: Span,
    /// The raw declaration text.
    pub declaration: String,
}

/// An `{@debug vars}` tag.
#[derive(Debug, Clone)]
pub struct DebugTag {
    /// The span of the tag.
    pub span: Span,
    /// The identifiers to debug.
    pub identifiers: Vec<SmolStr>,
}

/// An `{@render snippet()}` tag.
#[derive(Debug, Clone)]
pub struct RenderTag {
    /// The span of the tag.
    pub span: Span,
    /// The span of just the expression.
    pub expression_span: Span,
    /// The raw expression text (e.g., `name(args)`).
    pub expression: String,
}

/// An `{#if}` block.
#[derive(Debug, Clone)]
pub struct IfBlock {
    /// The span of the entire block.
    pub span: Span,
    /// The span of the condition expression.
    pub condition_span: Span,
    /// The condition expression.
    pub condition: String,
    /// The consequent (then) branch.
    pub consequent: Fragment,
    /// The alternate (else) branch.
    pub alternate: Option<ElseBranch>,
}

/// An else or else-if branch.
#[derive(Debug, Clone)]
pub enum ElseBranch {
    /// An `{:else}` branch.
    Else(Fragment),
    /// An `{:else if}` branch.
    ElseIf(Box<IfBlock>),
}

/// An `{#each}` block.
#[derive(Debug, Clone)]
pub struct EachBlock {
    /// The span of the entire block.
    pub span: Span,
    /// The span of the expression being iterated.
    pub expression_span: Span,
    /// The expression being iterated.
    pub expression: String,
    /// The iteration variable pattern.
    pub context: String,
    /// The span of the context pattern.
    pub context_span: Span,
    /// The index variable name.
    pub index: Option<SmolStr>,
    /// The key expression.
    pub key: Option<EachKey>,
    /// The body of the loop.
    pub body: Fragment,
    /// The else branch (if the list is empty).
    pub fallback: Option<Fragment>,
}

/// A key expression in an `{#each}` block.
#[derive(Debug, Clone)]
pub struct EachKey {
    /// The span of the key expression.
    pub span: Span,
    /// The key expression.
    pub expression: String,
}

/// An `{#await}` block.
#[derive(Debug, Clone)]
pub struct AwaitBlock {
    /// The span of the entire block.
    pub span: Span,
    /// The span of the promise expression.
    pub expression_span: Span,
    /// The promise expression.
    pub expression: String,
    /// The pending state content.
    pub pending: Option<Fragment>,
    /// The resolved state.
    pub then: Option<AwaitThen>,
    /// The rejected state.
    pub catch: Option<AwaitCatch>,
}

/// The resolved state of an await block.
#[derive(Debug, Clone)]
pub struct AwaitThen {
    /// The span of the then block.
    pub span: Span,
    /// The value variable name.
    pub value: Option<SmolStr>,
    /// The content.
    pub body: Fragment,
}

/// The rejected state of an await block.
#[derive(Debug, Clone)]
pub struct AwaitCatch {
    /// The span of the catch block.
    pub span: Span,
    /// The error variable name.
    pub error: Option<SmolStr>,
    /// The content.
    pub body: Fragment,
}

/// An `{#key}` block.
#[derive(Debug, Clone)]
pub struct KeyBlock {
    /// The span of the entire block.
    pub span: Span,
    /// The span of the key expression.
    pub expression_span: Span,
    /// The key expression.
    pub expression: String,
    /// The body content.
    pub body: Fragment,
}

/// An `{#snippet}` block.
#[derive(Debug, Clone)]
pub struct SnippetBlock {
    /// The span of the entire block.
    pub span: Span,
    /// The snippet name.
    pub name: SmolStr,
    /// The span of the parameters.
    pub parameters_span: Span,
    /// The raw parameters text.
    pub parameters: String,
    /// The body content.
    pub body: Fragment,
}

/// An attribute on an element or component.
#[derive(Debug, Clone)]
pub enum Attribute {
    /// A normal attribute `name="value"` or `name={expr}`.
    Normal(NormalAttribute),
    /// A spread attribute `{...obj}`.
    Spread(SpreadAttribute),
    /// A directive `use:action`, `bind:value`, etc.
    Directive(Directive),
    /// A shorthand attribute `{value}`.
    Shorthand(ShorthandAttribute),
    /// An attach attribute `{@attach expr}`.
    Attach(AttachAttribute),
}

impl Attribute {
    /// Returns the span of this attribute.
    pub fn span(&self) -> Span {
        match self {
            Attribute::Normal(a) => a.span,
            Attribute::Spread(a) => a.span,
            Attribute::Directive(a) => a.span,
            Attribute::Shorthand(a) => a.span,
            Attribute::Attach(a) => a.span,
        }
    }
}

/// A normal attribute.
#[derive(Debug, Clone)]
pub struct NormalAttribute {
    /// The span of the attribute.
    pub span: Span,
    /// The attribute name.
    pub name: SmolStr,
    /// The attribute value.
    pub value: AttributeValue,
}

/// An attribute value.
#[derive(Debug, Clone)]
pub enum AttributeValue {
    /// No value (boolean attribute).
    True,
    /// A string literal value.
    Text(TextValue),
    /// An expression value `{expr}`.
    Expression(ExpressionValue),
    /// A concatenation of text and expressions.
    Concat(Vec<AttributeValuePart>),
}

/// A part of a concatenated attribute value.
#[derive(Debug, Clone)]
pub enum AttributeValuePart {
    /// A text part.
    Text(TextValue),
    /// An expression part.
    Expression(ExpressionValue),
}

/// A text value in an attribute.
#[derive(Debug, Clone)]
pub struct TextValue {
    /// The span of the text.
    pub span: Span,
    /// The text content.
    pub value: String,
}

/// An expression value in an attribute.
#[derive(Debug, Clone)]
pub struct ExpressionValue {
    /// The span of the expression (including braces/quotes).
    pub span: Span,
    /// The span of just the expression content.
    pub expression_span: Span,
    /// The raw expression text.
    pub expression: String,
    /// Whether the expression was a quoted string literal (e.g., style:color="red").
    /// If true, the transformer should emit the expression as a string literal.
    pub is_quoted: bool,
}

/// A spread attribute `{...obj}`.
#[derive(Debug, Clone)]
pub struct SpreadAttribute {
    /// The span of the attribute.
    pub span: Span,
    /// The span of the expression.
    pub expression_span: Span,
    /// The expression being spread.
    pub expression: String,
}

/// An attach attribute `{@attach expr}`.
#[derive(Debug, Clone)]
pub struct AttachAttribute {
    /// The span of the attribute.
    pub span: Span,
    /// The span of the expression.
    pub expression_span: Span,
    /// The attachment expression.
    pub expression: String,
}

/// A shorthand attribute `{value}`.
#[derive(Debug, Clone)]
pub struct ShorthandAttribute {
    /// The span of the attribute.
    pub span: Span,
    /// The name (and expression).
    pub name: SmolStr,
}

/// A directive.
#[derive(Debug, Clone)]
pub struct Directive {
    /// The span of the directive.
    pub span: Span,
    /// The directive kind.
    pub kind: DirectiveKind,
    /// The directive name (after the colon).
    pub name: SmolStr,
    /// Modifiers (after `|`).
    pub modifiers: Vec<SmolStr>,
    /// The expression value.
    pub expression: Option<ExpressionValue>,
}

/// The kind of directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveKind {
    /// `on:event`
    On,
    /// `bind:prop`
    Bind,
    /// `class:name`
    Class,
    /// `style:prop`
    StyleDirective,
    /// `use:action`
    Use,
    /// `transition:name`
    Transition,
    /// `in:name`
    In,
    /// `out:name`
    Out,
    /// `animate:name`
    Animate,
    /// `let:name` (slot props)
    Let,
}

#[cfg(test)]
mod tests {
    use super::*;
    use text_size::TextSize;

    #[test]
    fn test_element_creation() {
        let element = Element {
            span: Span::new(TextSize::from(0), TextSize::from(10)),
            name: SmolStr::new("div"),
            attributes: vec![],
            children: vec![],
            self_closing: false,
        };
        assert_eq!(element.name.as_str(), "div");
    }

    #[test]
    fn test_template_node_span() {
        let text = Text {
            span: Span::new(TextSize::from(5), TextSize::from(10)),
            data: "hello".to_string(),
            is_whitespace: false,
        };
        let node = TemplateNode::Text(text);
        assert_eq!(node.span().start, TextSize::from(5));
        assert_eq!(node.span().end, TextSize::from(10));
    }
}
