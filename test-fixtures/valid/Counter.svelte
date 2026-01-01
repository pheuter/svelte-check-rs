<script lang="ts">
	interface Props {
		initial?: number;
		onchange?: (value: number) => void;
	}

	let { initial = 0, onchange }: Props = $props();
	let count = $state(initial);
	let doubled = $derived(count * 2);

	function increment() {
		count++;
		onchange?.(count);
	}
</script>

<button onclick={increment}>
	Count: {count} (doubled: {doubled})
</button>
