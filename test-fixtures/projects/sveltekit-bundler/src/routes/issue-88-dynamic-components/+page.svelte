<script lang="ts">
  import type { SvelteComponent } from 'svelte'
  import PlotA from './PlotA.svelte'
  import PlotB from './PlotB.svelte'

  const plots = { PlotA, PlotB } as Record<string, typeof SvelteComponent>
</script>

{#each Object.entries(plots) as [name, Plot]}
  <Plot label={name} />
{/each}
