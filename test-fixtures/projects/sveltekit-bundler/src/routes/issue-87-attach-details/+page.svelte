<script lang="ts">
  import type { Attachment } from 'svelte/attachments'

  function click_outside<T extends HTMLElement>(opts: { callback: (node: T) => void }): Attachment<T> {
    return (node: T) => {
      opts.callback(node)
    }
  }
</script>

<details {@attach click_outside({ callback: (node) => (node.open = false) })}>
  <summary>Click me</summary>
  <p>Content</p>
</details>
