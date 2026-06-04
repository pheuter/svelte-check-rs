<script lang="ts">
  // Regression for the #2863 `__svelte_each` helper: an `any`-typed iterable
  // must yield `any` items (like a native `for...of`), NOT `unknown`.
  //
  // Real-world trigger: a `sanity.fetch()`-style call returns `any`, and the
  // resulting array is iterated in a non-indexed `{#each}`. The helper used to
  // resolve the element to `unknown`, producing false-positive
  // `TS18046 'item' is of type 'unknown'` on member access. Caught only when
  // run against the careswitch monorepo; this fixture pins the behavior.
  let { items }: { items: any } = $props();
</script>

{#each items as item}
  <div>{item.foo.bar}</div>
{/each}
