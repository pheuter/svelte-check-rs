<!-- Tests for interactive element a11y rules -->
<script>
	function handleClick() {
		console.log('clicked');
	}
	function handleMouseOver() {
		console.log('mouse over');
	}
	function handleMouseOut() {
		console.log('mouse out');
	}
</script>

<!-- a11y-click-events-have-key-events: Click without keyboard handlers -->
<div onclick={handleClick}>Clickable div without keyboard</div>
<span onclick={handleClick}>Clickable span without keyboard</span>
<section onclick={handleClick}>Clickable section without keyboard</section>
<article onclick={handleClick}>Clickable article without keyboard</article>
<main onclick={handleClick}>Clickable main without keyboard</main>
<aside onclick={handleClick}>Clickable aside without keyboard</aside>
<header onclick={handleClick}>Clickable header without keyboard</header>
<footer onclick={handleClick}>Clickable footer without keyboard</footer>
<p onclick={handleClick}>Clickable paragraph without keyboard</p>

<!-- Using on:click directive syntax -->
<div on:click={handleClick}>Directive syntax without keyboard</div>

<!-- a11y-mouse-events-have-key-events: Mouse events without focus equivalents -->
<div onmouseover={handleMouseOver}>Mouse over without focus</div>
<div onmouseout={handleMouseOut}>Mouse out without blur</div>
<div on:mouseover={handleMouseOver}>Directive mouse over without focus</div>
<div on:mouseout={handleMouseOut}>Directive mouse out without blur</div>

<!-- Both mouse events missing their keyboard equivalents -->
<div onmouseover={handleMouseOver} onmouseout={handleMouseOut}>
	Both mouse events without keyboard equivalents
</div>

<!-- a11y-no-noninteractive-tabindex: Tabindex on non-interactive elements -->
<div tabindex="0">Non-interactive with tabindex</div>
<span tabindex="0">Span with tabindex</span>
<p tabindex="0">Paragraph with tabindex</p>
<section tabindex="0">Section with tabindex</section>
<article tabindex="0">Article with tabindex</article>
<h1 tabindex="0">Heading with tabindex</h1>
<li tabindex="0">List item with tabindex</li>

<!-- a11y-positive-tabindex: Positive tabindex values -->
<button tabindex="1">Positive tabindex</button>
<input tabindex="2" />
<a href="/link" tabindex="3">Positive tabindex link</a>
<div tabindex="5">Div with positive tabindex</div>
<span tabindex="99">Very high tabindex</span>

<!-- a11y-no-static-element-interactions: Static elements with click but no role -->
<div onclick={handleClick}>Static div needs role</div>
<span onclick={handleClick}>Static span needs role</span>
<section onclick={handleClick}>Static section needs role</section>
<article onclick={handleClick}>Static article needs role</article>
<main onclick={handleClick}>Static main needs role</main>
<aside onclick={handleClick}>Static aside needs role</aside>
<header onclick={handleClick}>Static header needs role</header>
<footer onclick={handleClick}>Static footer needs role</footer>

<!-- a11y-interactive-supports-focus: Interactive role without tabindex -->
<div role="button" onclick={handleClick}>Interactive role needs tabindex</div>
<span role="link" onclick={handleClick}>Link role needs tabindex</span>
<div role="checkbox" onclick={handleClick}>Checkbox role needs tabindex</div>
<div role="menuitem" onclick={handleClick}>Menuitem role needs tabindex</div>
<div role="tab" onclick={handleClick}>Tab role needs tabindex</div>
<div role="switch" onclick={handleClick}>Switch role needs tabindex</div>
