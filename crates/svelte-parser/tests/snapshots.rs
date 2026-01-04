use svelte_parser::parse;

fn parse_snapshot(name: &str, source: &str) {
    let result = parse(source);
    let output = format!(
        "Source:\n{}\n\nErrors: {:?}\n\nAST:\n{:#?}",
        source, result.errors, result.document
    );
    insta::assert_snapshot!(name, output);
}

#[test]
fn test_snapshot_simple_element() {
    parse_snapshot("simple_element", "<div>Hello</div>");
}

#[test]
fn test_snapshot_self_closing() {
    parse_snapshot("self_closing", "<br/><input/>");
}

#[test]
fn test_snapshot_attributes() {
    parse_snapshot(
        "attributes",
        r#"<div class="container" id="main" disabled>Content</div>"#,
    );
}

#[test]
fn test_snapshot_expression() {
    parse_snapshot("expression", "<p>{message}</p>");
}

#[test]
fn test_snapshot_if_block() {
    parse_snapshot(
        "if_block",
        r#"{#if show}
    <p>Visible</p>
{:else}
    <p>Hidden</p>
{/if}"#,
    );
}

#[test]
fn test_snapshot_if_else_if() {
    parse_snapshot(
        "if_else_if",
        r#"{#if status === 'loading'}
    <p>Loading...</p>
{:else if status === 'error'}
    <p>Error</p>
{:else}
    <p>Ready</p>
{/if}"#,
    );
}

#[test]
fn test_snapshot_each_block() {
    parse_snapshot(
        "each_block",
        r#"{#each items as item, index (item.id)}
    <p>{index}: {item.name}</p>
{:else}
    <p>No items</p>
{/each}"#,
    );
}

#[test]
fn test_snapshot_await_block() {
    parse_snapshot(
        "await_block",
        r#"{#await promise}
    <p>Loading...</p>
{:then data}
    <p>{data}</p>
{:catch error}
    <p>Error: {error}</p>
{/await}"#,
    );
}

#[test]
fn test_snapshot_key_block() {
    parse_snapshot(
        "key_block",
        r#"{#key id}
    <Component {id}/>
{/key}"#,
    );
}

#[test]
fn test_snapshot_snippet_and_render() {
    parse_snapshot(
        "snippet_render",
        r#"{#snippet button(text)}
    <button>{text}</button>
{/snippet}

{@render button('Click me')}"#,
    );
}

#[test]
fn test_snapshot_special_tags() {
    parse_snapshot(
        "special_tags",
        r#"{@html content}{@const x = 1}{@debug foo}"#,
    );
}

#[test]
fn test_snapshot_comment() {
    parse_snapshot("comment", "<!-- This is a comment --><div>Content</div>");
}

#[test]
fn test_snapshot_script() {
    parse_snapshot(
        "script",
        r#"<script>
    let count = $state(0);
</script>

<button on:click={() => count++}>{count}</button>"#,
    );
}

#[test]
fn test_snapshot_directive_modifiers() {
    parse_snapshot(
        "directive_modifiers",
        r#"<button on:click|preventDefault|stopPropagation={handler}>Click</button>"#,
    );
}

#[test]
fn test_snapshot_concatenated_attribute() {
    parse_snapshot(
        "concatenated_attribute",
        r#"<div class="prefix-{middle}-suffix" data-id="id-{id}">Content</div>"#,
    );
}

#[test]
fn test_snapshot_complex_expression() {
    parse_snapshot(
        "complex_expression",
        r#"{#each items.map(x => ({ ...x, doubled: x.value * 2 })) as item}
    <p>{item.name}: {item.doubled}</p>
{/each}"#,
    );
}

#[test]
fn test_snapshot_nested_blocks() {
    parse_snapshot(
        "nested_blocks",
        r#"{#if outer}
    {#each items as item}
        {#if item.visible}
            <p>{item.name}</p>
        {/if}
    {/each}
{/if}"#,
    );
}

#[test]
fn test_snapshot_component() {
    parse_snapshot(
        "component",
        r#"<MyComponent prop={value} on:click={handler}>
    <span slot="header">Header</span>
    Content
</MyComponent>"#,
    );
}

#[test]
fn test_snapshot_attach() {
    parse_snapshot(
        "attach",
        r#"<div {@attach myAttachment}></div>
<div {@attach (node) => { console.log(node); }}></div>
<Component {@attach tooltip(content)} />"#,
    );
}

#[test]
fn test_snapshot_attach_with_other_attrs() {
    parse_snapshot(
        "attach_with_attrs",
        r#"<div class="container" {@attach myAttachment} id="main">Content</div>"#,
    );
}

#[test]
fn test_snapshot_attach_multiple() {
    parse_snapshot(
        "attach_multiple",
        r#"<div {@attach first} {@attach second} {@attach third}>Multiple</div>"#,
    );
}

#[test]
fn test_snapshot_attach_factory() {
    parse_snapshot(
        "attach_factory",
        r#"<button {@attach tooltip(content, { placement: 'top' })}>Hover</button>"#,
    );
}

#[test]
fn test_snapshot_attach_self_closing() {
    parse_snapshot(
        "attach_self_closing",
        r#"<input type="text" {@attach myAttachment} />"#,
    );
}

// === Style Directive Snapshots ===

#[test]
fn test_snapshot_style_directive_basic() {
    parse_snapshot(
        "style_directive_basic",
        r#"<div style:color="red" style:width={width} style:opacity>Styled</div>"#,
    );
}

#[test]
fn test_snapshot_style_directive_css_custom_property() {
    // Issue #9: CSS custom properties starting with --
    parse_snapshot(
        "style_directive_css_custom_property",
        r#"<svg style:--icon-compensate={compensate === 0 ? null : `${compensate}px`}><path d=""/></svg>"#,
    );
}

#[test]
fn test_snapshot_style_directive_important() {
    parse_snapshot(
        "style_directive_important",
        r#"<div style:color|important="red" style:--theme|important={theme}>Important</div>"#,
    );
}

#[test]
fn test_snapshot_style_directive_with_style_attr() {
    parse_snapshot(
        "style_directive_with_style_attr",
        r#"<div style="font-size: 16px" style:color="red" style:--spacing={spacing}>Mixed</div>"#,
    );
}
