import MagicString from 'magic-string';
import fs from 'node:fs';
import './config-dependency.js';

if (process.env.SVELTE_CHECK_RS_TEST_PATCH_STDOUT === '1') {
	process.stdout.write = () => true;
}

const configuredPreprocessor = {
	name: 'configured-preprocessor-fixture',
	markup({ content, filename }) {
		if (process.env.SVELTE_CHECK_RS_TEST_PROTOCOL_OUTPUT === '1') {
			console.log('{"id":1,"code":"not a protocol frame"}');
			console.log('preprocessor multiline first\npreprocessor multiline second');
			process.stdout.write('preprocessor partial');
			process.stdout.write(' write\n');
		}
		const workerControl = process.env.SVELTE_CHECK_RS_TEST_WORKER_CONTROL;
		if (workerControl && fs.readFileSync(workerControl, 'utf8').trim() === 'crash') {
			fs.writeFileSync(workerControl, 'ok\n');
			process.exit(17);
		}
		if (
			process.env.SVELTE_CHECK_RS_TEST_VITE_INLINE === '1' ||
			process.env.SVELTE_CHECK_RS_TEST_VITE_CONFIG_FILE === '1' ||
			process.env.SVELTE_CHECK_RS_TEST_VITE_NO_PREPROCESS === '1'
		) {
			throw new Error('the conventional Svelte config must not win over Vite');
		}
		console.log('configured preprocessor stdout');
		const continueAfterError =
			process.env.SVELTE_CHECK_RS_TEST_CONTINUE_AFTER_PREPROCESS_ERROR === '1';
		if (continueAfterError && filename.endsWith('App.svelte')) {
			const error = new Error('fixture preprocessor failure');
			error.location = {
				start: { offset: 0 },
				end: { offset: 1 }
			};
			throw error;
		}

		const code = new MagicString(content);
		const numberPlaceholder = content.includes('"__PREPROCESS_TO_NUMBER__"')
			? '"__PREPROCESS_TO_NUMBER__"'
			: '"__VITE_ONLY_TO_NUMBER__"';
		const numberStart = content.indexOf(numberPlaceholder);
		const headingStart = content.indexOf('h3');
		const imageEnd = content.indexOf(' />', content.indexOf('<img src="fixture.png"'));
		const emitDiagnostic = process.env.SVELTE_CHECK_RS_TEST_MAPPED_DIAGNOSTIC === '1';

		if (continueAfterError && filename.endsWith('Child.svelte')) {
			const defaultValue = "'child'";
			const defaultStart = content.indexOf(defaultValue);
			code.overwrite(defaultStart, defaultStart + defaultValue.length, '42');
		}

		if (numberStart !== -1) {
			const replacement = emitDiagnostic ? '"still a string"' : '42';
			code.overwrite(numberStart, numberStart + numberPlaceholder.length, replacement);
		}
		if (emitDiagnostic) {
			code.prepend('\n\n\n');
		}
		if (headingStart !== -1 && !emitDiagnostic) {
			code.overwrite(headingStart, headingStart + 2, 'h2');
			code.overwrite(content.lastIndexOf('h3'), content.lastIndexOf('h3') + 2, 'h2');
		}
		if (imageEnd !== -1 && !emitDiagnostic) {
			code.appendLeft(imageEnd, ' alt=""');
		}
		if (process.env.SVELTE_CHECK_RS_TEST_MAPLESS_CHANGE === '1') {
			return { code: code.toString() };
		}

		const dependencies = [
			process.env.SVELTE_CHECK_RS_TEST_EXTERNAL_DEPENDENCY ||
				new URL('./heading.txt', import.meta.url)
		];
		if (workerControl) dependencies.push(workerControl);
		return {
			code: code.toString(),
			dependencies,
			// Exercise the plain-object form accepted by the Svelte preprocess API.
			map: JSON.parse(
				code.generateMap({ source: filename, includeContent: true, hires: true }).toString()
			)
		};
	},
	script({ content }) {
		if (process.env.SVELTE_CHECK_RS_TEST_SCRIPT_ERROR !== '1') return;
		const error = new Error('script fragment failure');
		error.location = {
			start: { line: 2, column: 1 },
			end: { line: 2, column: 4 }
		};
		throw error;
	},
	style() {
		if (process.env.SVELTE_CHECK_RS_TEST_EXTERNAL_STYLE_ERROR !== '1') return;
		const error = new Error('external Sass failure');
		error.span = {
			url: new URL('./src/_partial.scss', import.meta.url),
			start: { line: 2, column: 2 },
			end: { line: 2, column: 5 }
		};
		throw error;
	}
};

const maplessPreprocessor = {
	name: 'mapless-preprocessor-fixture',
	markup({ content }) {
		return {
			code: content.replace('"__PREPROCESS_TO_NUMBER__"', '42')
		};
	}
};

const config = {
	preprocess:
		process.env.SVELTE_CHECK_RS_TEST_MAPLESS_CHANGE === '1'
			? maplessPreprocessor
			: configuredPreprocessor
};

export default process.env.SVELTE_CHECK_RS_TEST_MISSING_CONFIG_EXPORT === '1'
	? undefined
	: config;
