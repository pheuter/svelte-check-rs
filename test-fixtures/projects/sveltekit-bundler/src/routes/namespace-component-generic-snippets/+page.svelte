<script lang="ts">
  import * as List from '$lib/namespace-snippet'

  type Row = { id: string; label: string; extra: number }
  const rows: Row[] = [{ id: 'one', label: 'One', extra: 1 }]
</script>

<List.Root items={rows}>
  {#snippet children({ item })}
    <List.Item item={item} />
    {item.extra + 1}
  {/snippet}
</List.Root>
