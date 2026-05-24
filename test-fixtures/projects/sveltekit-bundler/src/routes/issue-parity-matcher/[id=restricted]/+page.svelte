<script lang="ts">
  // No explicit PageProps import: svelte-check-rs auto-imports it from
  // ./$types for SvelteKit route files.  Pulling in the user's matching
  // import here would currently surface a separate TS2300 duplicate-
  // identifier diagnostic (a pre-existing transformer concern not in
  // scope for this fixture).
  let { params }: PageProps = $props();

  // SvelteKit narrows `params.id` to `"alpha" | "beta" | "gamma"` via the
  // matcher's inferred type predicate.  Assigning the narrow value to a
  // matching union must type-check (line 11 below).
  const narrowed: 'alpha' | 'beta' | 'gamma' = params.id;
  void narrowed;

  // Assigning the SAME narrow value to a DIFFERENT literal that doesn't
  // overlap the matcher's union must error with TS2322 (line 16 below).
  const wrong: 'other' = params.id;
  void wrong;
</script>

<p>id: {params.id}</p>
