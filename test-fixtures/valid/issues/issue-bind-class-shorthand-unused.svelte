<script lang="ts">
  // Regression: shorthand `bind:foo` / `class:foo` should be treated as a read
  // of the local variable (sugar for `bind:foo={foo}` / `class:foo={foo}`).
  // With strict `noUnusedLocals` enabled, these locals should NOT be flagged
  // as `TS6133: declared but never read`.
  let checked = $state(false);
  let active = $state(true);
</script>

<input type="checkbox" bind:checked />
<div class:active>Toggle me</div>
