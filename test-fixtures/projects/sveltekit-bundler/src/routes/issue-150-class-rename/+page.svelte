<script lang="ts">
  // Consumer side of issue #150: passing `class` and children to a component
  // whose props were inferred from an untyped `$props()` rename must type-check.
  import ClassRename from "$lib/issue-150-class-rename.svelte";
</script>

<ClassRename class="test">Hello</ClassRename>
