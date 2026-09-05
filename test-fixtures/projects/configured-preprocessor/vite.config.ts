import { svelte } from '@sveltejs/vite-plugin-svelte';
import MagicString from 'magic-string';

// Vite 8's bundled loader rewrites this into a Node-only virtual module.
if (!import.meta.resolve('svelte')) throw new Error('Svelte resolution failed');

console.log('vite config stdout');
if (process.env.SVELTE_CHECK_RS_TEST_PROTOCOL_OUTPUT === '1') {
	console.log('{"id":1,"config":{"found":false}}');
	console.log('vite config multiline first\nvite config multiline second');
	process.stdout.write('vite config partial');
	process.stdout.write(' write\r\n');
}

const inlinePreprocessor = {
	name: 'vite-inline-preprocessor',
	markup({ content, filename }) {
		console.log('vite preprocessor stdout');
		const code = new MagicString(content);
		const placeholder = '"__VITE_ONLY_TO_NUMBER__"';
		const start = content.indexOf(placeholder);
		if (start !== -1) code.overwrite(start, start + placeholder.length, '42');
		return {
			code: code.toString(),
			map: code.generateMap({ source: filename, includeContent: true, hires: true })
		};
	}
};

const plugins = process.env.SVELTE_CHECK_RS_TEST_VITE_INLINE === '1'
	? [svelte({ configFile: false, preprocess: inlinePreprocessor })]
	: process.env.SVELTE_CHECK_RS_TEST_VITE_CONFIG_FILE === '1'
		? [svelte({ configFile: './custom.svelte.config.js' })]
		: process.env.SVELTE_CHECK_RS_TEST_VITE_NO_PREPROCESS === '1'
			? [svelte({ configFile: false })]
			: [];

export default { plugins };
