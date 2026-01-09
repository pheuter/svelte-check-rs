<script lang="ts">
  // Issue #46: @const with regex match and complex patterns
  let y_label_full = $state("Energy (eV)");
  let color = $state("rgba(255, 0, 0, 0)");
</script>

{#if y_label_full}
  {@const [, y_label, y_unit] = y_label_full.match(/^(.+?)\s*\(([^)]+)\)$/) ?? [, y_label_full, ""]}
  {@const matches = /rgba\([^)]+\)$/.test(color)}
  <span>{y_label} ({y_unit})</span>
  {#if matches}<span>RGBA match</span>{/if}
{/if}
