<script lang="ts" module>
  type TData = unknown;
  type TValue = unknown;

  export interface Column<T = TValue> {
    key: string;
    label: string;
    format?: (value: T) => string;
  }

  export interface DataTableProps<TPropData = TData, TPropValue = TValue> {
    columns: Column<TPropValue>[];
    data: TPropData[];
  }
</script>

<script generics="TData extends unknown, TValue extends unknown" lang="ts">
  let { columns, data }: DataTableProps<TData, TValue> = $props();
</script>

<table>
  <thead>
    <tr>{#each columns as col}<th>{col.label}</th>{/each}</tr>
  </thead>
  <tbody>
    {#each data as row}
      <tr>{#each columns as col}<td>{String(row)}</td>{/each}</tr>
    {/each}
  </tbody>
</table>
