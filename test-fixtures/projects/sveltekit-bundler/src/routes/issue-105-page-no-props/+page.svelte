<script lang="ts">
	// This page component does NOT use $props().
	// It should not get PageProps forced as its prop type,
	// and mount() should work without providing props.
	let title = 'Hello';
</script>

<h1>{title}</h1>
