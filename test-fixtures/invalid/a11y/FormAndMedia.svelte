<!-- Tests for form and media a11y rules -->
<script>
	let name = $state('');
	let email = $state('');
</script>

<!-- a11y-label-has-associated-control: Labels without controls -->
<label>Name</label>
<label>Email Address</label>
<label>Phone Number</label>

<!-- Label with text but no for attribute or nested control -->
<label>
	Username
	<span class="required">*</span>
</label>

<!-- Nested non-control elements don't count -->
<label>
	Description
	<span>Some helper text</span>
	<div>More info</div>
</label>

<!-- Multiple labels without controls -->
<form>
	<label>First Name</label>
	<label>Last Name</label>
	<label>Address</label>
	<label>City</label>
</form>

<!-- a11y-media-has-caption: Video without captions -->
<video src="/video.mp4">No captions</video>
<video src="/video.mp4" controls>Video with controls but no captions</video>
<video src="/video.mp4" autoplay>Autoplay video without captions</video>

<!-- Video with track but wrong kind -->
<video src="/video.mp4">
	<track kind="subtitles" src="/subs.vtt" />
</video>

<video src="/video.mp4">
	<track kind="descriptions" src="/desc.vtt" />
</video>

<!-- Multiple videos without captions -->
<video src="/intro.mp4"></video>
<video src="/tutorial.mp4"></video>
<video src="/demo.mp4"></video>

<!-- a11y-autofocus: Avoid autofocus -->
<input autofocus />
<input type="text" autofocus />
<input type="email" autofocus />
<textarea autofocus></textarea>
<select autofocus>
	<option>Option 1</option>
</select>
<button autofocus>Autofocus button</button>

<!-- a11y-accesskey: Avoid accesskey -->
<button accesskey="s">Save (Alt+S)</button>
<a href="/home" accesskey="h">Home (Alt+H)</a>
<input type="text" accesskey="n" />
<div accesskey="d">Div with accesskey</div>

<!-- Multiple accesskeys -->
<nav>
	<a href="/page1" accesskey="1">Page 1</a>
	<a href="/page2" accesskey="2">Page 2</a>
	<a href="/page3" accesskey="3">Page 3</a>
</nav>
