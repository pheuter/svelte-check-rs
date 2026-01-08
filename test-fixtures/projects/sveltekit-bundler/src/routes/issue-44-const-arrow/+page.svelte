<script lang="ts">
  let condition = true;
  let value: string | null = 'test';
</script>

{#if condition}
  {@const is_valid = (val: string | null): boolean =>
    !val || val === 'none'}
  <span>Valid: {is_valid(value)}</span>
{/if}
