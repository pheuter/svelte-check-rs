<script lang="ts">
    import Counter from '$lib/Counter.svelte';
    import { createCounter } from '$lib/counter-state.svelte';
    import { createFullFeatured } from '$lib/valid-runes.svelte';

    // Use the counter module directly
    const counter = createCounter(10);

    // Use the full-featured module
    const featured = createFullFeatured(5);
</script>

<h1>Svelte Module Tests</h1>

<section>
    <h2>Counter Component</h2>
    <Counter />
</section>

<section>
    <h2>Direct Module Usage</h2>
    <p>Count: {counter.count}</p>
    <p>Doubled: {counter.doubled}</p>
    <button onclick={() => counter.increment()}>Increment</button>
</section>

<section>
    <h2>Full Featured Module</h2>
    <p>Count: {featured.count}</p>
    <p>Doubled: {featured.doubled}</p>
    <p>Stats: min={featured.stats.min}, max={featured.stats.max}, avg={featured.stats.avg.toFixed(2)}</p>
    <button onclick={() => featured.increment()}>+</button>
    <button onclick={() => featured.decrement()}>-</button>
</section>
