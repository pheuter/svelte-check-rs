<script lang="ts">
  // Bindable prop typed as `string | undefined`.  Binding a `string | null`
  // expression to this should error because `null` is not assignable to
  // `undefined`.
  let { value = $bindable() }: { value?: string } = $props();
</script>

<input bind:value />
