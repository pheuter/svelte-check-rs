<script>
	let items = $state([1, 2, 3]);
	let show = $state(true);
	let promise = $state(Promise.resolve('data'));
</script>

{#if show}
	<p>Visible</p>
{:else}
	<p>Hidden</p>
{/if}

{#each items as item, index (item)}
	<p>{index}: {item}</p>
{:else}
	<p>No items</p>
{/each}

{#await promise}
	<p>Loading...</p>
{:then data}
	<p>Data: {data}</p>
{:catch error}
	<p>Error: {error}</p>
{/await}

{#key items}
	<p>Items changed</p>
{/key}
