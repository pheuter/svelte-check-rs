<script>
	let value = $state('');
	let checked = $state(false);
	let visible = $state(true);
	let theme = $state('light');

	function handleClick(e) {
		console.log('clicked', e);
	}

	function handleSubmit() {
		console.log('submitted');
	}

	function action(node, param) {
		return { destroy() {} };
	}

	function fade(node, params) {
		return { duration: params.duration || 200 };
	}
</script>

<!-- Event handlers -->
<button on:click={handleClick}>Click me</button>
<button on:click|preventDefault|stopPropagation={handleSubmit}>Submit</button>
<button on:click|once|capture={handleClick}>Once</button>
<button on:click|self|trusted={handleClick}>Self</button>
<form on:submit|preventDefault={handleSubmit}>
	<input type="submit"/>
</form>

<!-- Bindings -->
<input bind:value/>
<input type="checkbox" bind:checked/>
<select bind:value={theme}>
	<option value="light">Light</option>
	<option value="dark">Dark</option>
</select>
<div bind:clientWidth={width} bind:clientHeight={height}></div>

<!-- Class directives -->
<div class:active={isActive} class:highlight>Styled</div>
<div class:dark={theme === 'dark'}>Theme aware</div>

<!-- Style directives -->
<div style:color="red">Styled inline</div>
<div style:opacity="0.5">Opacity</div>

<!-- Actions -->
<div use:action>With action</div>
<div use:action={params}>With params</div>
<div use:action={{ key: 'value' }}>With object</div>

<!-- Transitions -->
<div transition:fade>Fade in/out</div>
<div transition:fade={{ duration: 500 }}>Slow fade</div>
<div in:fade out:fade={{ delay: 100 }}>Separate</div>
<div in:fade|local out:fade|local>Local</div>

<!-- Animate -->
<div animate:flip={{ duration: 200 }}>Animated</div>

<!-- Let -->
<Slider let:value>
	Current value: {value}
</Slider>
