//! Tests for source map line number accuracy.
//!
//! These tests verify that source positions in the generated TSX correctly
//! map back to their original positions in the Svelte source code.

use source_map::{ByteOffset, LineIndex};
use svelte_parser::parse;
use svelte_transformer::{transform, TransformOptions};

/// Helper to find a substring and return its byte offset.
fn find_offset_of(text: &str, needle: &str) -> Option<u32> {
    text.find(needle).map(|pos| pos as u32)
}

/// Transform source and verify that a generated pattern maps back to the expected source line.
fn verify_line_mapping(source: &str, generated_pattern: &str, expected_source_line: u32) {
    let parsed = parse(source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some("Test.svelte".to_string()),
            source_maps: true,
        },
    );

    // Find the pattern in the generated TSX
    let generated_offset = find_offset_of(&result.tsx_code, generated_pattern)
        .unwrap_or_else(|| panic!("Pattern '{}' not found in generated TSX", generated_pattern));

    // Look up the original position
    let original_offset = result
        .source_map
        .original_position(ByteOffset::from(generated_offset));

    match original_offset {
        Some(offset) => {
            let source_line_index = LineIndex::new(source);
            let source_line_col = source_line_index.line_col(offset).unwrap();
            let actual_line = source_line_col.line + 1; // Convert to 1-indexed

            assert_eq!(
                actual_line,
                expected_source_line,
                "Expected pattern '{}' to map to line {}, but got line {}.\n\
                 Generated TSX around pattern:\n{}\n\
                 Source around expected line:\n{}",
                generated_pattern,
                expected_source_line,
                actual_line,
                get_context(&result.tsx_code, generated_offset as usize, 50),
                get_line_context(source, expected_source_line),
            );
        }
        None => {
            panic!(
                "No source mapping found for pattern '{}' at generated offset {}.\n\
                 Generated TSX around pattern:\n{}",
                generated_pattern,
                generated_offset,
                get_context(&result.tsx_code, generated_offset as usize, 50),
            );
        }
    }
}

/// Get context around a position in text.
fn get_context(text: &str, pos: usize, radius: usize) -> String {
    let start = pos.saturating_sub(radius);
    let end = (pos + radius).min(text.len());
    format!("...{}...", &text[start..end])
}

/// Get a line and surrounding lines from source.
fn get_line_context(text: &str, line: u32) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let idx = (line - 1) as usize;
    let start = idx.saturating_sub(1);
    let end = (idx + 2).min(lines.len());
    lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}: {}", start + i + 1, l))
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// BASIC EXPRESSION LINE NUMBER TESTS
// ============================================================================

#[test]
fn test_simple_expression_line_number() {
    let source = r#"<script>
    let message = "Hello";
</script>

<p>{message}</p>"#;

    // The expression {message} is on line 5
    verify_line_mapping(source, "message;", 5);
}

#[test]
fn test_multiple_expressions_line_numbers() {
    let source = r#"<script>
    let a = 1;
    let b = 2;
    let c = 3;
</script>

<p>{a}</p>
<p>{b}</p>
<p>{c}</p>"#;

    verify_line_mapping(source, "a;", 7);
    verify_line_mapping(source, "b;", 8);
    verify_line_mapping(source, "c;", 9);
}

#[test]
fn test_expression_with_method_call() {
    let source = r#"<script>
    let text = "hello";
</script>

<p>{text.toUpperCase()}</p>"#;

    verify_line_mapping(source, "text.toUpperCase()", 5);
}

// ============================================================================
// COMPONENT NAME LINE NUMBER TESTS
// ============================================================================

#[test]
fn test_component_name_line_number() {
    let source = r#"<script>
    import Button from './Button.svelte';
</script>

<Button>Click me</Button>"#;

    // The component <Button> is on line 5
    verify_line_mapping(source, "Button(null as any", 5);
}

#[test]
fn test_multiple_components_line_numbers() {
    let source = r#"<script>
    import Header from './Header.svelte';
    import Footer from './Footer.svelte';
    import Content from './Content.svelte';
</script>

<Header />
<Content>
    <p>Body</p>
</Content>
<Footer />"#;

    verify_line_mapping(source, "Header(null as any", 7);
    verify_line_mapping(source, "Content(null as any", 8);
    verify_line_mapping(source, "Footer(null as any", 11);
}

#[test]
fn test_nested_component_line_numbers() {
    let source = r#"<script>
    import Outer from './Outer.svelte';
    import Inner from './Inner.svelte';
</script>

<Outer>
    <Inner>
        <span>Nested</span>
    </Inner>
</Outer>"#;

    verify_line_mapping(source, "Outer(null as any", 6);
    verify_line_mapping(source, "Inner(null as any", 7);
}

#[test]
fn test_namespaced_component_line_numbers() {
    let source = r#"<script>
    import * as Dialog from './dialog';
</script>

<Dialog.Root>
    <Dialog.Trigger>Open</Dialog.Trigger>
    <Dialog.Content>
        <Dialog.Title>Title</Dialog.Title>
    </Dialog.Content>
</Dialog.Root>"#;

    verify_line_mapping(source, "Dialog.Root(null as any", 5);
    verify_line_mapping(source, "Dialog.Trigger(null as any", 6);
    verify_line_mapping(source, "Dialog.Content(null as any", 7);
    verify_line_mapping(source, "Dialog.Title(null as any", 8);
}

// ============================================================================
// CONTROL FLOW BLOCK LINE NUMBER TESTS
// ============================================================================

#[test]
fn test_if_block_condition_line_number() {
    let source = r#"<script>
    let show = true;
</script>

{#if show}
    <p>Visible</p>
{/if}"#;

    verify_line_mapping(source, "show)", 5);
}

#[test]
fn test_each_block_expression_line_number() {
    let source = r#"<script>
    let items = [1, 2, 3];
</script>

{#each items as item}
    <p>{item}</p>
{/each}"#;

    // Each block iterable - mapping covers just 'items'
    verify_line_mapping(source, "items;", 5);
    verify_line_mapping(source, "item;", 6);
}

// ============================================================================
// COMPLEX COMPONENT TESTS (like Combobox)
// ============================================================================

#[test]
fn test_generic_component_line_numbers() {
    let source = r#"<script
  lang="ts"
  generics="T extends {label: string; value: string}"
>
    import CheckIcon from './CheckIcon.svelte';

    let { options, selected } = $props<{
        options: T[];
        selected?: T;
    }>();
</script>

{#each options as option}
    <button onclick={() => selected = option}>
        <CheckIcon class="size-4" />
        {option.label}
    </button>
{/each}"#;

    // Component on line 15 - use pattern that only matches template check, not import
    // The mapping starts at 'CheckIcon' in 'CheckIcon(null as any'
    verify_line_mapping(source, "CheckIcon(null", 15);

    // Expression on line 16
    verify_line_mapping(source, "option.label", 16);
}

#[test]
fn test_component_with_spread_props_line_number() {
    let source = r#"<script>
    import Button from './Button.svelte';

    let props = { variant: 'primary' };
</script>

<Button {...props} class="custom">
    Click me
</Button>"#;

    verify_line_mapping(source, "Button(null as any", 7);
    // The mapping is on the expression 'props', after the '...' prefix
    verify_line_mapping(source, "props,", 7);
}

#[test]
fn test_component_with_event_handler_line_number() {
    let source = r#"<script>
    import Button from './Button.svelte';
    let count = 0;
</script>

<Button onclick={() => count++}>
    Count: {count}
</Button>"#;

    verify_line_mapping(source, "Button(null as any", 6);
    verify_line_mapping(source, "() => count++", 6);
}

// ============================================================================
// SNIPPET AND RENDER TAG LINE NUMBER TESTS
// ============================================================================

#[test]
fn test_snippet_body_expression_line_number() {
    let source = r#"<script>
    let items = ['a', 'b'];
</script>

{#snippet item(text)}
    <li>{text}</li>
{/snippet}

{#each items as i}
    {@render item(i)}
{/each}"#;

    // Expression inside snippet on line 6
    verify_line_mapping(source, "text;", 6);
}

// ============================================================================
// ATTRIBUTE EXPRESSION LINE NUMBER TESTS
// ============================================================================

#[test]
fn test_attribute_expression_line_number() {
    let source = r#"<script>
    let isActive = true;
    let className = "btn";
</script>

<button
    class={className}
    disabled={!isActive}
>
    Click
</button>"#;

    // HTML element attributes are emitted as standalone statements
    verify_line_mapping(source, "className;", 7);
    verify_line_mapping(source, "!isActive;", 8);
}

#[test]
fn test_component_prop_expression_line_number() {
    let source = r#"<script>
    import Input from './Input.svelte';
    let value = "";
    let placeholder = "Enter text";
</script>

<Input
    bind:value
    {placeholder}
    maxLength={100}
/>"#;

    verify_line_mapping(source, "placeholder,", 9);
    verify_line_mapping(source, "100,", 10);
}

// ============================================================================
// MULTI-LINE EXPRESSION TESTS
// ============================================================================

#[test]
fn test_multiline_object_expression_line_number() {
    let source = r#"<script>
    import Card from './Card.svelte';
</script>

<Card data={{
    title: "Hello",
    body: "World"
}} />"#;

    // The object expression starts on line 5
    verify_line_mapping(source, "Card(null as any", 5);
}

#[test]
fn test_long_component_with_many_props() {
    let source = r#"<script>
    import ComplexForm from './ComplexForm.svelte';

    let formData = { name: '', email: '' };
    function handleSubmit() {}
    function handleReset() {}
</script>

<ComplexForm
    data={formData}
    onSubmit={handleSubmit}
    onReset={handleReset}
    disabled={false}
    variant="primary"
/>"#;

    verify_line_mapping(source, "ComplexForm(null as any", 9);
    verify_line_mapping(source, "formData,", 10);
    verify_line_mapping(source, "handleSubmit,", 11);
    verify_line_mapping(source, "handleReset,", 12);
}

// ============================================================================
// NESTED CONTEXT TESTS
// ============================================================================

#[test]
fn test_component_inside_element() {
    let source = r#"<script>
    import Icon from './Icon.svelte';
</script>

<div>
    <Icon class="size-4" />
</div>"#;

    verify_line_mapping(source, "Icon(null", 6);
}

#[test]
fn test_component_inside_each_block() {
    let source = r#"<script>
    import Icon from './Icon.svelte';
    let items = [1, 2, 3];
</script>

{#each items as item}
    <Icon class="size-4" />
{/each}"#;

    verify_line_mapping(source, "Icon(null", 7);
}

#[test]
fn test_component_inside_button_with_onclick() {
    let source = r#"<script>
    import Icon from './Icon.svelte';
    let count = 0;
</script>

<button onclick={() => count++}>
    <Icon class="size-4" />
</button>"#;

    verify_line_mapping(source, "Icon(null", 7);
}

#[test]
fn test_component_inside_each_inside_button() {
    let source = r#"<script>
    import Icon from './Icon.svelte';
    let items = [1, 2, 3];
</script>

{#each items as item}
    <button onclick={() => console.log(item)}>
        <Icon class="size-4" />
    </button>
{/each}"#;

    verify_line_mapping(source, "Icon(null", 8);
}

#[test]
fn test_component_in_generic_context() {
    // Test with generics but without $props
    let source = r#"<script lang="ts" generics="T">
    import Icon from './Icon.svelte';
    let items: T[] = [];
</script>

{#each items as item}
    <button onclick={() => console.log(item)}>
        <Icon class="size-4" />
    </button>
{/each}"#;

    verify_line_mapping(source, "Icon(null", 8);
}

#[test]
fn test_component_with_props_no_generics() {
    // Test with $props but without generics
    let source = r#"<script lang="ts">
    import Icon from './Icon.svelte';
    let { items }: { items: string[] } = $props();
</script>

{#each items as item}
    <button onclick={() => console.log(item)}>
        <Icon class="size-4" />
    </button>
{/each}"#;

    verify_line_mapping(source, "Icon(null", 8);
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_component_immediately_after_script() {
    let source = r#"<script>
    import A from './A.svelte';
</script>
<A />"#;

    verify_line_mapping(source, "A(null as any", 4);
}

#[test]
fn test_expression_on_first_line() {
    let source = r#"{someGlobal}"#;

    verify_line_mapping(source, "someGlobal", 1);
}

#[test]
fn test_deeply_nested_components() {
    let source = r#"<script>
    import A from './A.svelte';
    import B from './B.svelte';
    import C from './C.svelte';
    import D from './D.svelte';
</script>

<A>
    <B>
        <C>
            <D>
                <span>Deep</span>
            </D>
        </C>
    </B>
</A>"#;

    verify_line_mapping(source, "A(null as any", 8);
    verify_line_mapping(source, "B(null as any", 9);
    verify_line_mapping(source, "C(null as any", 10);
    verify_line_mapping(source, "D(null as any", 11);
}
