<script>
	let cond = true;
	let items = [{ x: 1 }, { x: 2 }];
	let promise = Promise.resolve(42);
	let obj = { a: 1, b: 2 };
	let arr = [1, 2, 3];
</script>

{#if cond}
	{const a = 1}
	<p>{a}</p>
{/if}

{#each items as item}
	{const x = item.x}
	{let label = x * 2}
	<p>{label}</p>
{/each}

{#await promise then value}
	{const doubled = value * 2}
	<p>{doubled}</p>
{/await}

{#snippet foo()}
	{const inside = 1}
	<p>{inside}</p>
{/snippet}

<div>{const a = 1}{a}</div>

{#each items as item}
	{const shadow = item.x}
	<div>
		{const shadow = 'nested'}
		<p>{shadow}</p>
	</div>
{/each}

{const {a, b} = obj}
<p>{a} {b}</p>

{const [first, second] = arr}
{let [head, ...tail] = arr}
<p>{first} {second} {head} {tail.length}</p>
