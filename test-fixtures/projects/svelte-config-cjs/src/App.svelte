<script lang="ts">
	// `$lib` alias is declared only in svelte.config.cjs (CommonJS module.exports).
	import { greet } from '$lib/greeting';

	let name = $state('world');
</script>

<main>
	<p>{greet(name)}</p>
</main>
