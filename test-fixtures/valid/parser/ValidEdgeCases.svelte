<!-- Valid parser edge cases that should NOT produce errors -->
<!-- These test that unusual but valid syntax parses correctly -->
<script>
	let count = $state(0);
	let items = $state([1, 2, 3]);
	let obj = $state({ a: 1, b: 2 });
	let Component = $state(null);
	function action(node) { return { destroy() {} }; }
</script>

<!-- Valid self-closing elements -->
<br />
<hr />
<img src="/photo.jpg" alt="Photo" />
<input type="text" />
<meta charset="utf-8" />
<link rel="stylesheet" href="/style.css" />

<!-- Self-closing in Svelte (valid) -->
<div />
<span />
<Component />

<!-- Empty elements -->
<div></div>
<span></span>
<p></p>

<!-- Whitespace only content -->
<div>   </div>
<span>
</span>

<!-- Valid attribute variations -->
<div class="simple"></div>
<div class='single-quoted'></div>
<div class={count}></div>
<div {count}></div>
<div data-value={count}></div>
<div data-obj={obj}></div>
<div {...obj}></div>
<div {...$$props}></div>
<div {...$$restProps}></div>

<!-- Boolean attributes -->
<input disabled />
<input readonly />
<input required />
<button disabled={true}></button>
<button disabled={false}></button>

<!-- Quoted attribute values (unquoted numerics not supported in Svelte) -->
<div data-value="123"></div>
<div data-value="abc"></div>

<!-- Complex expressions in attributes -->
<div class={count > 0 ? 'positive' : 'zero'}></div>
<div class={`prefix-${count}`}></div>
<div class={items.join(' ')}></div>
<div style={`color: ${count > 0 ? 'green' : 'red'}`}></div>

<!-- Valid directives with all modifiers -->
<button on:click|preventDefault|stopPropagation={()=>{}}>Click</button>
<button on:click|once|capture={()=>{}}>Once</button>
<input bind:value={count} />
<input bind:this={obj} />
<div class:active={count > 0}></div>
<div class:active></div>
<div style:color="red"></div>
<div style:color={count > 0 ? 'green' : 'red'}></div>
<div use:action></div>
<div use:action={obj}></div>
<div transition:fade></div>
<div transition:fade={{ duration: 200 }}></div>
<div in:fade></div>
<div out:fade></div>
<div animate:flip></div>

<!-- Valid block syntax -->
{#if count > 0}
	<p>Positive</p>
{:else if count < 0}
	<p>Negative</p>
{:else}
	<p>Zero</p>
{/if}

{#each items as item, index (item)}
	<p>{index}: {item}</p>
{:else}
	<p>No items</p>
{/each}

{#each items as { value, label }}
	<p>{label}: {value}</p>
{/each}

{#await Promise.resolve(1)}
	<p>Loading...</p>
{:then value}
	<p>Value: {value}</p>
{:catch error}
	<p>Error: {error}</p>
{/await}

{#await Promise.resolve(1) then value}
	<p>Value: {value}</p>
{/await}

{#key count}
	<p>{count}</p>
{/key}

<!-- Valid snippets -->
{#snippet greeting(name)}
	<p>Hello, {name}!</p>
{/snippet}

{#snippet complex(a, b, c)}
	<div>{a} + {b} = {c}</div>
{/snippet}

{@render greeting('World')}
{@render complex(1, 2, 3)}

<!-- Valid special tags -->
{@html '<div>Raw HTML</div>'}
{@const doubled = count * 2}
{@debug count, doubled}

<!-- Valid Svelte elements -->
<svelte:head>
	<title>Page Title</title>
	<meta name="description" content="Description" />
</svelte:head>

<svelte:body on:click={() => {}} />
<svelte:window on:resize={() => {}} bind:innerWidth={count} />
<svelte:document on:visibilitychange={() => {}} />

<svelte:element this="div">Dynamic element</svelte:element>
<svelte:element this={count > 0 ? 'span' : 'div'}>Conditional element</svelte:element>

<svelte:component this={Component} prop={count} />

<svelte:fragment slot="named">Fragment content</svelte:fragment>

<svelte:options immutable={true} />

<!-- Valid comments -->
<!-- Simple comment -->
<!-- Multi
line
comment -->
<!-- Comment with special chars: <div> {expression} -->

<!-- Deeply nested valid structure -->
<div>
	<section>
		<article>
			<header>
				<nav>
					<ul>
						<li>
							<a href="/">
								<span>{count}</span>
							</a>
						</li>
					</ul>
				</nav>
			</header>
		</article>
	</section>
</div>

<!-- Nested blocks -->
{#if count > 0}
	{#each items as item}
		{#if item > 1}
			{#key item}
				<p>{item}</p>
			{/key}
		{/if}
	{/each}
{/if}

<!-- Unicode content -->
<p>Hello, 世界! 🌍</p>
<p>Ümlauts and äccénts</p>
<p>Emoji: 🎉 🚀 ✨</p>

<!-- HTML entities -->
<p>&lt;div&gt;</p>
<p>&amp;&nbsp;&copy;</p>
<p>&#60;&#62;&#38;</p>
<p>&#x3C;&#x3E;&#x26;</p>
