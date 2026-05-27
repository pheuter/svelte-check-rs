<script lang="ts">
  // Issue #151: `$`-prefixed property access and callback params are not stores.
  import { writable } from "svelte/store";

  interface Selection {
    $from: { pos: number };
  }

  const form = writable({ name: "" });
  let { selection }: { selection: Selection } = $props();

  const items = [1, 2, 3];
  const mapped = items.map(($item) => $item * 2);

  function update() {
    form.update(($form) => ({ ...$form, name: "x" }));
  }
</script>

<button onclick={update}>{selection.$from.pos} / {mapped.join(",")}</button>
