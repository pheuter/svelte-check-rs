<script lang="ts">
  import ChildTable from './ChildTable.svelte';

  let { selectedIds = $bindable() }: { selectedIds: string[] } = $props();
  let inputValue = $state('');
</script>

<ChildTable bind:selectedIds />
<input bind:value={inputValue} />
