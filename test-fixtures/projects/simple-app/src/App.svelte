<script lang="ts">
	import Counter from './Counter.svelte';

	let title = $state('My App');
</script>

<main>
	<h1>{title}</h1>
	<Counter initial={10} />
</main>

<style>
	main {
		padding: 1rem;
	}

	h1 {
		color: #333;
	}
</style>
