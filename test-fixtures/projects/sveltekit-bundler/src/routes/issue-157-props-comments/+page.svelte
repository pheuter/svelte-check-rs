<script lang="ts">
  import CommentedProps from './CommentedProps.svelte'

  let mapping = $state<Record<string, string> | undefined>(undefined)
  let cell = $state(`original`)
  let hidden = $state(false)
</script>

<CommentedProps bind:element_mapping={mapping} bind:cell_type={cell} bind:hidden label="ok" />
