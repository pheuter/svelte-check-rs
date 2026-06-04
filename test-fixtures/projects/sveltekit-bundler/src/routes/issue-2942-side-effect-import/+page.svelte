<script lang="ts">
	// Issue #2942 refinement: a BARE side-effect import (an import declaration
	// with no clause) reaching OUTSIDE the workspace root.
	//
	// The target (`projects/shared-external/value.ts`) is a sibling of the
	// `sveltekit-bundler` workspace, so this specifier climbs out of the root.
	// The specifier scanner originally recognized only `from`, dynamic
	// `import()`, and `require()` forms — so this bare side-effect import was
	// left unrewritten and produced a spurious TS2307. After the fix it is
	// rewritten so it resolves from the generated cache location.
	import '../../../../shared-external/value';

	const x: number = 1;
</script>

<p>{x}</p>
