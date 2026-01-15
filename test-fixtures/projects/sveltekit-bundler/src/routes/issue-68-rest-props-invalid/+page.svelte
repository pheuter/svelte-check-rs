<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements'

  let { element, missing_msg = `No image`, ...rest }:
    & HTMLAttributes<HTMLImageElement | HTMLDivElement>
    & { element: { name: string }; missing_msg?: string } = $props()

  const bad: number = rest;
</script>

<div {...rest}>
  {element.name}
</div>
