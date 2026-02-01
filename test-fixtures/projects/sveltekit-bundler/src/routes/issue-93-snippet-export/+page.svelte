<script module lang="ts">
  export type Row = {
    id: string;
    label: string;
  };

  export { rowCell };
</script>

<script lang="ts">
  const row: Row = { id: "1", label: "Row 1" };
</script>

{#snippet rowCell(item: Row)}
  <span>{item.label}</span>
{/snippet}

{@render rowCell(row)}
