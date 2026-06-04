// SvelteKit configuration authored as CommonJS (`module.exports`).
// Exercises the `.cjs` probe + the CommonJS `module.exports = { ... }`
// extraction path (the static extractor must read module.exports, not just
// ESM `export default`).
module.exports = {
	kit: {
		alias: {
			'$lib': './src/lib'
		}
	},
	compilerOptions: {
		runes: true
	},
	extensions: ['.svelte']
};
