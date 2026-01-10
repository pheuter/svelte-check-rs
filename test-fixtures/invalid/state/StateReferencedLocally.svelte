<script lang="ts">
	// Test cases for state_referenced_locally warning

	// Case 1: Destructuring from $props()
	let { data } = $props();
	const form = data.form; // Should warn: captures initial value

	// Case 2: Using $state() initial value
	let count = $state(0);
	const initialCount = count; // Should warn: captures initial value

	// Case 3: Destructuring $props into const
	const { items } = data; // Should warn: captures initial value

	// Case 4: $derived - should NOT warn (inside reactive context)
	const doubled = $derived(count * 2);

	// Case 5: Function/closure - should NOT warn
	function getCount() {
		return count; // OK - inside closure
	}

	// Case 6: Arrow function - should NOT warn
	const getValue = () => data.value; // OK - inside closure

	// Case 7: $effect - should NOT warn
	$effect(() => {
		console.log(count); // OK - inside $effect
	});
</script>

<p>Form: {form}</p>
<p>Count: {count}</p>
