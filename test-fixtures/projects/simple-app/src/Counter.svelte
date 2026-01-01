<script lang="ts">
	interface Props {
		initial?: number;
	}

	let { initial = 0 }: Props = $props();
	let count = $state(initial);
</script>

<button onclick={() => count++}>
	Count: {count}
</button>

<style>
	button {
		padding: 0.5rem 1rem;
		border-radius: 4px;
	}
</style>
