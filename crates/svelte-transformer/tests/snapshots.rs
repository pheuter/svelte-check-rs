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

fn transform_snapshot_with_filename(name: &str, filename: &str, source: &str) {
    let parsed = parse(source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some(filename.to_string()),
            source_maps: true,
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
