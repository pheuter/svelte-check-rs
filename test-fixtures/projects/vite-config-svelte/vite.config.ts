// Plain Svelte + Vite app whose Svelte config lives ONLY in vite.config.ts
// (issue #3031). There is intentionally NO svelte.config.* file, so this proves
// the vite.config static approximation is honored on its own.
//
// NOTE: svelte-check-rs reads this STATICALLY with SWC; upstream runs
// vite.resolveConfig at runtime. See SvelteConfig::load for the divergence.
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
	plugins: [
		svelte({
			compilerOptions: {
				runes: true,
				experimental: {
					async: true
				}
			},
			extensions: ['.svelte'],
			kit: {
				alias: {
					'$lib': './src/lib'
				}
			}
		})
	]
});
