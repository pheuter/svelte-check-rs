// Present alongside vite.config.ts to prove vite.config wins (issue #3031).
// If this file were (incorrectly) preferred, the `$lib` alias would resolve to
// "./from-svelte" instead of vite.config's "./from-vite".
export default {
	kit: {
		alias: {
			'$lib': './from-svelte'
		}
	}
};
