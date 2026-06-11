<script lang="ts">
  let {
    // Element remapping: maps original element symbols to new ones
    element_mapping = $bindable(),
    // atom types are mapped to H, He, Li by default
    cell_type = $bindable(`original`),
    /* hidden: yes, always */
    hidden = $bindable(false),
    label = `legend`,
  }: {
    element_mapping?: Record<string, string>
    cell_type?: string
    hidden?: boolean
    label?: string
  } = $props()
</script>

<div>
  {label}
  {cell_type}
  {hidden}
  {JSON.stringify(element_mapping ?? {})}
</div>
