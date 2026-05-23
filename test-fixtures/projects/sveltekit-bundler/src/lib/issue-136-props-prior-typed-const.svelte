<script lang="ts">
	const items: string[] = ['a', 'b'];
	let { label = 'default' } = $props();
</script>

<p>{label} ({items.length})</p>
