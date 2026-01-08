{#each data.filter((itm) => itm.value !== undefined) as
  { title, value, unit, fmt = default_fmt, tooltip }
  (title + value + unit + fmt)
}
{/each}
