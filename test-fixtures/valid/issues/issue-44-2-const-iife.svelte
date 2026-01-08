{#if condition}
  {@const computed_value = (() => {
    if (some_check) return valueA
    return valueB
  })()}
{/if}
