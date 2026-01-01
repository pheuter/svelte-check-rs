// SvelteKit configuration with aliases
export default {
	kit: {
		alias: {
			'$lib': './src/lib',
			'$components': './src/components'
		}
	},
	compilerOptions: {
		runes: true
	},
	extensions: ['.svelte']
};
