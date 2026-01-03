<!-- a11y-missing-attribute: Tests for missing required attributes -->
<script>
	let src = $state('/images/photo.jpg');
	let href = $state('/about');
</script>

<!-- img without alt attribute -->
<img src="/photo.jpg" />
<img {src} />
<img src="/logo.png" width="100" height="50" />

<!-- area without alt attribute (in image map) -->
<map name="imagemap">
	<area shape="rect" coords="0,0,100,100" href="/link1" />
	<area shape="circle" coords="50,50,30" href="/link2" />
</map>

<!-- a11y-missing-content: Anchors without accessible content -->

<!-- Empty anchor -->
<a href="/about"></a>
<a {href}></a>

<!-- Anchor with only whitespace -->
<a href="/contact">   </a>

<!-- Anchor with only an icon (no aria-label) -->
<a href="/settings"><span class="icon-gear"></span></a>

<!-- These should NOT trigger (have accessible content) -->
<!-- <a href="/home" aria-label="Go home"></a> -->
<!-- <a href="/menu" aria-labelledby="menu-label"></a> -->
<!-- <a href="/profile">Profile</a> -->
