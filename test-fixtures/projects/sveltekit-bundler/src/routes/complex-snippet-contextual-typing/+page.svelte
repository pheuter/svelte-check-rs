<script lang="ts">
  import Root from '$lib/complex-snippet/Root.svelte'
</script>

<Root type="single">
  {#snippet children({ months, weekdays })}
    {#each months as month}
      <div>{month.value}</div>
      {#each month.weeks as week}
        {week[0]}
      {/each}
    {/each}
    {weekdays[0].slice(0, 2)}
  {/snippet}
</Root>
