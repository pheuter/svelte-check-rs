<!-- Parser error: UnclosedBlock - Svelte blocks without closing tags -->
<script>
	let show = $state(true);
	let items = $state([1, 2, 3]);
	let promise = $state(Promise.resolve('data'));
	let key = $state(1);
</script>

<!-- Unclosed if block -->
{#if show}
	<p>Showing content</p>

<!-- Unclosed each block -->
{#each items as item}
	<li>{item}</li>

<!-- Unclosed await block -->
{#await promise}
	<p>Loading...</p>

<!-- Unclosed key block -->
{#key key}
	<div>Keyed content</div>

<!-- Unclosed snippet block -->
{#snippet mySnippet()}
	<p>Snippet content</p>

<!-- Nested unclosed blocks -->
{#if show}
	{#each items as item}
		<span>{item}</span>

<!-- Missing else closing -->
{#if show}
	<p>True</p>
{:else}
	<p>False</p>

<!-- Unclosed if with else if -->
{#if show}
	<p>First</p>
{:else if !show}
	<p>Second</p>
