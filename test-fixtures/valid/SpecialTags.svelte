<script>
	let htmlContent = $state('<strong>Bold text</strong>');
	let items = $state([{ id: 1 }, { id: 2 }]);
	let debugValue = $state(42);

	function getHtml() {
		return '<em>Italic</em>';
	}
</script>

<!-- @html tag -->
{@html htmlContent}
{@html '<div>Static HTML</div>'}
{@html getHtml()}
{@html `<span class="dynamic">${debugValue}</span>`}

<!-- @const tag -->
{#each items as item}
	{@const doubled = item.id * 2}
	{@const label = `Item ${item.id}`}
	<p>{label}: {doubled}</p>
{/each}

{#if debugValue > 0}
	{@const isPositive = true}
	{@const message = isPositive ? 'Positive' : 'Not positive'}
	<p>{message}</p>
{/if}

<!-- @debug tag -->
{@debug debugValue}
{@debug items, htmlContent}
{@debug}

<!-- @render tag with various expressions -->
{#snippet button(text, onClick)}
	<button on:click={onClick}>{text}</button>
{/snippet}

{@render button('Click', () => console.log('clicked'))}
{@render button(...['Submit', handleSubmit])}

{#snippet list(items)}
	<ul>
		{#each items as item}
			<li>{item}</li>
		{/each}
	</ul>
{/snippet}

{@render list([1, 2, 3])}
{@render list(items.map(i => i.id))}
