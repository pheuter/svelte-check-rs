<script>
	let items = $state([1, 2, 3]);
	let user = $state({ profile: { name: 'Alice' } });
	let count = $state(0);
	let data = $state(null);
</script>

<!-- Ternary expressions -->
{count > 0 ? 'positive' : count < 0 ? 'negative' : 'zero'}

<!-- Optional chaining -->
{user?.profile?.name ?? 'Unknown'}
{data?.items?.[0]?.value}

<!-- Template literals -->
{`Count is ${count}`}
{`User: ${user?.profile?.name ?? 'Guest'}`}

<!-- Arrow functions in expressions -->
{#each items.map(x => x * 2) as doubled}
	<span>{doubled}</span>
{/each}

<!-- Complex arrow with block body -->
{#each items.filter(x => {
	return x > 1;
}) as filtered}
	<span>{filtered}</span>
{/each}

<!-- Nested function calls -->
{items.map(x => x * 2).filter(x => x > 2).join(', ')}

<!-- Object literals in expressions -->
{@const config = { min: 0, max: 100 }}
<input type="range" min={config.min} max={config.max} />

<!-- Destructuring in expressions -->
{#each Object.entries(user).map(([key, value]) => ({ key, value })) as entry}
	<p>{entry.key}: {entry.value}</p>
{/each}

<!-- Nullish coalescing -->
{data ?? 'default'}
{user.profile?.bio ?? 'No bio provided'}

<!-- Spread in expressions -->
<Component {...{ a: 1, ...props, b: 2 }} />

<!-- Method calls with nested objects -->
{JSON.stringify({ nested: { data: items } }, null, 2)}
