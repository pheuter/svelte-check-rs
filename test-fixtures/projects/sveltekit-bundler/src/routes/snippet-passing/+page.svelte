<script lang="ts">
  import Parent from './Parent.svelte'
</script>

<!-- Test 1: Only named snippet (from issue) -->
<Parent>
  {#snippet inset()}
    <div>Inset content</div>
  {/snippet}
</Parent>

<!-- Test 2: Children only -->
<Parent>
  <div>Default children content</div>
</Parent>

<!-- Test 3: Both children and named snippet -->
<Parent>
  <div>Children before snippet</div>
  {#snippet inset()}
    <div>Named snippet content</div>
  {/snippet}
  <div>Children after snippet</div>
</Parent>
