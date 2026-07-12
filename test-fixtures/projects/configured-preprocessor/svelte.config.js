import MagicString from 'magic-string';

const configuredPreprocessor = {
	name: 'configured-preprocessor-fixture',
	markup({ content, filename }) {
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
		const numberPlaceholder = '"__PREPROCESS_TO_NUMBER__"';
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

		return {
			code: code.toString(),
			// Exercise the plain-object form accepted by the Svelte preprocess API.
			map: JSON.parse(
				code.generateMap({ source: filename, includeContent: true, hires: true }).toString()
			)
		};
	}
};

const config = {
	preprocess: configuredPreprocessor
};

export default process.env.SVELTE_CHECK_RS_TEST_MISSING_CONFIG_EXPORT === '1'
	? undefined
	: config;
