<script lang="ts">
    import { createCounter } from './counter-state.svelte';

    // Use the counter module
    const counter = createCounter(0);

    // This should work - accessing valid properties
    let currentCount: number = counter.count;
    let currentDoubled: number = counter.doubled;

    // INTENTIONAL ERROR: 'tripled' does not exist on Counter
    // @ts-expect-error - This should generate a type error
    let wrongProperty = counter.tripled;
</script>

<div>
    <p>Count: {counter.count}</p>
    <p>Doubled: {counter.doubled}</p>
    <button onclick={() => counter.increment()}>+</button>
    <button onclick={() => counter.decrement()}>-</button>
    <button onclick={() => counter.reset()}>Reset</button>
</div>
