<!-- Parser error: UnclosedTag - Tags that are never closed -->

<!-- Simple unclosed tags -->
<div>
	<span>Content without closing span
</div>

<!-- Deeply nested unclosed tag -->
<section>
	<article>
		<div>
			<p>Unclosed paragraph
		</div>
	</article>
</section>

<!-- Multiple unclosed tags -->
<ul>
	<li>Item 1
	<li>Item 2
	<li>Item 3
</ul>

<!-- Unclosed void-like elements used as containers -->
<div>
	<span>
</div>

<!-- Unclosed element at end of file -->
<footer>
	<nav>
		<a href="/">Home
