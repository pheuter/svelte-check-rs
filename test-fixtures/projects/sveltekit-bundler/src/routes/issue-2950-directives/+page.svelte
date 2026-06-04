<script lang="ts">
	// Issue #2950 (directives): in-tag `// @ts-ignore` comments must suppress the
	// diagnostic on the directive / attach / bind:this they precede — exactly
	// like upstream svelte2tsx 3a3d6e3a.
	import EventComponent from './EventComponent.svelte';

	// `badAction` is NOT an action (it's a number), so `use:badAction` errors.
	const badAction = 42 as unknown as number;

	// `badAttach` is NOT an attachment (it's a number), so `{@attach}` errors.
	const badAttach = 42 as unknown as number;

	// `wrongTypedRef` is typed as `string`, but `bind:this` assigns an
	// HTMLDivElement, so the assignment errors.
	let wrongTypedRef: string = '';

	// `notAHandler` is a number, not an event handler.
	const notAHandler = 42 as unknown as number;
</script>

<!-- Suppressed cases: each has a leading `// @ts-ignore` INSIDE the tag, so the
     otherwise-emitted TS error on that directive must be suppressed. -->
<div
	// @ts-ignore
	use:badAction
></div>

<div
	// @ts-ignore
	{@attach badAttach}
></div>

<div
	// @ts-ignore
	bind:this={wrongTypedRef}
></div>

<!-- Component event handler with a leading `// @ts-ignore` (exercises the
     component second-pass directive comment path). -->
<EventComponent
	// @ts-ignore
	on:select={notAHandler}
/>

<!-- Control cases: identical mismatch WITHOUT the comment must still error so
     suppression is targeted, not blanket. Kept on contiguous lines so each
     error maps to a predictable line. -->
<div use:badAction></div>
<div {@attach badAttach}></div>
<div bind:this={wrongTypedRef}></div>
