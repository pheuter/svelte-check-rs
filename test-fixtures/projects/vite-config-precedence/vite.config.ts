// Precedence fixture (issue #3031): BOTH a vite.config.ts (with Svelte plugin
// options) and a svelte.config.js exist. vite.config is preferred when it yields
// options, so the `$lib` alias resolved here ("./from-vite") must win over the
// svelte.config.js alias ("./from-svelte").
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
	plugins: [
		svelte({
			compilerOptions: {
				runes: true
			},
			kit: {
				alias: {
					'$lib': './from-vite'
				}
			}
		})
	]
});
