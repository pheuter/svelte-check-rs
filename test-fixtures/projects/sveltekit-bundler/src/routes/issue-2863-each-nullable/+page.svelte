<script lang="ts">
  // Issue #2863: Svelte 5 allows `undefined`/`null` iterables in `{#each}`.
  // A nullable/undefined array must NOT produce false-positive TS18047
  // ("possibly null") / TS18048 ("possibly undefined") diagnostics, and the
  // element type must resolve to the non-nullable element type (number, not
  // number | undefined) so member access on the item type-checks.
  interface OptionObject {
    label: string;
    value: number;
  }

  const maybeUndefined: number[] | undefined | null = null as any;
  const maybeObjs: OptionObject[] | null = null as any;
</script>

<!-- Non-indexed each over a nullable array. `.toFixed(2)` locks in that
     `option` is `number` (NonNullable), not `number | undefined`. -->
{#each maybeUndefined as option}
  <div>{option.toFixed(2)}</div>
{/each}

<!-- Indexed + keyed each over a nullable array. -->
{#each maybeObjs as o, i (i)}
  <div>{o.label} {i}</div>
{/each}
