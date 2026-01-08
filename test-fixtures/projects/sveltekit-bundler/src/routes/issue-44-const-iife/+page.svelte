<script lang="ts">
  let condition = true;
  let some_check = false;
  let valueA = 'A';
  let valueB = 'B';
</script>

{#if condition}
  {@const computed_value = (() => {
    if (some_check) return valueA
    return valueB
  })()}
  <span>{computed_value}</span>
{/if}
