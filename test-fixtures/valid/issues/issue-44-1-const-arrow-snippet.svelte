{#if condition}
  {@const is_valid = (value: string | null): boolean =>
    !value || value === `none`}
{/if}
