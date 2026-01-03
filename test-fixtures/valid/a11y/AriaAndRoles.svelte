<!-- Valid a11y: ARIA attributes and roles that should NOT trigger any warnings -->
<script>
	let expanded = $state(false);
	let checked = $state(false);
	let selected = $state(false);
	let value = $state(50);
</script>

<!-- Valid ARIA attributes - should NOT trigger a11y-aria-attributes -->
<div aria-label="Main content"></div>
<div aria-labelledby="heading-1"></div>
<div aria-describedby="description"></div>
<div aria-hidden="true"></div>
<div aria-live="polite"></div>
<div aria-atomic="true"></div>
<div aria-busy="false"></div>
<div aria-controls="panel-1"></div>
<div aria-current="page"></div>
<div aria-disabled="true"></div>
<div aria-expanded={expanded}></div>
<div aria-haspopup="menu"></div>
<div aria-invalid="false"></div>
<div aria-modal="true"></div>
<div aria-multiline="false"></div>
<div aria-multiselectable="true"></div>
<div aria-orientation="horizontal"></div>
<div aria-owns="child-1"></div>
<div aria-placeholder="Enter text..."></div>
<div aria-pressed="false"></div>
<div aria-readonly="true"></div>
<div aria-relevant="additions"></div>
<div aria-required="true"></div>
<div aria-roledescription="slide"></div>
<div aria-selected={selected}></div>
<div aria-sort="ascending"></div>
<div aria-valuemax="100"></div>
<div aria-valuemin="0"></div>
<div aria-valuenow={value}></div>
<div aria-valuetext="50 percent"></div>

<!-- Non-redundant roles - should NOT trigger a11y-no-redundant-roles -->
<div role="button">Div as button</div>
<span role="link">Span as link</span>
<div role="checkbox" aria-checked={checked}>Custom checkbox</div>
<div role="alert">Alert message</div>
<div role="dialog" aria-modal="true">Dialog content</div>
<div role="tablist">
	<div role="tab" aria-selected="true">Tab 1</div>
	<div role="tab" aria-selected="false">Tab 2</div>
</div>
<div role="tabpanel">Panel content</div>

<!-- Roles with required ARIA props provided - should NOT trigger a11y-role-has-required-aria-props -->
<div role="checkbox" aria-checked="false">Unchecked</div>
<div role="checkbox" aria-checked="true">Checked</div>
<div role="checkbox" aria-checked="mixed">Mixed</div>
<div role="checkbox" aria-checked={checked}>Dynamic checked</div>

<div role="radio" aria-checked="false">Radio option</div>
<div role="switch" aria-checked="true">Toggle switch</div>

<div role="slider" aria-valuenow="50" aria-valuemin="0" aria-valuemax="100">Slider</div>
<div role="slider" aria-valuenow={value}>Slider with dynamic value</div>

<div role="spinbutton" aria-valuenow="5" aria-valuemin="0" aria-valuemax="10">Spin button</div>

<div role="meter" aria-valuenow="75" aria-valuemin="0" aria-valuemax="100">Meter</div>

<div role="scrollbar" aria-controls="content" aria-valuenow="0" aria-valuemin="0" aria-valuemax="100">Scrollbar</div>

<div role="combobox" aria-controls="listbox" aria-expanded="false">Combobox</div>
<div role="combobox" aria-controls="options" aria-expanded={expanded}>Dynamic combobox</div>

<div role="option" aria-selected="true">Selected option</div>
<div role="option" aria-selected={selected}>Dynamic option</div>

<div role="heading" aria-level="2">Custom heading</div>
<div role="heading" aria-level="3">Another heading</div>

<!-- aria-hidden on NON-focusable elements - should NOT trigger a11y-hidden -->
<div aria-hidden="true">Hidden content</div>
<span aria-hidden="true">Hidden span</span>
<p aria-hidden="true">Hidden paragraph</p>
<section aria-hidden="true">Hidden section</section>

<!-- aria-hidden="false" on focusable elements (valid) -->
<button aria-hidden="false">Visible button</button>
<a href="/link" aria-hidden="false">Visible link</a>

<!-- Presentation/none roles -->
<ul role="presentation">
	<li role="none">Item styled as non-list</li>
</ul>
<table role="presentation">
	<tr><td>Layout table</td></tr>
</table>
