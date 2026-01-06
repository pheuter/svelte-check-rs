<script lang="ts">
    // Issue #20: svelte-ignore should only affect the immediately next element
</script>

<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
<div tabindex="0">First div - warning suppressed (line 6)</div>

<!-- This second div should still produce a warning -->
<div tabindex="0">Second div - warning NOT suppressed (line 9)</div>
