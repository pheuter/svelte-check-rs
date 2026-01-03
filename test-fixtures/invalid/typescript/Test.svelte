<script lang="ts">
	import Other from "./Other.svelte";
</script>

<!-- This should error: 'wrong' does not exist in type '{binary: boolean}' -->
<Other wrong />
