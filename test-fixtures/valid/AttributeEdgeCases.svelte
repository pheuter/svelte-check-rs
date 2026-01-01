<script>
	let className = $state('active');
	let disabled = $state(false);
	let props = $state({ a: 1, b: 2 });
	let id = $state('main');
</script>

<!-- Spread attributes -->
<div {...props}>Spread</div>
<Component {...props} extra="value" />

<!-- Shorthand attributes -->
<div {id}>Shorthand</div>
<input {disabled} />

<!-- Boolean attributes -->
<input disabled />
<button disabled={true}>Disabled</button>
<input readonly required />

<!-- Concatenated values -->
<div class="static-{className}-end">Concat</div>
<div data-id="prefix-{id}-suffix">Data attr</div>
<a href="/page/{id}">Link</a>

<!-- Multiple expressions in attribute -->
<div class="{className} {disabled ? 'disabled' : ''} other">Multi</div>

<!-- Expression-only quoted attribute -->
<div class="{className}">Expression only</div>

<!-- Single quotes -->
<div class='single-{className}'>Single quotes</div>

<!-- Unquoted expression attribute -->
<div class={className}>Unquoted</div>

<!-- Complex expression in attribute -->
<div data-json="{JSON.stringify({ key: 'value' })}">JSON</div>

<!-- Empty attribute value -->
<div data-empty="">Empty</div>

<!-- Numeric attributes -->
<div tabindex="0" data-count={42}>Numbers</div>
