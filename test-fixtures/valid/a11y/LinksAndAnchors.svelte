<!-- Valid a11y: Links and anchors that should NOT trigger any warnings -->
<script>
	let linkText = $state('Click here');
	let href = $state('/about');
</script>

<!-- Anchors with text content - should NOT trigger a11y-missing-content -->
<a href="/home">Home</a>
<a href="/about">About Us</a>
<a href="/contact">Contact</a>
<a href={href}>Dynamic link</a>
<a href="/page">{linkText}</a>

<!-- Anchors with nested text content -->
<a href="/profile">
	<span>View Profile</span>
</a>
<a href="/settings">
	<strong>Settings</strong> page
</a>

<!-- Anchors with aria-label - should NOT trigger -->
<a href="/home" aria-label="Go to homepage"></a>
<a href="/menu" aria-label="Open navigation menu">
	<span class="icon-menu"></span>
</a>
<a href="/close" aria-label="Close dialog">
	<svg viewBox="0 0 24 24"><path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/></svg>
</a>

<!-- Anchors with aria-labelledby -->
<span id="link-desc">Download our mobile app</span>
<a href="/download" aria-labelledby="link-desc">
	<img src="/app-icon.png" alt="" />
</a>

<!-- Anchors with icon + visible text -->
<a href="/github">
	<span class="icon-github"></span>
	GitHub Repository
</a>
<a href="/twitter">
	<svg class="icon" viewBox="0 0 24 24"></svg>
	Follow us on Twitter
</a>

<!-- Anchors with role="button" (still valid) -->
<a href="#" role="button" onclick={() => {}}>Click me</a>

<!-- Skip links -->
<a href="#main-content" class="skip-link">Skip to main content</a>
<a href="#navigation" class="skip-link">Skip to navigation</a>

<!-- Anchor tags without href (valid, just not links) -->
<a id="section-1">Section 1</a>
<a name="bookmark">Bookmark location</a>

<!-- Links with tabindex (valid positive prevention for tabindex rules on interactive elements) -->
<a href="/page" tabindex="0">Normal tabindex</a>
<a href="/page" tabindex="-1">Programmatically focusable</a>
