// Issue #2942: a `.svelte.ts` module file with a relative import reaching
// OUTSIDE the workspace root. Module files are written to the same generated
// cache as components and hit the identical out-of-root resolution problem,
// so the import must be rewritten here too. Without the fix this is a TS2307.
import { sharedValue } from '../../../../shared-external/value';

export function createCounter() {
	let count = $state(sharedValue);
	return {
		get count() {
			return count;
		},
		increment() {
			count++;
		}
	};
}
