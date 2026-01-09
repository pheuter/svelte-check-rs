<!-- Issue #52: {#each} with destructuring defaults spanning multiple lines -->
<script lang="ts">
  interface DataItem {
    title: string
    value?: string | number | null
    fmt?: string
  }

  let data: DataItem[] = [
    { title: `Energy`, value: 1.23, fmt: `.3f` },
    { title: `Force`, value: 0.01 },
  ]

  let default_fmt = `.2f`
</script>

<!-- Multi-line filter + destructuring with default values -->
<section>
  {#each data.filter((itm) =>
      itm.value !== undefined && itm.value !== null
    ) as
    { title, value, fmt = default_fmt }
    (title + value)
  }
    <div>{title}: {value}</div>
  {:else}
    No data
  {/each}
</section>

<!-- Simpler single-line version -->
<ul>
  {#each data.filter((d) => d.value != null) as { title, value, fmt = `.2f` } (title)}
    <li>{title}: {value}</li>
  {/each}
</ul>
