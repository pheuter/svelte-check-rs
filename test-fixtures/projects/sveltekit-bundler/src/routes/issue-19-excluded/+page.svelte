<script lang="ts">
    // Issue #19: Files in tsconfig exclude patterns should not be checked
</script>

<!-- This has a11y issues but is in excluded directory -->
<div tabindex="0">Should not produce warnings if excluded (line 6)</div>
<img src="test.png" alt="test" />
