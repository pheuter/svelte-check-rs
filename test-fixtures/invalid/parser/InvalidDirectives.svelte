<!-- Parser error: InvalidDirective - Malformed Svelte directive syntax -->
<script>
	let value = $state('');
	function handler() {}
</script>

<!-- Empty directive name -->
<button on:>Click</button>
<input bind: />
<div class:></div>
<div style:></div>
<div use:></div>

<!-- Directive with invalid modifier syntax -->
<button on:click|>Click</button>
<button on:click|preventDefault|>Click</button>

<!-- Bind directive issues -->
<input bind:value= />
<input bind:=value />

<!-- Class directive with invalid syntax -->
<div class:={true}></div>
<div class:active=></div>

<!-- Style directive issues -->
<div style:color|invalid="red"></div>
<div style:="red"></div>

<!-- Transition directive issues -->
<div transition:></div>
<div transition:fade|>Content</div>
<div in:></div>
<div out:></div>

<!-- Use directive issues -->
<div use:action|modifier>Content</div>

<!-- Animate directive issues -->
<div animate:>Content</div>
<div animate:flip|invalid>Content</div>

<!-- Let directive issues -->
<slot let:></slot>
<slot let:prop=></slot>

<!-- Multiple colons -->
<button on::click={handler}>Click</button>
<input bind::value={value} />

<!-- Invalid directive kind -->
<div invalid:directive="value"></div>
