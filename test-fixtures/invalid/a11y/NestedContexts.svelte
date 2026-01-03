<!-- A11y violations in nested contexts and complex structures -->
<script>
	let items = $state([1, 2, 3]);
	let sections = $state(['a', 'b', 'c']);
	function handleClick() {}
</script>

<!-- Heading hierarchy violations in nested components/elements -->

<!-- Nested sections with skipped headings -->
<section>
	<h1>Main Title</h1>
	<section>
		<h4>Skipped to h4 in nested section</h4>
	</section>
</section>

<!-- Articles with heading violations -->
<article>
	<h1>Article Title</h1>
	<div>
		<h5>Deeply nested heading skip</h5>
	</div>
</article>

<!-- Headings in conditionals that skip levels -->
<h1>Page Title</h1>
{#if true}
	<h4>Conditional heading skip</h4>
{/if}

<!-- Headings in loops -->
<h1>Gallery</h1>
{#each sections as section}
	<h6>Each section skips to h6</h6>
{/each}

<!-- Nested interactive elements with violations -->

<!-- Click handlers in nested non-interactive elements -->
<div>
	<section>
		<article>
			<div onclick={handleClick}>Deeply nested click without keyboard</div>
		</article>
	</section>
</div>

<!-- Multiple levels of static elements with interactions -->
<main>
	<section onclick={handleClick}>
		<article onclick={handleClick}>
			<div onclick={handleClick}>
				<span onclick={handleClick}>Many nested clicks without roles</span>
			</div>
		</article>
	</section>
</main>

<!-- Labels with nested non-control elements -->
<label>
	<div>
		<span>
			<strong>Nested text but no control</strong>
		</span>
	</div>
</label>

<!-- Labels in complex structures -->
<form>
	<fieldset>
		<legend>Contact</legend>
		<label>
			<span class="label-text">Name</span>
			<span class="required">*</span>
			<!-- Missing actual input -->
		</label>
	</fieldset>
</form>

<!-- Nested tabindex issues -->
<div tabindex="0">
	<div tabindex="1">
		<div tabindex="2">
			<div tabindex="3">Deeply nested positive tabindex chain</div>
		</div>
	</div>
</div>

<!-- Images in complex nested structures without alt -->
<figure>
	<picture>
		<source srcset="/photo.webp" type="image/webp" />
		<img src="/photo.jpg" />
	</picture>
</figure>

<div class="gallery">
	<div class="row">
		<div class="col">
			<img src="/img1.jpg" />
		</div>
		<div class="col">
			<img src="/img2.jpg" />
		</div>
	</div>
</div>

<!-- Videos nested in multiple wrappers -->
<div class="video-container">
	<div class="video-wrapper">
		<div class="video-inner">
			<video src="/video.mp4" controls></video>
		</div>
	</div>
</div>

<!-- Empty anchors in navigation structures -->
<nav>
	<ul>
		<li><a href="/page1"></a></li>
		<li><a href="/page2"></a></li>
		<li>
			<a href="/page3">
				<span class="icon"></span>
				<!-- Text missing -->
			</a>
		</li>
	</ul>
</nav>

<!-- ARIA issues in nested structures -->
<div aria-hidden="true">
	<div>
		<button>Focusable button inside aria-hidden ancestor</button>
	</div>
</div>

<!-- Role issues in nested elements -->
<div role="list">
	<div role="listitem">
		<div role="checkbox">Nested checkbox without aria-checked</div>
	</div>
</div>

<!-- Redundant roles in nested semantic elements -->
<nav role="navigation">
	<ul role="list">
		<li role="listitem">
			<a href="/home" role="link">Home</a>
		</li>
	</ul>
</nav>

<!-- Distracting elements nested in content -->
<main>
	<article>
		<section>
			<marquee>Nested marquee in article</marquee>
		</section>
	</article>
</main>

<header>
	<div class="banner">
		<blink>Nested blink in header</blink>
	</div>
</header>

<!-- Mouse events without focus in nested structures -->
<div class="tooltip-container">
	<div class="tooltip-trigger" onmouseover={() => console.log('show')}>
		<span onmouseout={() => console.log('hide')}>Hover me</span>
	</div>
</div>

<!-- Complex each block nesting with violations -->
{#each items as item}
	<div onclick={handleClick}>
		{#each sections as section}
			<span onclick={handleClick}>
				Nested each with clicks
			</span>
		{/each}
	</div>
{/each}

<!-- Snippets with nested a11y issues -->
{#snippet card(title)}
	<article>
		<h6>Heading in snippet skips levels</h6>
		<img src="/card.jpg" />
		<a href="/more"></a>
	</article>
{/snippet}

<h1>Cards</h1>
{@render card('Card 1')}
{@render card('Card 2')}

<!-- Interactive elements in non-interactive containers -->
<p tabindex="0">Paragraph with tabindex</p>
<h2 tabindex="0">Heading with tabindex</h2>
<blockquote tabindex="0">Blockquote with tabindex</blockquote>
<pre tabindex="0">Pre with tabindex</pre>
<code tabindex="0">Code with tabindex</code>

<!-- Multiple autofocus in nested form -->
<form>
	<div>
		<input autofocus type="text" />
	</div>
	<div>
		<textarea autofocus></textarea>
	</div>
</form>

<!-- Accesskeys in nested elements -->
<div>
	<nav>
		<a href="/a" accesskey="a">A</a>
		<a href="/b" accesskey="b">B</a>
		<a href="/c" accesskey="c">C</a>
	</nav>
</div>
