<script lang="ts">
	interface Props {
		checked?: boolean;
	}

	let { checked = $bindable(false) }: Props = $props();
</script>

<input type="checkbox" bind:checked />
