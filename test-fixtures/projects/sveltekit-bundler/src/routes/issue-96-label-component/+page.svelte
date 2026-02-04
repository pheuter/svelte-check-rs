<script lang="ts">
	import Issue96Checkbox from '$lib/issue-96-checkbox.svelte';

	let checked = $state(false);
</script>

<label>
	<Issue96Checkbox bind:checked={checked} />
	<span>Option</span>
</label>
