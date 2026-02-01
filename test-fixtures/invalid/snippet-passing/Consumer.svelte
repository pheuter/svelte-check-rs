<script lang="ts">
  import Parent from './Parent.svelte'
</script>

<Parent>
  {#snippet inset()}
    <div>Inset content</div>
  {/snippet}
</Parent>
