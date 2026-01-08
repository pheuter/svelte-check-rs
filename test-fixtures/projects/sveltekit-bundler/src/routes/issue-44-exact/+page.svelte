<script lang="ts">
  let key = $state('test');

  function check(val: string): boolean {
    return val.length > 0;
  }
</script>

<input bind:value={key} />

{#snippet example()}
  {@const checker = (val: string): boolean => val.length > 0}
  {@const result = (() => {
    return checker(key) ? 'valid' : 'invalid'
  })()}
  <span>{result}</span>
{/snippet}

{@render example()}
