//! Snapshot tests for the Svelte to TSX transformer.
//!
//! These tests verify the transformer output against known-good snapshots.

use svelte_parser::parse;
use svelte_transformer::{transform, TransformOptions};

fn transform_snapshot(name: &str, source: &str) {
    let parsed = parse(source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some("Test.svelte".to_string()),
            source_maps: true,
            ..Default::default()
        },
    );

    let output = format!(
        "=== Source ===\n{}\n\n=== TSX Output ===\n{}\n\n=== Source Map Mappings: {} ===",
        source,
        result.tsx_code,
        result.source_map.len()
    );
    insta::assert_snapshot!(name, output);
}

/// Transform snapshot with detailed source map output for testing line/column accuracy
fn transform_snapshot_with_source_map(name: &str, source: &str) {
    let parsed = parse(source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some("Test.svelte".to_string()),
            source_maps: true,
            ..Default::default()
        },
    );

    // Format source map entries with their spans for debugging
    let mut mappings = String::new();
    for (i, mapping) in result.source_map.mappings().enumerate() {
        mappings.push_str(&format!(
            "  {}: generated {}..{} -> original {}..{}\n",
            i,
            u32::from(mapping.generated.start),
            u32::from(mapping.generated.end),
            u32::from(mapping.original.start),
            u32::from(mapping.original.end)
        ));
    }

    let output = format!(
        "=== Source ===\n{}\n\n=== TSX Output ===\n{}\n\n=== Source Map Mappings ({}) ===\n{}",
        source,
        result.tsx_code,
        result.source_map.len(),
        mappings
    );
    insta::assert_snapshot!(name, output);
}

fn transform_snapshot_with_filename(name: &str, filename: &str, source: &str) {
    let parsed = parse(source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some(filename.to_string()),
            source_maps: true,
            ..Default::default()
        },
    );

    let output = format!(
        "=== Source ({}) ===\n{}\n\n=== TSX Output ===\n{}\n\n=== Source Map Mappings: {} ===",
        filename,
        source,
        result.tsx_code,
        result.source_map.len()
    );
    insta::assert_snapshot!(name, output);
}

// ============================================================================
// RUNE TRANSFORMATION TESTS
// ============================================================================

#[test]
fn test_transform_state_rune() {
    transform_snapshot(
        "state_rune",
        r#"<script lang="ts">
    let count = $state(0);
    let name = $state("hello");
    let data = $state({ x: 1, y: 2 });
</script>

<p>{count} - {name}</p>"#,
    );
}

#[test]
fn test_transform_state_raw_rune() {
    transform_snapshot(
        "state_raw_rune",
        r#"<script lang="ts">
    let items = $state.raw([1, 2, 3]);
</script>

<p>{items.length}</p>"#,
    );
}

#[test]
fn test_transform_derived_rune() {
    transform_snapshot(
        "derived_rune",
        r#"<script lang="ts">
    let count = $state(0);
    let doubled = $derived(count * 2);
    let computed = $derived.by(() => count * 3);
</script>

<p>{doubled} - {computed}</p>"#,
    );
}

#[test]
fn test_transform_effect_rune() {
    transform_snapshot(
        "effect_rune",
        r#"<script lang="ts">
    let count = $state(0);

    $effect(() => {
        console.log(count);
    });

    $effect.pre(() => {
        console.log('pre');
    });
</script>

<button>{count}</button>"#,
    );
}

#[test]
fn test_transform_props_rune() {
    transform_snapshot(
        "props_rune",
        r#"<script lang="ts">
    let { name, count = 0 } = $props<{ name: string; count?: number }>();
</script>

<p>{name}: {count}</p>"#,
    );
}

#[test]
fn test_transform_bindable_rune() {
    transform_snapshot(
        "bindable_rune",
        r#"<script lang="ts">
    let { value = $bindable("default") } = $props<{ value?: string }>();
</script>

<input bind:value />"#,
    );
}

#[test]
fn test_transform_inspect_rune() {
    transform_snapshot(
        "inspect_rune",
        r#"<script lang="ts">
    let count = $state(0);
    $inspect(count);
</script>

<p>{count}</p>"#,
    );
}

#[test]
fn test_transform_host_rune() {
    transform_snapshot(
        "host_rune",
        r#"<script lang="ts">
    const el = $host();
</script>

<p>Host element</p>"#,
    );
}

// ============================================================================
// PROPS TYPE EXTRACTION TESTS
// ============================================================================

#[test]
fn test_props_with_type_annotation() {
    transform_snapshot_with_filename(
        "props_type_annotation",
        "Counter.svelte",
        r#"<script lang="ts">
    type Props = { count: number; onchange?: (n: number) => void };
    let { count, onchange }: Props = $props();
</script>

<button onclick={() => onchange?.(count + 1)}>{count}</button>"#,
    );
}

#[test]
fn test_props_with_generic() {
    transform_snapshot_with_filename(
        "props_generic",
        "Greeting.svelte",
        r#"<script lang="ts">
    let { name, greeting = "Hello" } = $props<{ name: string; greeting?: string }>();
</script>

<p>{greeting}, {name}!</p>"#,
    );
}

#[test]
fn test_props_with_rest() {
    transform_snapshot(
        "props_rest",
        r#"<script lang="ts">
    let { class: className, ...rest } = $props<{ class?: string } & Record<string, unknown>>();
</script>

<div class={className} {...rest}>Content</div>"#,
    );
}

#[test]
fn test_props_with_comment_before() {
    transform_snapshot(
        "props_comment_before",
        r#"<script lang="ts">
    // See: issue-28 - comment before props
    const flag = true;

    let { children } = $props();
</script>

{@render children()}"#,
    );
}

#[test]
fn test_props_with_rest_loosened_annotation() {
    transform_snapshot(
        "props_rest_loosened_annotation",
        r#"<script lang="ts">
    type Foo = { foo: string };
    type Bar<T> = T & Record<string, unknown>;
    let { foo, ...rest }: Foo & Bar<{ baz: string }> = $props();
</script>

<div {...rest}>{foo}</div>"#,
    );
}

// ============================================================================
// TEMPLATE EXPRESSION TESTS
// ============================================================================

#[test]
fn test_template_simple_expression() {
    transform_snapshot(
        "template_expression",
        r#"<script>
    let message = "Hello";
</script>

<p>{message}</p>
<span>{message.toUpperCase()}</span>"#,
    );
}

#[test]
fn test_template_if_block() {
    transform_snapshot(
        "template_if_block",
        r#"<script lang="ts">
    let show = $state(true);
    let user: { name: string } | null = $state(null);
</script>

{#if show}
    <p>Visible</p>
{/if}

{#if user}
    <p>{user.name}</p>
{/if}"#,
    );
}

#[test]
fn test_template_if_else() {
    transform_snapshot(
        "template_if_else",
        r#"<script lang="ts">
    let status: 'loading' | 'ready' | 'error' = $state('loading');
</script>

{#if status === 'loading'}
    <p>Loading...</p>
{:else if status === 'error'}
    <p>Error!</p>
{:else}
    <p>Ready</p>
{/if}"#,
    );
}

#[test]
fn test_template_each_block() {
    transform_snapshot(
        "template_each_block",
        r#"<script lang="ts">
    let items = $state([{ id: 1, name: 'a' }, { id: 2, name: 'b' }]);
</script>

{#each items as item, index (item.id)}
    <p>{index}: {item.name}</p>
{/each}"#,
    );
}

#[test]
fn test_template_each_with_else() {
    transform_snapshot(
        "template_each_else",
        r#"<script lang="ts">
    let items: string[] = $state([]);
</script>

{#each items as item}
    <p>{item}</p>
{:else}
    <p>No items</p>
{/each}"#,
    );
}

#[test]
fn test_template_await_block() {
    transform_snapshot(
        "template_await_block",
        r#"<script lang="ts">
    let promise = fetch('/api/data').then(r => r.json());
</script>

{#await promise}
    <p>Loading...</p>
{:then data}
    <p>{data.message}</p>
{:catch error}
    <p>Error: {error.message}</p>
{/await}"#,
    );
}

#[test]
fn test_template_snippet_block() {
    transform_snapshot(
        "template_snippet",
        r#"<script lang="ts">
    let items = $state(['a', 'b', 'c']);
</script>

{#snippet item(text: string)}
    <li>{text}</li>
{/snippet}

<ul>
    {#each items as i}
        {@render item(i)}
    {/each}
</ul>"#,
    );
}

#[test]
fn test_template_snippet_generic_header() {
    transform_snapshot(
        "template_snippet_generic_header",
        r#"{#snippet relationshipFormFields<
  T extends { relationshipType: string; shortName?: string | undefined },
>(args: T)}
    <div>{args.relationshipType}</div>
{/snippet}

{@render relationshipFormFields({ relationshipType: "friend" })}"#,
    );
}

#[test]
fn test_component_snippet_generic_header_as_prop() {
    transform_snapshot(
        "component_snippet_generic_header",
        r#"<Table>
    {#snippet row<
        T extends { id: string },
    >(item)}
        <span>{item.id}</span>
    {/snippet}
</Table>"#,
    );
}

// ============================================================================
// EVENT HANDLER TESTS
// ============================================================================

#[test]
fn test_event_handler_inline() {
    transform_snapshot(
        "event_inline",
        r#"<script lang="ts">
    let count = $state(0);
</script>

<button onclick={() => count++}>Increment</button>"#,
    );
}

#[test]
fn test_event_handler_reference() {
    transform_snapshot(
        "event_reference",
        r#"<script lang="ts">
    function handleClick(e: MouseEvent) {
        console.log(e.target);
    }
</script>

<button onclick={handleClick}>Click me</button>"#,
    );
}

#[test]
fn test_event_handler_on_directive() {
    transform_snapshot(
        "event_on_directive",
        r#"<script lang="ts">
    function handleInput(e: Event) {
        console.log((e.target as HTMLInputElement).value);
    }
</script>

<input on:input={handleInput} />"#,
    );
}

// ============================================================================
// BINDING TESTS
// ============================================================================

#[test]
fn test_binding_value() {
    transform_snapshot(
        "binding_value",
        r#"<script lang="ts">
    let text = $state("");
</script>

<input bind:value={text} />"#,
    );
}

#[test]
fn test_binding_checked() {
    transform_snapshot(
        "binding_checked",
        r#"<script lang="ts">
    let checked = $state(false);
</script>

<input type="checkbox" bind:checked />"#,
    );
}

// ============================================================================
// COMPLEX COMPONENT TESTS
// ============================================================================

#[test]
fn test_complete_counter_component() {
    transform_snapshot_with_filename(
        "counter_complete",
        "Counter.svelte",
        r#"<script lang="ts">
    let { initial = 0 } = $props<{ initial?: number }>();
    let count = $state(initial);

    const doubled = $derived(count * 2);

    function increment() {
        count += 1;
    }

    $effect(() => {
        console.log('Count changed:', count);
    });
</script>

<div class="counter">
    <button onclick={increment}>
        Count: {count}
    </button>
    <p>Doubled: {doubled}</p>
</div>"#,
    );
}

#[test]
fn test_complete_form_component() {
    transform_snapshot_with_filename(
        "form_complete",
        "LoginForm.svelte",
        r#"<script lang="ts">
    let { onsubmit } = $props<{ onsubmit: (data: { email: string; password: string }) => void }>();

    let email = $state("");
    let password = $state("");
    let isValid = $derived(email.includes('@') && password.length >= 8);

    function handleSubmit(e: SubmitEvent) {
        e.preventDefault();
        if (isValid) {
            onsubmit({ email, password });
        }
    }
</script>

<form onsubmit={handleSubmit}>
    <input type="email" bind:value={email} placeholder="Email" />
    <input type="password" bind:value={password} placeholder="Password" />
    <button disabled={!isValid}>Login</button>
</form>"#,
    );
}

#[test]
fn test_module_script() {
    transform_snapshot(
        "module_script",
        r#"<script context="module" lang="ts">
    export const VERSION = "1.0.0";
    export function helper(x: number): number {
        return x * 2;
    }
</script>

<script lang="ts">
    let count = $state(0);
</script>

<p>Version: {VERSION}</p>"#,
    );
}

#[test]
fn test_javascript_component() {
    transform_snapshot_with_filename(
        "javascript_component",
        "Simple.svelte",
        r#"<script>
    let count = $state(0);
</script>

<button onclick={() => count++}>{count}</button>"#,
    );
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_empty_component() {
    transform_snapshot("empty_component", "");
}

#[test]
fn test_only_template() {
    transform_snapshot("only_template", "<div>Static content</div>");
}

#[test]
fn test_nested_expressions() {
    transform_snapshot(
        "nested_expressions",
        r#"<script lang="ts">
    let items = $state([{ children: [1, 2, 3] }]);
</script>

{#each items as item}
    {#each item.children as child}
        <span>{child}</span>
    {/each}
{/each}"#,
    );
}

#[test]
fn test_special_characters_in_strings() {
    transform_snapshot(
        "special_chars",
        r#"<script lang="ts">
    let message = $state("Hello \"world\" with 'quotes' and \n newlines");
</script>

<p>{message}</p>"#,
    );
}

#[test]
fn test_layout_with_children() {
    transform_snapshot_with_filename(
        "layout_children",
        "+layout.svelte",
        r#"<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

{@render children()}"#,
    );
}

#[test]
fn test_snippet_with_store_typeof() {
    transform_snapshot(
        "snippet_store_typeof",
        r#"<script lang="ts">
    import { formStore } from './stores';
    const formData = formStore;
</script>

{#snippet mySnippet(value: typeof $formData.prop)}
    <div>{value}</div>
{/snippet}

{@render mySnippet($formData.prop)}"#,
    );
}

#[test]
fn test_store_subscription_in_script_function() {
    transform_snapshot(
        "store_in_script_function",
        r#"<script lang="ts">
    import { formStore } from './stores';
    const { form: formData } = formStore;

    function updateEndTime() {
        $formData.endTime = $formData.startTime ? 'test' : null;
    }
</script>

<button onclick={() => updateEndTime()}>Update</button>"#,
    );
}

// ============================================================================
// COMPLEX GENERIC COMPONENT TESTS
// ============================================================================

#[test]
fn test_generic_component_simple() {
    transform_snapshot_with_filename(
        "generic_simple",
        "Select.svelte",
        r#"<script lang="ts" generics="T extends { id: string; label: string }">
    let { options, selected = $bindable() } = $props<{
        options: T[];
        selected?: T;
    }>();
</script>

{#each options as option}
    <button onclick={() => selected = option}>
        {option.label}
    </button>
{/each}"#,
    );
}

#[test]
fn test_generic_placeholder_rune_mapping() {
    transform_snapshot_with_filename(
        "generic_placeholder_rune",
        "Placeholder.svelte",
        r#"<script lang="ts" generics="T">
    type T = any;
    let count = $state(0);
</script>

<p>{count}</p>"#,
    );
}

#[test]
fn test_generic_component_multiple_params() {
    transform_snapshot_with_filename(
        "generic_multiple",
        "Combobox.svelte",
        r#"<script
  lang="ts"
  generics="T extends {label: string; value: string}, TMode extends 'single' | 'multiple'"
>
    import CheckIcon from '@lucide/svelte/icons/check';
    import type { Snippet } from 'svelte';

    type ValueType<TMode extends 'single' | 'multiple'> = TMode extends 'single' ? string : string[];

    type Props = {
        mode: TMode;
        value?: ValueType<TMode>;
        options: T[];
        item?: Snippet<[T]>;
    };

    let {
        mode,
        value = $bindable((mode === 'multiple' ? [] : '') as ValueType<TMode>),
        options,
        item = itemDefault,
    }: Props = $props();

    const selectedOptions = $derived.by(() => {
        if (mode === 'multiple') {
            return options.filter((opt) => (value as string[]).includes(opt.value));
        } else {
            const found = options.find((f) => f.value === value);
            return found ? [found] : [];
        }
    });
</script>

<div>
    {#each selectedOptions as option}
        <span>{@render item(option)}</span>
        <CheckIcon class="size-4" />
    {/each}
</div>

{#snippet itemDefault(option: T)}
    {option.label}
{/snippet}"#,
    );
}

#[test]
fn test_nested_components_deep() {
    transform_snapshot(
        "nested_components_deep",
        r#"<script lang="ts">
    import Outer from './Outer.svelte';
    import Middle from './Middle.svelte';
    import Inner from './Inner.svelte';

    let data = $state({ value: 42 });
</script>

<Outer prop={data}>
    <Middle value={data.value}>
        <Inner>
            <span>{data.value}</span>
        </Inner>
    </Middle>
</Outer>"#,
    );
}

#[test]
fn test_component_with_snippets_as_props() {
    transform_snapshot(
        "component_snippets_props",
        r#"<script lang="ts">
    import DataTable from './DataTable.svelte';

    type Row = { id: number; name: string; email: string };
    let rows: Row[] = $state([]);
</script>

<DataTable {rows}>
    {#snippet header()}
        <tr>
            <th>ID</th>
            <th>Name</th>
            <th>Email</th>
        </tr>
    {/snippet}

    {#snippet row(item: Row)}
        <tr>
            <td>{item.id}</td>
            <td>{item.name}</td>
            <td>{item.email}</td>
        </tr>
    {/snippet}

    {#snippet empty()}
        <tr><td colspan="3">No data</td></tr>
    {/snippet}
</DataTable>"#,
    );
}

#[test]
fn test_component_namespace_pattern() {
    transform_snapshot(
        "component_namespace",
        r#"<script lang="ts">
    import * as Dialog from '$lib/components/ui/dialog';
    import * as Popover from '$lib/components/ui/popover';

    let open = $state(false);
</script>

<Dialog.Root bind:open>
    <Dialog.Trigger>Open Dialog</Dialog.Trigger>
    <Dialog.Content>
        <Dialog.Header>
            <Dialog.Title>Title</Dialog.Title>
            <Dialog.Description>Description</Dialog.Description>
        </Dialog.Header>
        <Popover.Root>
            <Popover.Trigger>Nested Popover</Popover.Trigger>
            <Popover.Content>
                <p>Popover content</p>
            </Popover.Content>
        </Popover.Root>
    </Dialog.Content>
</Dialog.Root>"#,
    );
}

#[test]
fn test_complex_event_handlers() {
    transform_snapshot(
        "complex_events",
        r#"<script lang="ts">
    let items = $state<string[]>([]);

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            event.stopPropagation();
        }
    }
</script>

<div
    role="button"
    tabindex="0"
    onclick={(event) => {
        event.stopPropagation();
        items = [...items, 'new'];
    }}
    onkeydown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            event.stopPropagation();
            items = [...items, 'new'];
        } else {
            event.stopPropagation();
        }
    }}
>
    Click or press Enter
</div>"#,
    );
}

#[test]
fn test_spread_and_rest_props() {
    transform_snapshot(
        "spread_rest_props",
        r#"<script lang="ts">
    import Button from './Button.svelte';

    let { class: className, variant = 'default', ...rest } = $props<{
        class?: string;
        variant?: 'default' | 'outline' | 'ghost';
    } & Record<string, unknown>>();
</script>

<Button class={className} {variant} {...rest}>
    <slot />
</Button>"#,
    );
}

#[test]
fn test_conditional_component_rendering() {
    transform_snapshot(
        "conditional_components",
        r#"<script lang="ts">
    import LoadingSpinner from './LoadingSpinner.svelte';
    import ErrorMessage from './ErrorMessage.svelte';
    import DataDisplay from './DataDisplay.svelte';

    type State =
        | { status: 'loading' }
        | { status: 'error'; error: Error }
        | { status: 'success'; data: string[] };

    let state: State = $state({ status: 'loading' });
</script>

{#if state.status === 'loading'}
    <LoadingSpinner />
{:else if state.status === 'error'}
    <ErrorMessage message={state.error.message} />
{:else}
    <DataDisplay items={state.data} />
{/if}"#,
    );
}

#[test]
fn test_each_with_component_and_key() {
    transform_snapshot(
        "each_component_key",
        r#"<script lang="ts">
    import ListItem from './ListItem.svelte';

    type Item = { id: string; title: string; completed: boolean };
    let items: Item[] = $state([]);

    function toggle(id: string) {
        items = items.map(item =>
            item.id === id ? { ...item, completed: !item.completed } : item
        );
    }
</script>

<ul>
    {#each items as item, index (item.id)}
        <ListItem
            {item}
            {index}
            onToggle={() => toggle(item.id)}
        />
    {/each}
</ul>"#,
    );
}

#[test]
fn test_await_with_components() {
    transform_snapshot(
        "await_components",
        r#"<script lang="ts">
    import Skeleton from './Skeleton.svelte';
    import UserCard from './UserCard.svelte';
    import ErrorAlert from './ErrorAlert.svelte';

    type User = { id: number; name: string; avatar: string };
    let userPromise: Promise<User> = $state(fetch('/api/user').then(r => r.json()));
</script>

{#await userPromise}
    <Skeleton variant="card" />
{:then user}
    <UserCard {user} />
{:catch error}
    <ErrorAlert {error} />
{/await}"#,
    );
}

// ============================================================================
// ATTACHMENTS TESTS
// ============================================================================

#[test]
fn test_attach_on_element() {
    transform_snapshot(
        "attach_element",
        r#"<script lang="ts">
    import type { Attachment } from 'svelte/attachments';

    const myAttachment: Attachment = (element) => {
        console.log(element.nodeName);
        return () => console.log('cleanup');
    };
</script>

<div {@attach myAttachment}></div>"#,
    );
}

#[test]
fn test_attach_inline() {
    transform_snapshot(
        "attach_inline",
        r#"<script lang="ts">
    let color = $state('red');
</script>

<canvas
    width={32}
    height={32}
    {@attach (canvas) => {
        const context = canvas.getContext('2d');
        context.fillStyle = color;
    }}
></canvas>"#,
    );
}

#[test]
fn test_attach_on_component() {
    transform_snapshot(
        "attach_component",
        r#"<script lang="ts">
    import tippy from 'tippy.js';
    import Button from './Button.svelte';

    let content = $state('Hello!');

    function tooltip(content: string) {
        return (element: HTMLElement) => {
            const tooltip = tippy(element, { content });
            return tooltip.destroy;
        };
    }
</script>

<Button {@attach tooltip(content)}>
    Hover me
</Button>"#,
    );
}

#[test]
fn test_attach_source_mapping() {
    // This test verifies that source mappings for @attach expressions
    // are correctly tracked for error reporting
    transform_snapshot_with_source_map(
        "attach_source_mapping",
        r#"<script lang="ts">
    const myAttach = (el: Element) => { el.id = 'test'; };
</script>

<div {@attach myAttach}></div>"#,
    );
}

// ============================================================================
// STYLE DIRECTIVE TESTS (Issue #9)
// ============================================================================

#[test]
fn test_style_directive_string_value() {
    transform_snapshot(
        "style_directive_string",
        r#"<div style:color="red" style:background-color="blue">Styled</div>"#,
    );
}

#[test]
fn test_style_directive_expression_value() {
    transform_snapshot(
        "style_directive_expression",
        r#"<script lang="ts">
    let myColor = $state('red');
    let width = $state(100);
</script>

<div style:color={myColor} style:width={`${width}px`}>Styled</div>"#,
    );
}

#[test]
fn test_style_directive_shorthand() {
    transform_snapshot(
        "style_directive_shorthand",
        r#"<script lang="ts">
    let color = $state('red');
    let opacity = $state(0.5);
</script>

<div style:color style:opacity>Styled</div>"#,
    );
}

#[test]
fn test_style_directive_css_custom_property() {
    // Issue #9: CSS custom properties (variables) starting with --
    transform_snapshot_with_source_map(
        "style_directive_css_custom_property",
        r#"<script lang="ts">
    let compensate = $state(0);
    let theme = $state('dark');
</script>

<svg style:--icon-compensate={compensate === 0 ? null : `${compensate}px`}>
    <path d=""/>
</svg>
<div style:--theme={theme} style:--spacing="8px">Content</div>"#,
    );
}

#[test]
fn test_style_directive_important_modifier() {
    transform_snapshot(
        "style_directive_important",
        r#"<script lang="ts">
    let color = $state('red');
</script>

<div style:color|important="red" style:--theme|important={color}>Important</div>"#,
    );
}

#[test]
fn test_style_directive_with_style_attribute() {
    transform_snapshot(
        "style_directive_with_attr",
        r#"<script lang="ts">
    let color = $state('red');
</script>

<div style="font-size: 16px" style:color={color} style:--spacing="8px">Mixed</div>"#,
    );
}

#[test]
fn test_style_directive_multiple_on_element() {
    transform_snapshot(
        "style_directive_multiple",
        r#"<script lang="ts">
    let darkMode = $state(false);
</script>

<div
    style:color
    style:width="12rem"
    style:background-color={darkMode ? 'black' : 'white'}
    style:--primary="blue"
    style:opacity|important={darkMode ? 0.8 : 1}
>
    Multiple styles
</div>"#,
    );
}

// ============================================================================
// USE DIRECTIVE TESTS (Issue #7)
// ============================================================================

#[test]
fn test_use_directive_basic() {
    transform_snapshot(
        "use_directive_basic",
        r#"<script lang="ts">
    function tooltip(node: HTMLElement) {
        return { destroy() {} };
    }
</script>

<div use:tooltip>Hover me</div>"#,
    );
}

#[test]
fn test_use_directive_with_parameter() {
    transform_snapshot(
        "use_directive_with_parameter",
        r#"<script lang="ts">
    function tooltip(node: HTMLElement, content: string) {
        return { destroy() {} };
    }
    let content = $state("Hello");
</script>

<div use:tooltip={content}>Hover me</div>"#,
    );
}

#[test]
fn test_use_directive_member_access() {
    // Issue #7: Use directive with member access (dot notation)
    transform_snapshot(
        "use_directive_member_access",
        r#"<script lang="ts">
    const formSelect = {
        enhance: (node: HTMLFormElement) => {
            return { destroy() {} };
        }
    };
</script>

<form method="POST" action="?/select" use:formSelect.enhance>
    <button type="submit">Submit</button>
</form>"#,
    );
}

#[test]
fn test_use_directive_member_access_with_parameter() {
    transform_snapshot(
        "use_directive_member_access_with_parameter",
        r#"<script lang="ts">
    const actions = {
        tooltip: {
            show: (node: HTMLElement, options: { content: string }) => {
                return { destroy() {} };
            }
        }
    };
    let options = $state({ content: "Tooltip text" });
</script>

<div use:actions.tooltip.show={options}>Hover me</div>"#,
    );
}

#[test]
fn test_use_directive_deep_member_access() {
    transform_snapshot(
        "use_directive_deep_member_access",
        r#"<script lang="ts">
    const lib = {
        ui: {
            actions: {
                draggable: (node: HTMLElement) => {
                    return { destroy() {} };
                }
            }
        }
    };
</script>

<div use:lib.ui.actions.draggable>Drag me</div>"#,
    );
}

#[test]
fn test_use_directive_multiple_with_member_access() {
    transform_snapshot(
        "use_directive_multiple_with_member_access",
        r#"<script lang="ts">
    function tooltip(node: HTMLElement) {
        return { destroy() {} };
    }
    const draggable = {
        handle: (node: HTMLElement) => {
            return { destroy() {} };
        }
    };
    let opts = $state({});
</script>

<div use:tooltip use:draggable.handle use:tooltip={opts}>Interactive</div>"#,
    );
}

#[test]
fn test_use_directive_source_mapping() {
    // Verify source mappings for use directive with member access
    transform_snapshot_with_source_map(
        "use_directive_source_mapping",
        r#"<script lang="ts">
    const form = {
        enhance: (node: HTMLFormElement) => {
            return { destroy() {} };
        }
    };
</script>

<form use:form.enhance>
    <button>Submit</button>
</form>"#,
    );
}

// === Issue #40: XML Namespace Attributes ===

#[test]
fn test_xmlns_xlink_transform() {
    // Issue #40: XML namespace attributes should be passed through correctly
    transform_snapshot(
        "xmlns_xlink_transform",
        r#"<script lang="ts">
    let className = '';
</script>

<svg class={className} xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
    <rect width="100" height="100" />
</svg>"#,
    );
}

#[test]
fn test_xlink_href_transform() {
    // Issue #40: xlink:href should be transformed correctly
    transform_snapshot(
        "xlink_href_transform",
        r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink">
    <use xlink:href="#icon"/>
</svg>"##,
    );
}

// === Issue #41: Comments in Attribute Expressions ===

#[test]
fn test_comment_in_attr_transform() {
    // Issue #41: Comments in attribute expressions should be preserved
    transform_snapshot(
        "comment_in_attr_transform",
        r#"<script lang="ts">
    let items: any[] = [];
</script>

<div data={/* TODO: fix typing */ items as any}/>"#,
    );
}

// === Issue #42: bind:key Directive ===

#[test]
fn test_bind_key_transform() {
    // Issue #42: bind:key should transform correctly
    transform_snapshot(
        "bind_key_transform",
        r#"<script lang="ts">
    import Component from './Component.svelte';
    let selectedKey = $state('default');
</script>

<Component bind:key={selectedKey}/>"#,
    );
}

// === Issue #44: @const with Arrow Functions ===

#[test]
fn test_const_arrow_function_transform() {
    // Issue #44: @const with arrow function should transform correctly
    transform_snapshot(
        "const_arrow_function_transform",
        r#"<script lang="ts">
    let condition = true;
    let value: string | null = 'test';
</script>

{#if condition}
    {@const is_valid = (val: string | null): boolean => !val || val === 'none'}
    <span>Valid: {is_valid(value)}</span>
{/if}"#,
    );
}

#[test]
fn test_const_iife_transform() {
    // Issue #44: @const with IIFE should transform correctly
    transform_snapshot(
        "const_iife_transform",
        r#"<script lang="ts">
    let condition = true;
    let some_check = false;
    let valueA = 'A';
    let valueB = 'B';
</script>

{#if condition}
    {@const computed_value = (() => {
        if (some_check) return valueA
        return valueB
    })()}
    <span>{computed_value}</span>
{/if}"#,
    );
}

#[test]
fn test_snippet_with_const_transform() {
    // Issue #44: @const inside snippet should transform correctly
    transform_snapshot(
        "snippet_with_const_transform",
        r#"<script lang="ts">
    let key = $state('test');

    function check(val: string): boolean {
        return val.length > 0;
    }
</script>

{#snippet example()}
    {@const result = check(key) ? 'valid' : 'invalid'}
    <span>{result}</span>
{/snippet}

{@render example()}"#,
    );
}

#[test]
fn test_each_complex_destructure_transform() {
    // Issue #44: Complex {#each} with destructuring should transform correctly
    transform_snapshot(
        "each_complex_destructure_transform",
        r#"<script lang="ts">
    interface Item {
        title: string;
        value: number;
        unit: string;
        fmt?: (v: number) => string;
    }

    let data: Item[] = [];

    function default_fmt(v: number): string {
        return v.toString();
    }
</script>

{#each data.filter((itm) => itm.value !== undefined) as { title, value, unit, fmt = default_fmt } (title + value + unit)}
    <div>{title}: {fmt(value)}{unit}</div>
{/each}"#,
    );
}
