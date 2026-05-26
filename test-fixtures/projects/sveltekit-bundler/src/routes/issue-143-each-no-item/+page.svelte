<script lang="ts">
  let count = $state(4);
</script>

<button onclick={() => count++}>add</button>

{#each { length: count }, i}
  <div>placeholder {i}</div>
{/each}

{#each { length: 3 }, j}
  <span>{j}</span>
{/each}
