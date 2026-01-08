<script lang="ts">
  let condition = true;
  let value: string | null = 'test';
</script>

{#if condition}
  {@const is_valid = (value: string | null): boolean =>
    !value || value === `none`}
  <span>Valid: {is_valid(value)}</span>
{/if}
