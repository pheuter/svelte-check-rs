<script lang="ts">
  const items = [{ id: "a", label: "A" }];
</script>

{#snippet itemCell(item: (typeof items)[number])}
  {item.label}
{/snippet}

{@render itemCell(items[0])}
