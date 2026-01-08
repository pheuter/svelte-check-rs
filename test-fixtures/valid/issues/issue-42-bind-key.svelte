<script lang="ts">
  let customKey = $state('default');
  let customValue = $state(false);
</script>

<FieldCheckbox
  bind:value={customValue}
  bind:key={customKey}
/>
