<script lang="ts">
  let { children } = $props();
</script>

<div class="wrapper">
  {@render children?.()}
</div>
