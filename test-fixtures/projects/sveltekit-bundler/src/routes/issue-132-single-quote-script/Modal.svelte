<script lang='ts'>
  import type { Snippet } from 'svelte';

  interface Props {
    isOpen: boolean;
    children: Snippet;
    title?: string;
    onAction?: () => void;
  }

  let {
    isOpen = $bindable(),
    children,
    title,
    onAction
  }: Props = $props();
</script>

{#if isOpen}
  <div>
    {#if title}<h2>{title}</h2>{/if}
    {@render children()}
    {#if onAction}<button onclick={onAction}>Go</button>{/if}
  </div>
{/if}
