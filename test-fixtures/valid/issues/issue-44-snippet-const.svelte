<script lang="ts">
  let key = $state('test');

  function check(val: string): boolean {
    return val.length > 0;
  }
</script>

<input bind:value={key} />

{#snippet example()}
  {@const result = check(key) ? 'valid' : 'invalid'}
  <span>{result}</span>
{/snippet}

{@render example()}
