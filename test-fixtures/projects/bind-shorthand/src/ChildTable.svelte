<script lang="ts">
  let { selectedIds = $bindable() }: { selectedIds: string[] } = $props();
</script>

<div>{selectedIds.length}</div>
