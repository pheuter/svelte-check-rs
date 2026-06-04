<script lang="ts">
	// `$lib` alias is declared only in vite.config.ts (issue #3031).
	import { greet } from '$lib/greeting';

	let name = $state('world');
</script>

<main>
	<p>{greet(name)}</p>
</main>
