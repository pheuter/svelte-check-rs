<script lang="ts" generics="T extends { label: string }">
  let { item }: { item: T } = $props()
</script>

<div>{item.label}</div>
