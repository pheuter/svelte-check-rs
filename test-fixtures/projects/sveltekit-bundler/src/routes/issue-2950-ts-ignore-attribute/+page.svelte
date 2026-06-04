<script lang="ts">
	// Issue #2950: in-tag `// @ts-ignore` comments must suppress the
	// diagnostic on the attribute they precede.
	let x = true;
</script>

<!-- Suppressed: `dir` expects a string union, `x` is a boolean.
     The `// @ts-ignore` inside the tag must suppress TS2322. -->
<div
	// @ts-ignore
	dir={x}
></div>

<!-- Control: identical mismatch WITHOUT the comment must still error so
     suppression is targeted, not blanket. -->
<div dir={x}></div>
