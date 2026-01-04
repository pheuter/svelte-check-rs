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
            ..Default::default()
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

    // HTML element attributes are emitted inside __svelte_create_element checks
    verify_line_mapping(source, "class: className", 7);
    verify_line_mapping(source, "disabled: !isActive", 8);
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
// SNIPPET PARAMETER SOURCE MAPPING TESTS
// ============================================================================
// These tests ensure that snippet parameters map back to their original
// source positions, preventing the regression where errors in snippet
// parameters would show incorrect line numbers.

#[test]
fn test_snippet_parameter_line_number() {
    let source = r#"<script>
    import Button from './Button.svelte';
</script>

<Button>
    {#snippet icon({ size })}
        <span style="font-size: {size}px">★</span>
    {/snippet}
</Button>"#;

    // The destructured parameter { size } should map to line 6
    verify_line_mapping(source, "{ size }", 6);
}

#[test]
fn test_snippet_parameter_with_multiple_params() {
    let source = r#"<script>
    import List from './List.svelte';
</script>

<List>
    {#snippet item({ label, value, index })}
        <li>{index}: {label} = {value}</li>
    {/snippet}
</List>"#;

    // The destructured parameters should map to line 6
    verify_line_mapping(source, "{ label, value, index }", 6);
}

#[test]
fn test_snippet_parameter_in_component_child_prop() {
    // This is the exact pattern that caused the original bug
    let source = r#"<script>
    import * as Tooltip from './tooltip';
    import Info from './Info.svelte';
</script>

<Tooltip.Root>
    <Tooltip.Trigger>
        {#snippet child({ props })}
            <span {...props}>
                <Info class="h-3.5 w-3.5" />
            </span>
        {/snippet}
    </Tooltip.Trigger>
</Tooltip.Root>"#;

    // The { props } parameter should map to line 8
    verify_line_mapping(source, "{ props }", 8);
}

#[test]
fn test_snippet_parameter_with_type_annotation() {
    let source = r#"<script lang="ts">
    import Dialog from './Dialog.svelte';
    type ButtonProps = { variant: string };
</script>

<Dialog>
    {#snippet trigger({ props }: { props: ButtonProps })}
        <button {...props}>Open</button>
    {/snippet}
</Dialog>"#;

    // Parameters with type annotations should still map correctly
    verify_line_mapping(source, "{ props }", 7);
}

#[test]
fn test_snippet_parameter_nested_in_each() {
    let source = r#"<script>
    import Card from './Card.svelte';
    let items = [1, 2, 3];
</script>

{#each items as item}
    <Card>
        {#snippet header({ title })}
            <h2>{title}</h2>
        {/snippet}
    </Card>
{/each}"#;

    // The { title } parameter should map to line 8
    verify_line_mapping(source, "{ title }", 8);
}

#[test]
fn test_snippet_parameter_nested_in_if() {
    let source = r#"<script>
    import Modal from './Modal.svelte';
    let showModal = true;
</script>

{#if showModal}
    <Modal>
        {#snippet footer({ close })}
            <button onclick={close}>Close</button>
        {/snippet}
    </Modal>
{/if}"#;

    // The { close } parameter should map to line 8
    verify_line_mapping(source, "{ close }", 8);
}

#[test]
fn test_snippet_parameter_deeply_nested() {
    let source = r#"<script>
    import * as Dialog from './dialog';
</script>

<Dialog.Root>
    <Dialog.Content>
        <Dialog.Header>
            {#snippet custom({ className })}
                <div class={className}>Custom Header</div>
            {/snippet}
        </Dialog.Header>
    </Dialog.Content>
</Dialog.Root>"#;

    // Deeply nested snippet parameters should map correctly
    verify_line_mapping(source, "{ className }", 8);
}

#[test]
fn test_multiple_snippets_with_parameters() {
    let source = r#"<script>
    import Table from './Table.svelte';
</script>

<Table>
    {#snippet header({ column })}
        <th>{column.name}</th>
    {/snippet}
    {#snippet cell({ row, column })}
        <td>{row[column.key]}</td>
    {/snippet}
    {#snippet footer({ total })}
        <tfoot>{total}</tfoot>
    {/snippet}
</Table>"#;

    // Each snippet parameter should map to its correct line
    verify_line_mapping(source, "{ column }", 6);
    verify_line_mapping(source, "{ row, column }", 9);
    verify_line_mapping(source, "{ total }", 12);
}

#[test]
fn test_snippet_body_and_parameter_both_mapped() {
    let source = r#"<script>
    import Button from './Button.svelte';
</script>

<Button>
    {#snippet leading({ size })}
        <span>{size}</span>
    {/snippet}
</Button>"#;

    // Both the parameter and the expression inside should map correctly
    verify_line_mapping(source, "{ size }", 6);
    // The expression in the body is emitted as a standalone statement with semicolon
    verify_line_mapping(source, "size;", 7);
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

// ============================================================================
// PROP NAME LINE NUMBER TESTS
// ============================================================================
// These tests verify that component prop names (not just values) are correctly
// mapped back to their original source positions. This is important because
// TypeScript often reports errors at the property name position, not the value.

#[test]
fn test_prop_name_with_expression_value() {
    let source = r#"<script>
    import Input from './Input.svelte';
    let val = "";
</script>

<Input
    placeholder={val}
/>"#;

    // The prop name "placeholder" should map to line 7
    verify_line_mapping(source, "placeholder:", 7);
}

#[test]
fn test_prop_name_with_text_value() {
    let source = r#"<script>
    import Input from './Input.svelte';
</script>

<Input
    placeholder="Enter text"
/>"#;

    // The prop name "placeholder" should map to line 6
    verify_line_mapping(source, "placeholder:", 6);
}

#[test]
fn test_prop_name_with_boolean_value() {
    let source = r#"<script>
    import Button from './Button.svelte';
</script>

<Button
    disabled
/>"#;

    // The prop name "disabled" should map to line 6
    verify_line_mapping(source, "disabled:", 6);
}

#[test]
fn test_bind_directive_name() {
    let source = r#"<script>
    import Input from './Input.svelte';
    let text = "";
</script>

<Input
    bind:myValue={text}
/>"#;

    // The bind prop name "myValue" should map to line 7 (where "myValue" appears after "bind:")
    // Using "myValue" to avoid matching helper function patterns
    verify_line_mapping(source, "myValue:", 7);
}

#[test]
fn test_bind_pair_expression_line_number() {
    let source = r#"<script>
    const sidebar = {
        openMobile: false,
        setOpenMobile: (v: boolean) => {}
    };
</script>

<Sheet.Root
    bind:open={() => sidebar.openMobile,
      (v) => sidebar.setOpenMobile(v)}
/>"#;

    let getter_line = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("sidebar.openMobile"))
        .map(|(i, _)| (i + 1) as u32)
        .unwrap();

    let setter_line = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("sidebar.setOpenMobile"))
        .map(|(i, _)| (i + 1) as u32)
        .unwrap();

    verify_line_mapping(source, "sidebar.openMobile", getter_line);
    verify_line_mapping(source, "sidebar.setOpenMobile", setter_line);
}

#[test]
fn test_multiple_prop_names() {
    let source = r#"<script>
    import Form from './Form.svelte';
    let data = {};
    let onSubmit = () => {};
</script>

<Form
    formData={data}
    handler={onSubmit}
    disabled={false}
    title="My Form"
/>"#;

    // Each prop name should map to its correct line
    verify_line_mapping(source, "formData:", 8);
    verify_line_mapping(source, "handler:", 9);
    verify_line_mapping(source, "disabled:", 10);
    verify_line_mapping(source, "title:", 11);
}

#[test]
fn test_prop_name_in_large_file() {
    // Simulate a file with many lines before the component to ensure
    // line numbers are correctly calculated even at higher line numbers
    let mut source = String::from("<script lang=\"ts\">\n");
    for i in 0..80 {
        source.push_str(&format!("  import {{ item{} }} from './lib{}';\n", i, i));
    }
    source.push_str("  import Combobox from './Combobox.svelte';\n");
    source.push_str("</script>\n\n");

    // Add many lines of template content
    for i in 0..380 {
        source.push_str(&format!("  <div>Field {}</div>\n", i));
    }

    // Add the component with props on a high line number
    // Using unique prop names to avoid matching helper function patterns
    source.push_str(
        r#"<Combobox
    myMode="multiple"
    myOptions={someOptions}
    bind:selectedItem={selectedValue}
/>"#,
    );

    // Find the actual line numbers
    let options_line = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("myOptions={"))
        .map(|(i, _)| (i + 1) as u32)
        .unwrap();

    let bind_line = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("bind:selectedItem"))
        .map(|(i, _)| (i + 1) as u32)
        .unwrap();

    // Verify prop name mappings at high line numbers
    verify_line_mapping(&source, "myOptions:", options_line);
    verify_line_mapping(&source, "selectedItem:", bind_line);
}
