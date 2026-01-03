<!-- Valid a11y: Interactive elements that should NOT trigger any warnings -->
<script>
	function handleClick() { console.log('clicked'); }
	function handleKeyDown(e) { if (e.key === 'Enter') handleClick(); }
	function handleMouseOver() { console.log('hover'); }
	function handleMouseOut() { console.log('leave'); }
	function handleFocus() { console.log('focus'); }
	function handleBlur() { console.log('blur'); }
</script>

<!-- Native interactive elements with click - should NOT trigger any warnings -->
<button onclick={handleClick}>Click me</button>
<a href="/link" onclick={handleClick}>Link with click</a>
<input type="button" onclick={handleClick} value="Input button" />
<input type="submit" onclick={handleClick} value="Submit" />
<select onchange={handleClick}><option>Option</option></select>

<!-- Click WITH keyboard handler - should NOT trigger a11y-click-events-have-key-events -->
<div onclick={handleClick} onkeydown={handleKeyDown} tabindex="0" role="button">
	Div with click and keyboard
</div>
<span onclick={handleClick} onkeyup={handleKeyDown} tabindex="0" role="button">
	Span with click and keyup
</span>
<div onclick={handleClick} onkeypress={handleKeyDown} tabindex="0" role="button">
	Div with keypress
</div>

<!-- Using directive syntax -->
<div on:click={handleClick} on:keydown={handleKeyDown} tabindex="0" role="button">
	Directive syntax
</div>

<!-- Mouse events WITH focus equivalents - should NOT trigger a11y-mouse-events-have-key-events -->
<!-- Using buttons since they're natively focusable and interactive -->
<button onmouseover={handleMouseOver} onfocus={handleFocus}>
	Mouseover with focus
</button>
<button onmouseout={handleMouseOut} onblur={handleBlur}>
	Mouseout with blur
</button>
<button
	onmouseover={handleMouseOver}
	onmouseout={handleMouseOut}
	onfocus={handleFocus}
	onblur={handleBlur}
>
	All mouse and focus events
</button>

<!-- Directive syntax for mouse/focus -->
<button on:mouseover={handleMouseOver} on:focus={handleFocus}>
	Directive mouseover with focus
</button>
<button on:mouseout={handleMouseOut} on:blur={handleBlur}>
	Directive mouseout with blur
</button>

<!-- Valid tabindex values - should NOT trigger a11y-positive-tabindex -->
<button tabindex="0">Normal tabindex</button>
<a href="/link" tabindex="0">Link with tabindex 0</a>
<input tabindex="0" />
<div tabindex="0" role="button">Focusable div</div>
<button tabindex="-1">Programmatically focusable</button>
<a href="/hidden" tabindex="-1">Hidden from tab order</a>

<!-- Interactive elements with tabindex - should NOT trigger a11y-no-noninteractive-tabindex -->
<button tabindex="0">Button</button>
<a href="/link" tabindex="0">Link</a>
<input type="text" tabindex="0" />
<select tabindex="0"><option>Option</option></select>
<textarea tabindex="0"></textarea>

<!-- Non-interactive with tabindex=-1 (valid for programmatic focus) -->
<div tabindex="-1">Programmatically focusable div</div>
<p tabindex="-1">Programmatically focusable paragraph</p>

<!-- Non-interactive with interactive role AND tabindex - should NOT trigger -->
<div role="button" tabindex="0" onclick={handleClick}>Custom button</div>
<span role="link" tabindex="0" onclick={handleClick}>Custom link</span>
<div role="checkbox" tabindex="0" aria-checked="false">Custom checkbox</div>
<div role="menuitem" tabindex="0">Menu item</div>
<div role="tab" tabindex="0" aria-selected="true">Tab</div>

<!-- Static elements WITH proper role - should NOT trigger a11y-no-static-element-interactions -->
<div role="button" onclick={handleClick} tabindex="0">Div button</div>
<span role="link" onclick={handleClick} tabindex="0">Span link</span>
<section role="button" onclick={handleClick} tabindex="0">Section button</section>
<article role="link" onclick={handleClick} tabindex="0">Article link</article>

<!-- Interactive role WITH tabindex - should NOT trigger a11y-interactive-supports-focus -->
<div role="button" tabindex="0" onclick={handleClick}>Focusable button</div>
<div role="link" tabindex="0" onclick={handleClick}>Focusable link</div>
<div role="checkbox" tabindex="0" aria-checked="false">Focusable checkbox</div>
<div role="menuitem" tabindex="-1">Programmatically focusable menu item</div>
<div role="tab" tabindex="0" aria-selected="false">Focusable tab</div>
<div role="switch" tabindex="0" aria-checked="false">Focusable switch</div>

<!-- Native interactive elements (inherently focusable) -->
<button role="switch" aria-checked="true">Native button as switch</button>
<input type="checkbox" role="switch" aria-checked="false" />
