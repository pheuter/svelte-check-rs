<!-- Parser error: InvalidExpression - Malformed template expressions -->
<script>
	let count = $state(0);
	let items = $state([]);
</script>

<!-- Empty expression -->
<p>{}</p>

<!-- Unclosed expression -->
<p>{count</p>

<!-- Expression with unclosed parentheses -->
<p>{(count + 1}</p>

<!-- Expression with unclosed brackets -->
<p>{items[0}</p>

<!-- Expression with unclosed braces -->
<p>{{ foo: 'bar' }</p>

<!-- Expression with unclosed string -->
<p>{"hello}</p>
<p>{'world}</p>
<p>{`template}</p>

<!-- Expression with invalid syntax -->
<p>{count +}</p>
<p>{+ count}</p>
<p>{count + +}</p>

<!-- Expression with trailing comma -->
<p>{count,}</p>

<!-- Double braces (not valid in Svelte) -->
<p>{{count}}</p>

<!-- Expression in attribute with issues -->
<div class={>Invalid</div>
<div style={incomplete></div>

<!-- Arrow function with issues -->
<button onclick={() =>}>Click</button>
<button onclick={(e) => {>Click</button>

<!-- Ternary with missing parts -->
<p>{show ?}</p>
<p>{show ? 'yes' :}</p>
<p>{? 'yes' : 'no'}</p>
