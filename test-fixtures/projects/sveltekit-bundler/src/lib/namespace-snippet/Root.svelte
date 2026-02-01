<script lang="ts" generics="T extends { id: string; label: string }">
  import type { Snippet } from 'svelte'

  let { items, children }: { items: T[]; children?: Snippet<[{ item: T }]> } = $props()
</script>

{#each items as item (item.id)}
  {#if children}
    {@render children({ item })}
  {/if}
{/each}
