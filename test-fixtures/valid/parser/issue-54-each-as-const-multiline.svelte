<!-- Issue #54: {#each ... as const as [destructuring with defaults]} on multiple lines -->
<script lang="ts">
  // Test that "as const" with destructuring defaults works on multiple lines
</script>

<!-- PASSES: as const on same line without defaults -->
<div id="pass-1">
  {#each [`pie`, `bubble`] as const as mode (mode)}
    <span>{mode}</span>
  {/each}
</div>

<!-- PASSES: as const with tuple destructuring (no defaults) on same line -->
<div id="pass-2">
  {#each [[`a`, 1], [`b`, 2]] as const as [name, val] (name)}
    <span>{name}: {val}</span>
  {/each}
</div>

<!-- Test: as const with destructuring default value on same line -->
<div id="test-1">
  {#each [[`a`, 1], [`b`, 2, { x: 1 }]] as const as [name, val, opts = {}] (name)}
    <span>{name}: {val}</span>
  {/each}
</div>

<!-- Test: Multi-line as const with destructuring default -->
<div id="test-2">
  {#each [
    [`Pie`, `pie`],
    [`Donut`, `pie`, { radius: 40 }],
  ] as const as
    [title, mode, props = {}]
    (title)
  }
    <span>{title}: {mode}</span>
  {/each}
</div>
