<script lang="ts">
	import type { ActionReturn } from 'svelte/action';

	const handleDemo = (event: CustomEvent<{ value: number }>) => {
		console.log(event.detail.value);
	};

	function demoAction(
		_node: HTMLElement
	): ActionReturn<
		undefined,
		{ 'on:demo_action'?: (event: CustomEvent<{ value: number }>) => void }
	> {
		return {};
	}
</script>

<div on:demo_change={handleDemo}></div>
<div on:demo_change={(event) => event.detail.value}></div>
<div use:demoAction on:demo_action={(event) => event.detail.value}></div>
