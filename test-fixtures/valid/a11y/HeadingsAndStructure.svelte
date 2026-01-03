<!-- Valid a11y: Headings and structure that should NOT trigger any warnings -->

<!-- Proper heading hierarchy - should NOT trigger a11y-structure -->
<h1>Main Page Title</h1>
<h2>First Section</h2>
<h3>Subsection 1.1</h3>
<h3>Subsection 1.2</h3>
<h2>Second Section</h2>
<h3>Subsection 2.1</h3>
<h4>Sub-subsection 2.1.1</h4>
<h4>Sub-subsection 2.1.2</h4>
<h3>Subsection 2.2</h3>

<!-- Multiple articles with their own heading hierarchies -->
<article>
	<h2>Article 1</h2>
	<h3>Article 1 Section</h3>
	<p>Content</p>
</article>

<article>
	<h2>Article 2</h2>
	<h3>Article 2 Section</h3>
	<h4>Article 2 Subsection</h4>
	<p>Content</p>
</article>

<!-- Sections with headings -->
<section>
	<h2>Section Title</h2>
	<p>Section content</p>
	<section>
		<h3>Nested Section</h3>
		<p>Nested content</p>
	</section>
</section>

<!-- Heading levels in nav/aside (commonly h2+) -->
<nav>
	<h2>Navigation</h2>
	<ul>
		<li><a href="/home">Home</a></li>
		<li><a href="/about">About</a></li>
	</ul>
</nav>

<aside>
	<h2>Related Content</h2>
	<h3>Popular Articles</h3>
	<ul>
		<li>Article 1</li>
		<li>Article 2</li>
	</ul>
</aside>

<!-- Header and footer with headings -->
<header>
	<h1>Site Title</h1>
</header>

<main>
	<h2>Main Content</h2>
	<p>Content here</p>
</main>

<footer>
	<h2>Footer Information</h2>
	<h3>Contact Us</h3>
	<h3>Follow Us</h3>
</footer>

<!-- Same level headings (valid) -->
<h2>Section A</h2>
<p>Content A</p>
<h2>Section B</h2>
<p>Content B</p>
<h2>Section C</h2>
<p>Content C</p>

<!-- Going back up in hierarchy (valid) -->
<h1>Title</h1>
<h2>Section 1</h2>
<h3>Subsection 1.1</h3>
<h2>Section 2</h2><!-- Going back to h2 is valid -->
<h3>Subsection 2.1</h3>
<h1>New Major Section</h1><!-- Starting fresh with h1 is valid -->
<h2>Subsection</h2>

<!-- NO distracting elements - just regular content -->
<p>Regular paragraph content</p>
<div>Regular div content</div>
<span>Inline content</span>
<strong>Important text</strong>
<em>Emphasized text</em>

<!-- Animation via CSS (not marquee/blink) -->
<div class="animated">Animated via CSS</div>
<div class="fade-in">Fade in effect</div>
<div class="scroll-animation">Scroll animation</div>
