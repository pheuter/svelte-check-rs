<script lang="ts">
  // Child with a typed prop, mounted programmatically in mount-loosen.svelte.ts.
  let { label }: { label: string } = $props();
</script>

<span>{label}</span>
