<script lang="ts">
	// Valid patterns that should NOT produce state_referenced_locally warnings

	// Pattern 1: Using $derived for computed values
	let { data } = $props();
	const form = $derived(data.form);
	const items = $derived(data.items ?? []);

	// Pattern 2: State accessed inside $effect
	let count = $state(0);
	$effect(() => {
		console.log('Count changed:', count);
	});

	// Pattern 3: State accessed inside functions
	function increment() {
		count += 1;
	}

	function getCount() {
		return count;
	}

	// Pattern 4: State passed to context via getter
	import { setContext } from 'svelte';
	setContext('count', () => count);

	// Pattern 5: Assigning state (not reading)
	let value = $state(0);
	value = 42; // Assignment, not read

	// Pattern 6: Props used directly in template (not captured)
	// (template references are fine)
</script>

<p>Form: {data.form}</p>
<p>Count: {count}</p>
<button onclick={increment}>Increment</button>
