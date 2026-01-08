<script lang="ts">
  let data = [
    { title: 'Item 1', value: 10, unit: 'px' },
    { title: 'Item 2', value: 20, unit: 'em' }
  ];

  function default_fmt(v: number) {
    return v.toString();
  }
</script>

{#each data.filter((itm) => itm.value !== undefined) as { title, value, unit, fmt = default_fmt } (title + value + unit)}
  <div>{title}: {fmt(value)}{unit}</div>
{/each}
