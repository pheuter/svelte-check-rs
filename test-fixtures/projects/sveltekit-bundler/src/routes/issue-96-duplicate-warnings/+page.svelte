<script lang="ts">
	interface Props {
		initialCount?: number;
	}

	let { initialCount = 0 }: Props = $props();

	let count = $state(initialCount);
</script>

<p>Count: {count}</p>
