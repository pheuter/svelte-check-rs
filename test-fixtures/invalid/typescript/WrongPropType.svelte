<script lang="ts">
	import Other from "./Other.svelte";
</script>

<!-- This should error: Type 'string' is not assignable to type 'boolean' -->
<Other binary="not-a-boolean" />
