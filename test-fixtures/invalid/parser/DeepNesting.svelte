<!-- Parser edge cases: Deep nesting and recovery scenarios -->
<script>
	let show = $state(true);
	let items = $state([1, 2, 3]);
</script>

<!-- Deeply nested unclosed tags -->
<div>
	<div>
		<div>
			<div>
				<div>
					<span>Unclosed at depth 6
				</div>
			</div>
		</div>
	</div>
</div>

<!-- Deeply nested mismatched tags -->
<section>
	<article>
		<div>
			<span>Content</div><!-- Mismatch at depth 4 -->
		</span>
	</article>
</section>

<!-- Nested blocks with unclosed inner block -->
{#if show}
	{#each items as item}
		{#if item > 0}
			<p>Nested block content</p>
		<!-- Missing /if for innermost block -->
	{/each}
{/if}

<!-- Mixed HTML and block nesting errors -->
<div>
	{#if show}
		<span>
	{/if}<!-- Closed block before tag -->
		</span>
</div>

<!-- Multiple unclosed tags at different levels -->
<nav>
	<ul>
		<li>
			<a href="/">
				<span>
					<!-- All unclosed from here -->

<!-- Deeply nested expression errors -->
<div>
	<span>
		<p>
			{#if show}
				{unclosed
			{/if}
		</p>
	</span>
</div>

<!-- Chain of mismatched closings -->
</span>
</p>
</div>
</article>
</section>
</main>

<!-- Interleaved block and HTML errors -->
{#if show}
	<div>
{:else}
		<span>
	</div>
{/if}
		</span>

<!-- Unclosed elements inside blocks -->
{#each items as item}
	<li>
		<a href="/{item}">
			Item {item}
		<!-- li, a, and potentially each all unclosed -->
{/each}

<!-- Recursive-like structure errors -->
<div class="level-1">
	<div class="level-2">
		<div class="level-3">
			<div class="level-4">
				<div class="level-5">
					<div class="level-6">
						<div class="level-7">
							<div class="level-8">
								Text at level 8
							<!-- All 8 divs unclosed -->
