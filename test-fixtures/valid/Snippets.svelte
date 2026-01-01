<script>
	let items = $state(['a', 'b', 'c']);
</script>

{#snippet renderItem(item, index)}
	<li>{index + 1}. {item}</li>
{/snippet}

<ul>
	{#each items as item, i}
		{@render renderItem(item, i)}
	{/each}
</ul>
