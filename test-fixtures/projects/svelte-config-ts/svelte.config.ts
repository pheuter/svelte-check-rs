// SvelteKit configuration authored in TypeScript (issue #3009).
// Exercises the `.ts` probe + the TypeScript SWC syntax branch + the
// `const config` + `export default config` resolution path.
import type { Config } from '@sveltejs/kit';

const config = {
	kit: {
		alias: {
			'$lib': './src/lib'
		}
	},
	compilerOptions: {
		runes: true
	},
	extensions: ['.svelte']
} satisfies Config;

export default config;
