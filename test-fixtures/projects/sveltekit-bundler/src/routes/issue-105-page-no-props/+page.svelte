<script lang="ts">
	// This page component does NOT use $props().
	// $props.id() does not declare props, so this should not get PageProps forced,
	// and mount() should work without providing props.
	let title = 'Hello';
	const uid = $props.id();
</script>

<h1 id={uid}>{title}</h1>
