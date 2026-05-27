<script lang="ts">
  // Issue #151, pattern A: a dollar-prefixed property access (ProseMirror's
  // selection.$from / selection.$to) must not be treated as a store
  // subscription. The import...from keyword also makes the bare name
  // referenceable, so only the member-access rule prevents a bogus store alias
  // that references a non-existent name (TS2552/TS2304).
  import type { Snippet } from "svelte";

  interface ResolvedPos {
    pos: number;
    parent: { textContent: string };
  }
  interface Selection {
    $from: ResolvedPos;
    $to: ResolvedPos;
  }

  let { selection, children }: { selection: Selection; children?: Snippet } =
    $props();

  function getFromText(): string {
    return selection.$from.parent.textContent;
  }
</script>

<p>{getFromText()}</p>
<p>{selection.$from.pos}</p>
<p>{selection.$to.pos}</p>
{@render children?.()}
