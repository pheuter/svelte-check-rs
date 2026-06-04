<script lang="ts">
	// `$lib` alias is declared only in svelte.config.ts (issue #3009).
	import { greet } from '$lib/greeting';

	let name = $state('world');
</script>

<main>
	<p>{greet(name)}</p>
</main>
