<script lang="ts">
	function handleMount(node: HTMLElement, file: { name: string }) {
		return { destroy() {} };
	}

	let file = { name: 'test.png' };
</script>

<!-- use:action(args) shorthand syntax — equivalent to use:action={args} -->
<img use:handleMount(file) alt="test" />
