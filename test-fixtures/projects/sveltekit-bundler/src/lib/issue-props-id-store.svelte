<script lang="ts">
	import { writable } from 'svelte/store';

	const state = writable(() => writable({ id: () => 42 }));
	const props = $state();
	const idFromStore: number = $props.id();
</script>

<div data-store-id={idFromStore}></div>
