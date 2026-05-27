<script lang="ts">
  // Issue #150: untyped props destructuring that renames the `class` prop to a
  // local alias. The rename must not be read as a type annotation, and the
  // renamed prop must stay destructurable alongside `children`.
  let { children, class: cls = "" } = $props();
</script>

<span class={cls}>
  {@render children?.()}
</span>
