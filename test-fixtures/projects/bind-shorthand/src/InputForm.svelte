<script lang="ts">
  let value = $state('');
  let checked = $state(false);
</script>

<input bind:value />
<input type="checkbox" bind:checked />
