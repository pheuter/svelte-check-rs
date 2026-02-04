<script lang="ts">
	const items = [
		{ id: '1', name: 'Item A' },
		{ id: '2', name: 'Item B' }
	];

	let selected = $state<string | null>(null);

	function selectItem(id: string) {
		selected = id;
	}
</script>

<table role="grid">
	<tbody>
		{#each items as item}
			<tr class:selected={selected === item.id}>
				<td onclick={() => selectItem(item.id)}>{item.name}</td>
			</tr>
		{/each}
	</tbody>
</table>
