<script lang="ts">
	// Legacy event component: `on:select` is a typed CustomEvent so a
	// wrong-typed `on:select` handler on the <EventComponent> tag errors.
	import { createEventDispatcher } from 'svelte';

	const dispatch = createEventDispatcher<{ select: { id: number } }>();

	function fire() {
		dispatch('select', { id: 1 });
	}
</script>

<button onclick={fire}>fire</button>
