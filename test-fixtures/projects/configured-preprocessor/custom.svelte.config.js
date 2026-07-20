import MagicString from 'magic-string';

export default {
	preprocess: {
		name: 'custom-config-file-preprocessor',
		markup({ content, filename }) {
			const code = new MagicString(content);
			const placeholder = '"__VITE_ONLY_TO_NUMBER__"';
			const start = content.indexOf(placeholder);
			if (start !== -1) code.overwrite(start, start + placeholder.length, '42');
			return {
				code: code.toString(),
				map: code.generateMap({ source: filename, includeContent: true, hires: true })
			};
		}
	}
};
