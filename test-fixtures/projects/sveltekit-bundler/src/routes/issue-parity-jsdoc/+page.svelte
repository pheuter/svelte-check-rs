<script>
  // Consumes `data` so `svelte-kit sync` generates this route's $types,
  // which the JSDoc `@param {PageLoadEvent}` injection in +page.js resolves
  // against.
  let { data } = $props();
</script>

<p>missing: {data.missing}</p>
