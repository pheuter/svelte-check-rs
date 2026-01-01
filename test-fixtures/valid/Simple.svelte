<script>
	let name = $state('world');
</script>

<h1>Hello {name}!</h1>

<input bind:value={name} />
