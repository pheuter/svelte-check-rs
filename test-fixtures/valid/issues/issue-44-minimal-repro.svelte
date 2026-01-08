<script lang="ts">
  let key = $state(`test`)
</script>

<input bind:value={key} />

{#snippet example()}
  {@const check = (val: string): boolean => val.length > 0}
  {@const result = (() => {
    return check(key) ? `valid` : `invalid`
  })()}
  <span>{result}</span>
{/snippet}

{@render example()}
