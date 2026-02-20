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

// === Use Directive Snapshots ===

#[test]
fn test_snapshot_use_directive_basic() {
    // Basic use directive with simple identifier
    parse_snapshot("use_directive_basic", r#"<div use:tooltip>Hover me</div>"#);
}

#[test]
fn test_snapshot_use_directive_with_value() {
    // Use directive with expression value
    parse_snapshot(
        "use_directive_with_value",
        r#"<div use:tooltip={content}>Hover me</div>"#,
    );
}

#[test]
fn test_snapshot_use_directive_member_access() {
    // Issue #7: Use directive with member access (dot notation)
    parse_snapshot(
        "use_directive_member_access",
        r#"<form method="POST" action="?/select" use:formSelect.enhance>Submit</form>"#,
    );
}

#[test]
fn test_snapshot_use_directive_member_access_with_value() {
    // Use directive with member access and expression value
    parse_snapshot(
        "use_directive_member_access_with_value",
        r#"<form use:enhance.submit={options}>Submit</form>"#,
    );
}

#[test]
fn test_snapshot_use_directive_deep_member_access() {
    // Use directive with deeply nested member access
    parse_snapshot(
        "use_directive_deep_member_access",
        r#"<div use:actions.tooltip.show={config}>Content</div>"#,
    );
}

#[test]
fn test_snapshot_use_directive_multiple() {
    // Multiple use directives including member access
    parse_snapshot(
        "use_directive_multiple",
        r#"<div use:tooltip use:draggable.handle use:resizable={options}>Draggable</div>"#,
    );
}

#[test]
fn test_snapshot_use_directive_with_modifiers() {
    // Use directive with member access and modifiers (if supported)
    parse_snapshot(
        "use_directive_with_modifiers",
        r#"<div use:action.method|once|capture={handler}>Content</div>"#,
    );
}

// === Issue #107: use:action(args) shorthand syntax ===

#[test]
fn test_snapshot_use_directive_paren_args() {
    // use:action(args) — parens absorbed into directive name per official Svelte parser
    parse_snapshot(
        "use_directive_paren_args",
        r#"<img use:handleMount(file) alt="test" />"#,
    );
}

#[test]
fn test_snapshot_use_directive_paren_args_nested() {
    // use:action(fn('x')) — nested call expression in parens
    parse_snapshot(
        "use_directive_paren_args_nested",
        r#"<div use:tooltip(getText('hello')) class="box"></div>"#,
    );
}

// === Issue #108: Escaped backslashes in expressions ===

#[test]
fn test_snapshot_expression_escaped_backslash() {
    // String ending with \\ — naive lookbehind incorrectly treats closing quote as escaped
    parse_snapshot(
        "expression_escaped_backslash",
        r#"<button onclick={() => open('C:\\Users\\')}>Open</button>"#,
    );
}

// === Issue #40: XML Namespace Attributes ===

#[test]
fn test_snapshot_xmlns_xlink() {
    // Issue #40: XML namespace attributes should be parsed as normal attributes
    parse_snapshot(
        "xmlns_xlink",
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
  <rect width="100" height="100" />
</svg>"#,
    );
}

#[test]
fn test_snapshot_xlink_href() {
    // Issue #40: xlink:href attribute in SVG
    parse_snapshot(
        "xlink_href",
        r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink">
  <use xlink:href="#icon"/>
</svg>"##,
    );
}

// === Issue #41: Comments in Attribute Expressions ===

#[test]
fn test_snapshot_comment_in_attr_multiline() {
    // Issue #41: Multi-line comment inside attribute expression
    parse_snapshot(
        "comment_in_attr_multiline",
        r#"<div data={/* TODO: fix typing */ items as any}/>"#,
    );
}

#[test]
fn test_snapshot_comment_in_attr_with_value() {
    // Issue #41: Comment before value in attribute expression
    parse_snapshot(
        "comment_in_attr_with_value",
        r#"<MyComponent data={/* config */ { a: 1 }}/>"#,
    );
}

// === Issue #42: bind:key Directive ===

#[test]
fn test_snapshot_bind_key_directive() {
    // Issue #42: bind:key should be a valid directive
    parse_snapshot(
        "bind_key_directive",
        r#"<Component bind:key={selectedKey}/>"#,
    );
}

#[test]
fn test_snapshot_bind_keyword_directives() {
    // Other keyword bindings that should work
    parse_snapshot(
        "bind_keyword_directives",
        r#"<input bind:value={val}/>
<Component bind:key={k} bind:html={h}/>"#,
    );
}

// === Edge Cases: Directives with Keyword Names ===

#[test]
fn test_snapshot_on_if_event() {
    // on:if should be valid (custom event with keyword name)
    parse_snapshot("on_if_event", r#"<Component on:if={handleIf}/>"#);
}

#[test]
fn test_snapshot_on_else_event() {
    // on:else should be valid (custom event with keyword name)
    parse_snapshot("on_else_event", r#"<Component on:else={handleElse}/>"#);
}

// === Issue #46: Regex Literals in Expressions ===

#[test]
fn test_snapshot_regex_simple() {
    // Simple regex literal in expression
    parse_snapshot("regex_simple", r#"{value.match(/test/)}"#);
}

#[test]
fn test_snapshot_regex_with_parens() {
    // Regex with capture groups (parentheses)
    parse_snapshot(
        "regex_with_parens",
        r#"{value.match(/^(.+?)\s*\(([^)]+)\)$/)}"#,
    );
}

#[test]
fn test_snapshot_regex_with_char_class() {
    // Regex with character class containing special chars
    parse_snapshot("regex_with_char_class", r#"{value.match(/[^)]+/)}"#);
}

#[test]
fn test_snapshot_regex_rgba_pattern() {
    // Complex RGBA pattern from issue #46
    parse_snapshot(
        "regex_rgba_pattern",
        r#"{/rgba\([^)]+[,/]\s*0(\.0*)?\s*\)$/.test(color)}"#,
    );
}

#[test]
fn test_snapshot_const_with_regex_match() {
    // Issue #46: @const with regex match and nullish coalescing
    parse_snapshot(
        "const_with_regex_match",
        r#"{#if true}{@const [, label] = value.match(/^(.+?)\s*\(([^)]+)\)$/) ?? [, value, ``]}{label}{/if}"#,
    );
}

#[test]
fn test_snapshot_const_with_regex_test() {
    // Issue #46: @const with regex.test()
    parse_snapshot(
        "const_with_regex_test",
        r#"{#if true}{@const matches = /rgba\([^)]+\)$/.test(color)}{matches}{/if}"#,
    );
}

#[test]
fn test_snapshot_multiple_const_with_regex() {
    // Issue #46: Multiple @const tags, first with regex
    parse_snapshot(
        "multiple_const_with_regex",
        r#"{#if true}{@const a = /test/.test(x)}{@const b = 2}{a}{b}{/if}"#,
    );
}

#[test]
fn test_snapshot_snippet_with_const_regex() {
    // Issue #46: Snippet containing @const with regex
    parse_snapshot(
        "snippet_with_const_regex",
        r#"{#snippet tooltip({ x })}
    {@const match = x.match(/test/)}
    <span>{match}</span>
{/snippet}"#,
    );
}

#[test]
fn test_snapshot_const_arrow_function_regex() {
    // Issue #46: @const with typed arrow function containing regex
    parse_snapshot(
        "const_arrow_function_regex",
        r#"{#if true}{@const check = (s: string): boolean => /test/.test(s)}{check("x")}{/if}"#,
    );
}

#[test]
fn test_snapshot_const_iife_regex() {
    // Issue #46: @const with IIFE containing regex
    parse_snapshot(
        "const_iife_regex",
        r#"{#if true}{@const result = (() => { return /test/.test(x); })()}{result}{/if}"#,
    );
}

#[test]
fn test_snapshot_regex_quantifier_braces() {
    // Regex with {n,m} quantifier
    parse_snapshot("regex_quantifier_braces", r#"{value.match(/\d{2,4}/)}"#);
}

#[test]
fn test_snapshot_regex_in_template_literal() {
    // Regex inside template literal ${} expression
    parse_snapshot(
        "regex_in_template_literal",
        r#"{`result: ${/test/.test(x)}`}"#,
    );
}

#[test]
fn test_snapshot_division_not_regex() {
    // Division should not be confused with regex
    parse_snapshot("division_not_regex", r#"{(a + b) / 2}"#);
}

#[test]
fn test_snapshot_complex_snippet_const_regex() {
    // Complex case from issue #46 minimal repro
    parse_snapshot(
        "complex_snippet_const_regex",
        r#"<div class="parent">
  {#snippet tooltip({ x, y_formatted }: { x: number; y_formatted: string })}
    {@const [, y_label, y_unit] = y_label_full.match(/^(.+?)\s*\(([^)]+)\)$/) ??
      [, y_label_full, ``]}
    {@const segment = Object.entries(x_positions ?? {}).find(([, [start, end]]) =>
      x >= start && x <= end
    )}
    <span>{y_label}: {y_formatted} {y_unit}</span>
  {/snippet}
</div>"#,
    );
}
