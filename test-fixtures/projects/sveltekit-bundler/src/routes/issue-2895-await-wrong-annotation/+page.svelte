<script lang="ts">
  // Issue #2895 NEGATIVE / lock-in: a user-supplied `{:then}` type annotation is
  // preserved AND type-checked against the awaited type, not erased. The
  // transform emits `const __value_N: Awaited<typeof __await_N> = await ...;`
  // followed by `const <binding> = __value_N;`, so an annotation that disagrees
  // with the resolved type (here `string` vs the resolved `number`) must surface
  // a genuine TS2322 — proving the annotation is checked, not silently dropped.
  let p: Promise<number> = Promise.resolve(1);
</script>

{#await p}
  <p>loading</p>
{:then v: string}
  <p>{v}</p>
{/await}
