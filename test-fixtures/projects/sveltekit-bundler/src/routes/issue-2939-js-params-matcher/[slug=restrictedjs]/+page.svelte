<script lang="ts">
  // Consumer of the `.js` param matcher in `src/params/restrictedjs.js`.
  // No explicit PageProps import: svelte-check-rs auto-imports it from
  // ./$types for SvelteKit route files (importing it here would surface a
  // separate TS2300 duplicate-identifier diagnostic, out of scope here).
  let { params }: PageProps = $props();

  // SvelteKit narrows `params.slug` to `"js-a" | "js-b"` via the matcher's
  // inferred type predicate — which must survive the JSDoc `@satisfies`
  // cast the `.js` transform emits.  Assigning the narrow value to the
  // matching union must type-check (line 13 below).
  const narrowed: 'js-a' | 'js-b' = params.slug;
  void narrowed;

  // Assigning the SAME narrow value to a NON-overlapping literal must error
  // with TS2322 (line 18 below).  This proves the predicate is preserved
  // through the JSDoc cast (not silently widened to `string`), making the
  // no-TS8010 assertion non-vacuous.
  const wrong: 'nope' = params.slug;
  void wrong;
</script>

<p>slug: {params.slug}</p>
