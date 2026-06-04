<script lang="ts">
	// Issue #2942: relative import reaching OUTSIDE the workspace root.
	//
	// The target (`projects/shared-external/value.ts`) is a sibling of the
	// `sveltekit-bundler` workspace, so this specifier climbs out of the root.
	// When the transformed file is written to the generated cache folder, this
	// import must be rewritten so it still resolves from the cache location.
	// Without the fix this produces a spurious TS2307.
	import { sharedValue, sharedGreeting } from '../../../../shared-external/value';
	import type { SharedConfig } from '../../../../shared-external/value';

	const greeting: string = sharedGreeting('world');
	const doubled: number = sharedValue * 2;
	const config: SharedConfig = { enabled: true, label: 'demo' };
</script>

<h1>{greeting}</h1>
<p>{doubled}</p>
<p>{config.label}</p>
