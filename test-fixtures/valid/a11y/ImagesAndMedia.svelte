<!-- Valid a11y: Images and media that should NOT trigger any warnings -->
<script>
	let dynamicAlt = $state('A beautiful sunset over the ocean');
	let videoSrc = $state('/video.mp4');
</script>

<!-- Images with proper alt attributes - should NOT trigger a11y-missing-attribute -->
<img src="/photo.jpg" alt="A sunset" />
<img src="/logo.png" alt="" /><!-- Empty alt is valid for decorative images -->
<img src="/icon.svg" alt="Settings icon" />
<img src={dynamicAlt} alt="Dynamic content" />
<img src="/photo.jpg" alt={dynamicAlt} />

<!-- Images with ARIA alternatives - should NOT trigger -->
<img src="/photo.jpg" aria-label="Description via aria-label" />
<img src="/photo.jpg" aria-labelledby="img-description" />
<span id="img-description">Image description</span>

<!-- Area elements with alt - should NOT trigger a11y-missing-attribute -->
<map name="imagemap">
	<area shape="rect" coords="0,0,100,100" href="/link1" alt="First region" />
	<area shape="circle" coords="50,50,30" href="/link2" alt="Second region" />
	<area shape="poly" coords="0,0,50,50,0,100" href="/link3" alt="Third region" />
</map>

<!-- Alt text that is NOT redundant - should NOT trigger a11y-img-redundant-alt -->
<img src="/sunset.jpg" alt="Golden sunset over Pacific Ocean" />
<img src="/team.jpg" alt="Our engineering team at the annual retreat" />
<img src="/logo.png" alt="Company logo" />
<img src="/diagram.svg" alt="System architecture diagram showing microservices" />
<img src="/chart.png" alt="Revenue growth chart for Q4 2024" />

<!-- Videos with proper captions - should NOT trigger a11y-media-has-caption -->
<video src="/intro.mp4">
	<track kind="captions" src="/captions.vtt" srclang="en" label="English" />
</video>

<video src="/tutorial.mp4" controls>
	<track kind="captions" src="/tutorial-captions.vtt" srclang="en" />
	<track kind="captions" src="/tutorial-captions-es.vtt" srclang="es" label="Spanish" />
</video>

<!-- Muted videos don't need captions - should NOT trigger -->
<video src="/background.mp4" muted autoplay loop></video>
<video src="/animation.mp4" muted>
	<source src="/animation.webm" type="video/webm" />
</video>

<!-- Video with both captions and subtitles -->
<video src="/movie.mp4">
	<track kind="captions" src="/captions.vtt" srclang="en" />
	<track kind="subtitles" src="/subtitles-fr.vtt" srclang="fr" />
</video>

<!-- Audio elements (no caption requirement) -->
<audio src="/podcast.mp3" controls></audio>
<audio controls>
	<source src="/music.ogg" type="audio/ogg" />
	<source src="/music.mp3" type="audio/mpeg" />
</audio>
