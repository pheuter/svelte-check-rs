// A reusable counter module using Svelte 5 runes
// This is a .svelte.ts file - it can use $state, $derived, $effect
// but NOT $props or $bindable (those are component-only)

export function createCounter(initial: number = 0) {
    let count = $state(initial);
    let doubled = $derived(count * 2);
    let history = $state<number[]>([]);

    $effect(() => {
        // Track count changes in history
        history.push(count);
    });

    return {
        get count() { return count; },
        get doubled() { return doubled; },
        get history() { return $state.snapshot(history); },
        increment() { count++; },
        decrement() { count--; },
        reset() { count = initial; },
    };
}

// Type for the counter return value
export type Counter = ReturnType<typeof createCounter>;
