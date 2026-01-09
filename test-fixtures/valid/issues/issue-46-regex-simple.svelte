<script lang="ts">
  // Issue #46: Simple regex literal parsing
  let value = $state("test string");
</script>

{#if value.match(/test/)}
  <span>Matches!</span>
{/if}

{value.replace(/test/g, "replaced")}
