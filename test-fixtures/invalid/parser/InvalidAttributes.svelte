<!-- Parser error: InvalidAttribute - Malformed attribute syntax -->
<script>
	let value = $state('test');
</script>

<!-- Missing attribute value after equals -->
<input type= />
<div class= style="color: red"></div>

<!-- Unclosed attribute quote -->
<div class="unclosed></div>
<div class='unclosed></div>

<!-- Attribute with no name -->
<div ="value"></div>

<!-- Multiple equals signs -->
<div class=="double"></div>

<!-- Attribute starting with invalid character -->
<div 123class="invalid"></div>
<div -class="invalid"></div>

<!-- Unclosed expression in attribute -->
<div class={value></div>
<div style={`color: ${></div>

<!-- Invalid characters in attribute name -->
<div class name="invalid"></div>
<div data-foo bar="invalid"></div>

<!-- Empty attribute name with value -->
<div ="">empty name</div>

<!-- Attribute with unclosed brackets -->
<div data-items={[1, 2, 3></div>
<div data-obj={{ foo: 'bar'></div>

<!-- Spread with issues -->
<div {...}></div>
<div {...props></div>
<div {... props}></div>
