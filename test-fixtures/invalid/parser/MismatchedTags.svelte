<!-- Parser error: MismatchedClosingTag - Closing tags don't match opening tags -->

<!-- Simple mismatch -->
<div>Content</span>

<!-- Swapped closing tags -->
<div><span>Content</div></span>

<!-- Wrong element name -->
<section>
	<article>Content</div>
</section>

<!-- Nested mismatches -->
<ul>
	<li>Item</ol>
</ul>

<!-- Case sensitivity (if applicable) -->
<DIV>Content</div>

<!-- Component-like mismatch -->
<Button>Click me</button>

<!-- Multiple mismatches in sequence -->
<header>
	<nav>Links</header>
	<main>Content</nav>
</main>
