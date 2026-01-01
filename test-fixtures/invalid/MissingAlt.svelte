<!-- This file has a11y issues -->
<script>
	let src = $state('/images/photo.jpg');
</script>

<!-- Missing alt attribute -->
<img {src} />

<!-- Empty anchor -->
<a href="/about"></a>

<!-- Positive tabindex -->
<div tabindex="5">Content</div>

<!-- Distracting element -->
<marquee>Scrolling text</marquee>

<!-- Autofocus -->
<input autofocus />
