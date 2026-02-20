<script lang="ts">
	function handleMount(node: HTMLElement, file: { name: string }) {
		return { destroy() {} };
	}

	function tooltip(text: string) {
		return (node: HTMLElement) => ({ destroy() {} });
	}

	function getText(key: string) { return key; }

	let file = { name: 'test.png' };
</script>

<!-- use:action(args) shorthand — absorbed into directive name per official Svelte parser -->
<img use:handleMount(file) alt="test" />

<!-- Nested call expression in parens -->
<div use:tooltip(getText('hello')) class="box"></div>

<!-- Empty parens -->
<div use:tooltip()></div>

<!-- Chained calls -->
<div use:tooltip(getText('a'))(extra)></div>
