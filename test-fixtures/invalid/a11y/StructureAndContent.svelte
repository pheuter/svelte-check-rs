<!-- Tests for structure and content a11y rules -->

<!-- a11y-structure: Heading hierarchy violations -->

<!-- Skipping from h1 to h3 -->
<h1>Main Title</h1>
<h3>Skipped h2</h3>

<!-- Skipping from h1 to h4 -->
<h1>Title</h1>
<h4>Skipped two levels</h4>

<!-- Skipping from h2 to h5 -->
<h1>Page Title</h1>
<h2>Section</h2>
<h5>Skipped from h2 to h5</h5>

<!-- Multiple heading skips in sequence -->
<article>
	<h1>Article Title</h1>
	<h3>First subsection</h3>
	<h5>Deep subsection</h5>
	<h6>Even deeper</h6>
</article>

<!-- Skipping to h6 from h1 -->
<h1>Top</h1>
<h6>Skipped to h6</h6>

<!-- a11y-img-redundant-alt: Alt text containing redundant words -->
<img src="/photo.jpg" alt="Image of a sunset" />
<img src="/photo.jpg" alt="Picture of mountains" />
<img src="/photo.jpg" alt="Photo of the team" />
<img src="/photo.jpg" alt="This is an image of a cat" />
<img src="/photo.jpg" alt="A picture showing the results" />
<img src="/photo.jpg" alt="Photo: Company logo" />

<!-- Case insensitive -->
<img src="/photo.jpg" alt="IMAGE of something" />
<img src="/photo.jpg" alt="PICTURE here" />
<img src="/photo.jpg" alt="PHOTO of event" />

<!-- Redundant words in longer alt text -->
<img src="/photo.jpg" alt="This is a beautiful image of a waterfall in the mountains" />
<img src="/photo.jpg" alt="A professional photo taken during the conference" />
<img src="/photo.jpg" alt="Picture showing the before and after comparison" />

<!-- a11y-distracting-elements: Deprecated distracting elements -->
<marquee>Scrolling text is distracting</marquee>
<marquee direction="right">Right scrolling</marquee>
<marquee behavior="alternate">Bouncing text</marquee>

<blink>Blinking text is even worse</blink>

<!-- Multiple distracting elements -->
<div>
	<marquee>First scrolling text</marquee>
	<marquee>Second scrolling text</marquee>
	<blink>Blinking content</blink>
</div>

<!-- Nested distracting elements -->
<marquee>
	<blink>Scrolling and blinking</blink>
</marquee>
