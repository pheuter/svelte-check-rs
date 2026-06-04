<script lang="ts">
  // Issue #2863 NEGATIVE / lock-in: loosening `{#each}` to accept
  // `null`/`undefined` (upstream 7468286a) must NOT swallow a genuine
  // non-iterable. The `__svelte_each`/`__svelte_each_indexed` helper constraint
  // (`T extends ArrayLike<unknown> | Iterable<unknown> | null | undefined`)
  // still rejects a plain `number`, so each block below must surface TS2345.
  const n: number = 1;
</script>

<!-- Non-indexed each over a `number` routes through `__svelte_each(...)` and
     must still error TS2345 ("not assignable to parameter of type
     'ArrayLike<unknown> | Iterable<unknown> | null | undefined'"). -->
{#each n as x}
  <div>{x}</div>
{/each}

<!-- Indexed + keyed each over a `number` routes through
     `__svelte_each_indexed(...)` and must also still error TS2345. -->
{#each n as y, i (i)}
  <div>{y} {i}</div>
{/each}
