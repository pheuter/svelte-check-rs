<!-- Parser error: DuplicateAttribute - Same attribute specified multiple times -->
<script>
	let cls = $state('active');
	let style = $state('color: red');
</script>

<!-- Simple duplicate -->
<div class="foo" class="bar">Duplicate class</div>
<div id="one" id="two">Duplicate id</div>
<div style="color: red" style="color: blue">Duplicate style</div>

<!-- Duplicate with different value types -->
<input type="text" type={dynamicType} />
<div class="static" class={cls}>Mixed static and dynamic</div>

<!-- Duplicate data attributes -->
<div data-id="1" data-id="2">Duplicate data attribute</div>
<div data-testid="foo" data-testid="bar">Duplicate testid</div>

<!-- Duplicate ARIA attributes -->
<div aria-label="First" aria-label="Second">Duplicate aria-label</div>
<div aria-hidden="true" aria-hidden="false">Duplicate aria-hidden</div>

<!-- Duplicate event handlers -->
<button onclick={handler1} onclick={handler2}>Duplicate onclick</button>
<button on:click={handler1} on:click={handler2}>Duplicate on:click</button>

<!-- Duplicate bind directives -->
<input bind:value={a} bind:value={b} />

<!-- Duplicate class directives with same name -->
<div class:active={true} class:active={false}>Duplicate class directive</div>

<!-- Duplicate style directives -->
<div style:color="red" style:color="blue">Duplicate style directive</div>

<!-- Many duplicates -->
<div
	class="a"
	class="b"
	class="c"
	id="x"
	id="y"
	title="1"
	title="2"
>Many duplicates</div>

<!-- Shorthand and explicit duplicates -->
<div {cls} class={cls}>Shorthand and explicit</div>
<div {style} style={style}>Style shorthand and explicit</div>
