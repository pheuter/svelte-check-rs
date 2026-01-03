<!-- A11y violations with dynamic/expression-based values -->
<!-- These test that diagnostics work with Svelte's reactive features -->
<script>
	let imageSrc = $state('/photo.jpg');
	let videoSrc = $state('/video.mp4');
	let linkHref = $state('/about');
	let isActive = $state(true);
	let tabValue = $state(5);
	let items = $state([1, 2, 3]);
	let role = $state('button');

	function handleClick() {}
	function handleMouseOver() {}
</script>

<!-- Dynamic src without alt - should trigger a11y-missing-attribute -->
<img src={imageSrc} />
<img src={`/images/${imageSrc}`} />
<img src={isActive ? '/active.png' : '/inactive.png'} />

<!-- Dynamic href with empty anchor - should trigger a11y-missing-content -->
<a href={linkHref}></a>
<a href={`/page/${items[0]}`}></a>
<a href={isActive ? '/active' : '/inactive'}></a>

<!-- Dynamic tabindex with positive value - should trigger a11y-positive-tabindex -->
<div tabindex={tabValue}>Positive tabindex from variable</div>
<div tabindex={isActive ? 5 : 0}>Conditional positive tabindex</div>
<button tabindex={1 + 1}>Expression positive tabindex</button>

<!-- Dynamic role without required props - should trigger a11y-role-has-required-aria-props -->
<div role={role}>Dynamic role missing props</div>
<div role={isActive ? 'checkbox' : 'button'}>Conditional checkbox without aria-checked</div>
<div role={`${role}`}>Template literal role</div>

<!-- Images in loops without alt -->
{#each items as item}
	<img src={`/image-${item}.jpg`} />
{/each}

<!-- Links in loops without content -->
{#each items as item}
	<a href={`/page/${item}`}></a>
{/each}

<!-- Conditional rendering with a11y issues -->
{#if isActive}
	<img src="/active.png" />
	<video src={videoSrc}></video>
{:else}
	<img src="/inactive.png" />
	<marquee>Scrolling when inactive</marquee>
{/if}

<!-- Dynamic event handlers without keyboard equivalents -->
<div onclick={() => handleClick()}>Arrow function click</div>
<div onclick={isActive ? handleClick : null}>Conditional click</div>
<div onclick={handleClick} class={isActive ? 'active' : ''}>Click with dynamic class</div>

<!-- Dynamic mouse events without focus equivalents -->
<div onmouseover={() => handleMouseOver()}>Dynamic mouseover</div>
<div onmouseout={() => console.log('out')}>Dynamic mouseout</div>

<!-- Dynamic autofocus (should still trigger) -->
<input autofocus={isActive} />
<input autofocus={true} />

<!-- Dynamic accesskey -->
<button accesskey={isActive ? 's' : 'x'}>Dynamic accesskey</button>

<!-- Spread attributes that might have issues -->
<img {...{ src: imageSrc }} />
<a {...{ href: linkHref }}></a>

<!-- Complex expressions in attributes -->
<div tabindex={Math.max(1, tabValue)}>Complex expression tabindex</div>
<img src={items.map(i => `/img-${i}`)[0]} />

<!-- Snippets with a11y issues -->
{#snippet imageGallery(images)}
	{#each images as img}
		<img src={img} />
	{/each}
{/snippet}

{@render imageGallery(items.map(i => `/photo-${i}.jpg`))}

<!-- Await blocks with a11y issues -->
{#await Promise.resolve('/photo.jpg')}
	<p>Loading...</p>
{:then src}
	<img {src} />
{/await}

<!-- Key blocks with a11y issues -->
{#key imageSrc}
	<img src={imageSrc} />
{/key}
