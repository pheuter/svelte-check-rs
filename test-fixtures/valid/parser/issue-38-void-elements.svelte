<script>
  let text = 'Hello'
</script>

<!-- Issue #38: HTML void elements should not require closing tags -->
<div>
  line 1<br>
  line 2
</div>

<!-- More void element test cases -->
<div>
  <hr>
  <img src="test.png" alt="test">
  <input type="text">
  <meta charset="utf-8">
  <link rel="stylesheet" href="style.css">
  <area shape="rect" coords="0,0,10,10">
  <base href="/">
  <col span="2">
  <embed src="video.mp4">
  <param name="autoplay" value="true">
  <source src="audio.mp3" type="audio/mpeg">
  <track kind="subtitles" src="subs.vtt">
  <wbr>
</div>

<!-- Mixed content with void elements -->
<p>
  This is some text<br>
  with a line break<br>
  and another one.
</p>
