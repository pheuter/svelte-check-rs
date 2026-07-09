<script module lang="ts">
	export type FancyProps = { label: string };
	export const theme = 'blue';
</script>

<script lang="ts">
	let { label }: FancyProps = $props();
</script>

<p>{label}</p>
