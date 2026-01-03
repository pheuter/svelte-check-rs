<!-- Parser edge cases: Unusual and boundary syntax scenarios -->
<script>
	let x = $state(0);
</script>

<!-- Empty blocks and elements -->
{#if}{/if}
{#each}{/each}
{#await}{/await}
{#key}{/key}
<></>
<  ><  / >

<!-- Whitespace variations in tags -->
<div    class = "test"   >content</  div  >
<span
	class
	=
	"multiline"
>
</
span
>

<!-- Unicode in unexpected places -->
<div class="émoji">🎉</div>
<button onclick={() => console.log('日本語')}>クリック</button>

<!-- Very long attribute values (edge case for buffers) -->
<div data-long="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

<!-- Expression with very deep nesting -->
<p>{((((((((((x))))))))))}</p>

<!-- Multiple consecutive expressions -->
<p>{x}{x}{x}{x}{x}{x}{x}{x}{x}{x}</p>

<!-- Expression with all bracket types -->
<p>{obj[arr[fn({key: [1,2,3]})]]}</p>

<!-- Attribute edge cases -->
<div data-=""  data-2 = "" ></div>
<input type = 'text' value = "" />

<!-- Self-closing with attributes -->
<img src="a.jpg" alt="test" / >
<br / >
<input type="text" / >

<!-- Mixing quote styles -->
<div class="double" id='single' data-x=unquoted></div>

<!-- Boolean attributes variations -->
<input disabled />
<input disabled="" />
<input disabled="disabled" />
<input disabled={true} />

<!-- Numeric-looking attribute names (invalid) -->
<div 0="zero" 1st="first" 2nd="second"></div>

<!-- Special character sequences -->
<p>&lt;&gt;&amp;&quot;&apos;</p>
<p>&#60;&#62;&#38;</p>
<p>&#x3C;&#x3E;&#x26;</p>

<!-- HTML comments edge cases -->
<!-- -->
<!---->
<!----->
<!-- -- -->
<!-- - - - -->

<!-- Script and style tag edge cases -->
<script></script>
<style></style>
<script lang="ts"></script>
<style lang="scss"></style>

<!-- CDATA-like (not valid but shouldn't crash) -->
<![CDATA[Some content]]>

<!-- Processing instructions (not valid but shouldn't crash) -->
<?xml version="1.0"?>

<!-- DOCTYPE (not valid in Svelte but shouldn't crash) -->
<!DOCTYPE html>

<!-- Unusual but valid-ish constructs -->
<div {...$$props}></div>
<div {...$$restProps}></div>

<!-- Slot with all attribute types -->
<slot name="named" {x} data-foo="bar" />

<!-- Svelte special elements with unusual attributes -->
<svelte:head><title>Test</title></svelte:head>
<svelte:body onscroll={() => {}} />
<svelte:window onresize={() => {}} />
<svelte:document onvisibilitychange={() => {}} />

<!-- Self-referential component -->
<svelte:self />
<svelte:self prop={x} />

<!-- Dynamic component edge cases -->
<svelte:component this={null} />
<svelte:component this={undefined} />

<!-- Element directive with expression -->
<svelte:element this={x > 0 ? 'div' : 'span'}>
	Content
</svelte:element>

<!-- Fragment variations -->
<svelte:fragment slot="named">Content</svelte:fragment>

<!-- Options with all variations -->
<svelte:options immutable={true} />
<svelte:options accessors={true} />
<svelte:options namespace="svg" />

<!-- Multiple consecutive special tags -->
{@html '<div>'}
{@html x}
{@const y = x + 1}
{@const z = y * 2}
{@debug x, y, z}

<!-- Render tag edge cases -->
{@render snippet?.()}
{@render children?.()}
