<!-- Parser error: InvalidBlockSyntax - Malformed block statements -->
<script>
	let items = $state([1, 2, 3]);
	let show = $state(true);
	let promise = $state(Promise.resolve('data'));
</script>

<!-- If block with missing condition -->
{#if}
	<p>No condition</p>
{/if}

<!-- If block with invalid condition -->
{#if =>}
	<p>Invalid condition syntax</p>
{/if}

<!-- Each block with missing expression -->
{#each}
	<li>No items</li>
{/each}

<!-- Each block with missing 'as' variable -->
{#each items}
	<li>No variable</li>
{/each}

<!-- Each block with invalid destructuring -->
{#each items as {}}
	<li>Empty destructure</li>
{/each}

<!-- Each block with malformed key -->
{#each items as item (}
	<li>{item}</li>
{/each}

<!-- Await block with missing expression -->
{#await}
	<p>Loading</p>
{/await}

<!-- Await block with invalid then syntax -->
{#await promise then}
	<p>Missing variable</p>
{/await}

<!-- Await block with invalid catch syntax -->
{#await promise}
	<p>Loading</p>
{:catch}
	<p>Missing error variable</p>
{/await}

<!-- Key block with missing expression -->
{#key}
	<div>No key</div>
{/key}

<!-- Snippet block with missing name -->
{#snippet}
	<p>No name</p>
{/snippet}

<!-- Snippet block with invalid parameters -->
{#snippet mySnippet(}
	<p>Unclosed params</p>
{/snippet}

<!-- Invalid block type -->
{#unknown condition}
	<p>Unknown block type</p>
{/unknown}

<!-- Mismatched block closing -->
{#if show}
	<p>Content</p>
{/each}

<!-- Else without if -->
{:else}
	<p>Orphan else</p>

<!-- Then without await -->
{:then value}
	<p>Orphan then</p>

<!-- Catch without await -->
{:catch error}
	<p>Orphan catch</p>
