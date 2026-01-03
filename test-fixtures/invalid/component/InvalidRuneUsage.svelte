<!-- Component validation: Invalid rune usage in template expressions -->
<!-- Runes should only be used in script blocks, not in template expressions -->
<script>
	// Valid rune usage in script block
	let count = $state(0);
	let doubled = $derived(count * 2);
</script>

<!-- $state() in template - INVALID -->
<p>Count: {$state(0)}</p>
<p>Direct state: {$state('initial')}</p>
<button onclick={() => $state(0)}>State in handler</button>

<!-- $state.raw() in template - INVALID -->
<p>Raw: {$state.raw({ a: 1 })}</p>

<!-- $state.snapshot() in template - INVALID -->
<p>Snapshot: {$state.snapshot(count)}</p>

<!-- $derived() in template - INVALID -->
<p>Derived: {$derived(count * 2)}</p>
<span>{$derived(count + 1)}</span>

<!-- $derived.by() in template - INVALID -->
<p>Derived by: {$derived.by(() => count * 3)}</p>

<!-- $effect() in template - INVALID -->
<div>{$effect(() => console.log('effect'))}</div>

<!-- $effect.pre() in template - INVALID -->
<div>{$effect.pre(() => console.log('pre effect'))}</div>

<!-- $effect.tracking() in template - INVALID -->
<p>Tracking: {$effect.tracking()}</p>

<!-- $effect.root() in template - INVALID -->
<div>{$effect.root(() => {})}</div>

<!-- $props() in template - INVALID -->
<p>Props: {$props()}</p>

<!-- $bindable() in template - INVALID -->
<p>Bindable: {$bindable(0)}</p>
<input value={$bindable('')} />

<!-- $inspect() in template - INVALID -->
<p>Inspect: {$inspect(count)}</p>

<!-- $inspect.trace() in template - INVALID -->
<p>Trace: {$inspect.trace()}</p>

<!-- $host() in template - INVALID -->
<p>Host: {$host()}</p>

<!-- Runes in control flow blocks - INVALID -->
{#if $state(true)}
	<p>Inside if</p>
{/if}

{#each $state([1, 2, 3]) as item}
	<li>{item}</li>
{/each}

{#await $state(Promise.resolve('data'))}
	<p>Loading</p>
{:then value}
	<p>{value}</p>
{/await}

{#key $state('key')}
	<div>Keyed content</div>
{/key}

<!-- Runes in attribute expressions - INVALID -->
<div class={$state('active')}>Class from rune</div>
<input value={$state('')} />
<button disabled={$derived(count > 10)}>Disabled button</button>

<!-- Runes in component props - INVALID -->
<Child prop={$state(0)} />
<Child count={$derived(count * 2)} />

<!-- Nested rune calls - INVALID -->
<p>{$derived($state(0) + 1)}</p>

<!-- Runes in expressions with other operators - INVALID -->
<p>{$state(0) + 1}</p>
<p>{count + $derived(1)}</p>
<p>{$state(true) ? 'yes' : 'no'}</p>

<!-- Runes in template literals - INVALID -->
<p>{`Value: ${$state(0)}`}</p>
