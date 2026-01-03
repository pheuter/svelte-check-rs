<!-- Parser edge cases: Component and Svelte-specific syntax errors -->
<script>
	let Component = $state(null);
	let props = $state({});
	let show = $state(true);
</script>

<!-- Component with various attribute issues -->
<Component
	prop=
	another="value"
/>

<Component
	prop={
	other="incomplete"
/>

<Component
	{...
/>

<Component
	...props}
/>

<!-- Dynamic component issues -->
<svelte:component
	this=
/>

<svelte:component
	this={Component
/>

<svelte:component this={show ? Component :}>
	Content
</svelte:component>

<!-- Svelte:element issues -->
<svelte:element this=>
	Content
</svelte:element>

<svelte:element this={>
	Content
</svelte:element>

<svelte:element this="div" {>
	Content with unclosed expression
</svelte:element>

<!-- Slot issues -->
<slot name= />
<slot name={} />
<slot {name />
<slot name="test" let: />
<slot let:prop= />

<!-- Event forwarding issues -->
<button on:click /><!-- Valid but edge case -->
<button on: />
<button on:click| />
<button on:|preventDefault />

<!-- Bind issues on components -->
<Component bind: />
<Component bind:value /><!-- Missing equals -->
<Component bind:value= />
<Component bind:value={} />
<Component bind:this= />

<!-- Transition on component (invalid) -->
<Component transition:fade />

<!-- Multiple same-type blocks in component -->
<Component>
	<slot slot="a">First</slot>
	<slot slot="a">Duplicate slot name</slot>
</Component>

<!-- Snippet definition issues -->
{#snippet (params)}
	Missing name
{/snippet}

{#snippet name(}
	Unclosed params
{/snippet}

{#snippet name(a, b,)}
	Trailing comma
{/snippet}

{#snippet name(a b)}
	Missing comma
{/snippet}

<!-- Render issues -->
{@render }
{@render ()}
{@render snippet(}
{@render snippet(a,)}
{@render snippet(a b)}
{@render 123()}

<!-- Const tag issues -->
{@const }
{@const =}
{@const x}
{@const x =}
{@const = 5}

<!-- Debug tag issues -->
{@debug }
{@debug ,}
{@debug x,}
{@debug ,x}

<!-- Html tag issues -->
{@html }
{@html }
{@html <>}

<!-- Attach syntax issues (Svelte 5) -->
{@attach }
{@attach action}
{@attach action(}
{@attach action()}

<!-- Props destructuring issues -->
<script>
	let { = $props();
	let { a, = $props();
	let { ...} = $props();
</script>

<!-- Generics issues -->
<script generics="T">
</script>

<script generics="T extends">
</script>

<script generics="T, U,">
</script>

<!-- Module script issues -->
<script context="module">
	export let invalid;<!-- Invalid in module context -->
</script>

<!-- Style issues -->
<style lang=>
	.class { }
</style>

<style>
	:global( {
		color: red;
	}
</style>

<style>
	.unclosed {
		color: blue
</style>
