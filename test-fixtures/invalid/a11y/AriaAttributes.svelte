<!-- Tests for ARIA-related a11y rules -->
<script>
	let expanded = $state(false);
</script>

<!-- a11y-aria-attributes: Invalid ARIA attribute names -->
<div aria-foo="bar">Invalid aria attribute</div>
<div aria-notreal="true">Another invalid one</div>
<div aria-customattr="value">Custom aria (invalid)</div>
<button aria-labeledby="typo">Misspelled labelledby</button>
<div aria-role="button">aria-role doesn't exist (use role)</div>

<!-- a11y-no-redundant-roles: Elements with implicit roles -->
<button role="button">Redundant button role</button>
<a href="/link" role="link">Redundant link role</a>
<img src="/photo.jpg" alt="Photo" role="img" />
<nav role="navigation">Redundant nav role</nav>
<main role="main">Redundant main role</main>
<article role="article">Redundant article role</article>
<aside role="complementary">Redundant aside role</aside>
<h1 role="heading">Redundant heading role</h1>
<h2 role="heading">Redundant heading role</h2>
<ul role="list">Redundant list role</ul>
<ol role="list">Redundant list role</ol>
<li role="listitem">Redundant listitem role</li>
<table role="table"><tr role="row"><td role="cell">Redundant table roles</td></tr></table>
<hr role="separator" />
<section role="region">Redundant section role</section>

<!-- a11y-role-has-required-aria-props: Missing required ARIA properties -->
<div role="checkbox">Missing aria-checked</div>
<div role="radio">Missing aria-checked</div>
<div role="switch">Missing aria-checked</div>
<div role="slider">Missing aria-valuenow</div>
<div role="spinbutton">Missing aria-valuenow</div>
<div role="meter">Missing aria-valuenow</div>
<div role="scrollbar">Missing multiple required props</div>
<div role="combobox">Missing aria-controls and aria-expanded</div>
<div role="option">Missing aria-selected</div>
<div role="heading">Missing aria-level</div>
<div role="separator" aria-orientation="vertical">Focusable separator missing value props</div>

<!-- a11y-hidden: aria-hidden on focusable elements -->
<button aria-hidden="true">Hidden focusable button</button>
<a href="/link" aria-hidden="true">Hidden focusable link</a>
<input type="text" aria-hidden="true" />
<select aria-hidden="true"><option>Option</option></select>
<textarea aria-hidden="true"></textarea>
<div tabindex="0" aria-hidden="true">Focusable div hidden</div>
<span tabindex="-1" aria-hidden="true">This is also technically focusable</span>
