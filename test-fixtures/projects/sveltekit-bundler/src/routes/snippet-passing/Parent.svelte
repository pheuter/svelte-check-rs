<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    children?: Snippet
    inset?: Snippet
  }

  let { children, inset }: Props = $props()
</script>

{#if children}
  {@render children()}
{/if}
{#if inset}
  {@render inset()}
{/if}
