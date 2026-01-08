<script lang="ts">
  // Test file for issues #36, #37, #38
  let value = 'test';
</script>

<!-- Issue #38: Void elements -->
<div>
  line 1<br>
  line 2<hr>
  <img src="test.png" alt="test">
  <input type="text" bind:value>
</div>

<!-- Issue #37: CSS custom properties - commenting out as it needs a real component -->
<!-- <MyComponent --primary-color="red" /> -->

<!-- Issue #36: Dot notation - commenting out as it needs threlte -->
<!-- <T.Mesh rotation.x={Math.PI / 2}></T.Mesh> -->
