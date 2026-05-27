<script lang="ts">
  // Issue #150: untyped props destructuring that renames the `class` prop.
  let { children, class: cls = "" } = $props();
</script>

<span class={cls}>
  {@render children?.()}
</span>
