<script lang="ts">
  import type { Snippet } from 'svelte'

  let { children, inset }: { children?: Snippet; inset?: Snippet } = $props()
</script>

{#if children}
  {@render children()}
{/if}
{#if inset}
  {@render inset()}
{/if}
