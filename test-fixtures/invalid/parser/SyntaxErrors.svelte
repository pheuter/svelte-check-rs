<!-- Parser error: SyntaxError - General syntax errors -->
<script>
	let count = $state(0);
</script>

<!-- Unmatched braces in template -->
<p>Hello {world</p>
<p>Count is }count{</p>

<!-- Random closing tags -->
</div>
</span>
</p>

<!-- Invalid comment syntax -->
<!- Single dash comment ->
<!--- Triple dash --->

<!-- Nested comments (invalid in HTML) -->
<!-- outer <!-- inner --> -->

<!-- Invalid special tags -->
{@invalid expression}
{@}
{@const}
{@debug}
{@html}

<!-- Special tags with invalid content -->
{@html}
{@const =}
{@debug ,}

<!-- Malformed render tag -->
{@render}
{@render }
{@render snippet(}

<!-- Script tag issues -->
<script lang=>
	// Empty lang
</script>

<!-- Style tag issues -->
<style lang=>
	/* Empty lang */
</style>

<!-- Multiple script/style without module -->
<script>
	let a = 1;
</script>
<script>
	let b = 2;
</script>

<!-- Incomplete elements -->
<div
<span class="test"
<input type="text"

<!-- Mixed content issues -->
<div>Text <span>nested</div> more</span>

<!-- Invalid self-closing on non-void elements -->
<div />
<span />
<p />

<!-- Actually, self-closing divs are valid in Svelte, let me try other errors -->

<!-- Stray closing brackets -->
<div class="test">Content</div>>
<p>Text<</p>

<!-- Invalid character references -->
<p>&invalid;</p>
<p>&#;</p>
<p>&#x;</p>
<p>&#xZZZ;</p>
