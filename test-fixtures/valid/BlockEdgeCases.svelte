<script>
	let condition = $state(true);
	let status = $state('pending');
	let items = $state([{ id: 1, name: 'a' }, { id: 2, name: 'b' }]);
	let data = $state({ users: [{ name: 'Alice', roles: ['admin'] }] });
	let promise = $state(Promise.resolve({ value: 42 }));
</script>

<!-- Chained if-else-if -->
{#if status === 'loading'}
	<p>Loading...</p>
{:else if status === 'error'}
	<p>Error occurred</p>
{:else if status === 'empty'}
	<p>No data</p>
{:else if status === 'pending'}
	<p>Pending</p>
{:else}
	<p>Ready</p>
{/if}

<!-- Each with destructuring -->
{#each items as { id, name }}
	<p>{id}: {name}</p>
{/each}

<!-- Nested destructuring -->
{#each data.users as { name, roles }}
	<p>{name}</p>
	{#each roles as role}
		<span>{role}</span>
	{/each}
{/each}

<!-- Complex key expressions -->
{#each items as item (item.id)}
	<p>{item.name}</p>
{/each}

{#each items as item, i (`${item.id}-${i}`)}
	<p>{item.name}</p>
{/each}

<!-- Deeply nested blocks -->
{#if condition}
	{#each items as item}
		{#if item.id > 0}
			{#key item.id}
				<p>{item.name}</p>
			{/key}
		{/if}
	{/each}
{/if}

<!-- Await with complex then destructuring -->
{#await promise then { value }}
	<p>Value: {value}</p>
{:catch { message }}
	<p>Error: {message}</p>
{/await}

<!-- Key with complex expression -->
{#key items.map(i => i.id).join(',')}
	<p>Items changed</p>
{/key}
