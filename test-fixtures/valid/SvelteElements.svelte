<script>
	let count = $state(0);
	let MyComponent = null;
	const options = {};
</script>

<svelte:head>
	<title>Page Title</title>
	<meta name="description" content="Description"/>
</svelte:head>

<svelte:body on:click={() => count++}/>

<svelte:window on:keydown={(e) => console.log(e.key)} bind:innerWidth={width}/>

<svelte:document on:visibilitychange={() => console.log('visibility changed')}/>

<svelte:element this="div" class="dynamic">
	Dynamic element content
</svelte:element>

<svelte:component this={MyComponent} {...options}/>

<svelte:self count={count - 1}/>

<svelte:options customElement="my-element"/>

<svelte:fragment slot="header">
	<h1>Header content</h1>
</svelte:fragment>
