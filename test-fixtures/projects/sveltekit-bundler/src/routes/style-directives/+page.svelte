<script lang="ts">
	// Test style directive type errors
	let color: string = 'red';
	let width: number = 100;
	let undefinedVar: undefined;

	// This variable doesn't exist - should error
	// let nonExistent: string;
</script>

<!-- Valid style directives - no errors -->
<div style:color={color}>Valid color</div>
<div style:width={`${width}px`}>Valid width</div>
<div style:--my-var={color}>Valid CSS custom property</div>

<!-- Type error: undefinedVariable doesn't exist - line 17, column 19 -->
<div style:color={undefinedVariable}>Type error: undefined variable</div>

<!-- Type error: wrong type in expression - line 20, column 20 -->
<div style:opacity={width > 100 ? 'invalid' : wrongVar}>Type error: wrong variable</div>

<!-- CSS custom property with type error - line 23, column 26 -->
<div style:--icon-compensate={nonExistentVar === 0 ? null : `${nonExistentVar}px`}>CSS var error</div>
