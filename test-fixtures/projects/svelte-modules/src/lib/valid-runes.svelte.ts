// This file tests all valid runes in .svelte.ts module files
// These should all transform correctly without errors

// Basic $state
export function createSimpleState() {
    let value = $state(0);
    return {
        get value() { return value; },
        set value(v: number) { value = v; },
    };
}

// $state with generic type
export function createTypedState<T>(initial: T) {
    let value = $state<T>(initial);
    return {
        get value() { return value; },
        set value(v: T) { value = v; },
    };
}

// $state.raw for non-reactive state
export function createRawState() {
    let items = $state.raw([1, 2, 3]);
    return {
        get items() { return items; },
        setItems(newItems: number[]) { items = newItems; },
    };
}

// $derived
export function createDerived() {
    let count = $state(0);
    let doubled = $derived(count * 2);
    let tripled = $derived(count * 3);

    return {
        get count() { return count; },
        get doubled() { return doubled; },
        get tripled() { return tripled; },
        increment() { count++; },
    };
}

// $derived.by for complex computations
export function createComplexDerived() {
    let items = $state<number[]>([1, 2, 3, 4, 5]);

    let sum = $derived.by(() => {
        return items.reduce((a, b) => a + b, 0);
    });

    let average = $derived.by(() => {
        return items.length > 0 ? sum / items.length : 0;
    });

    return {
        get items() { return items; },
        get sum() { return sum; },
        get average() { return average; },
        addItem(n: number) { items = [...items, n]; },
    };
}

// $effect for side effects
export function createLogger() {
    let count = $state(0);
    let logs = $state<string[]>([]);

    $effect(() => {
        logs.push(`Count changed to: ${count}`);
    });

    return {
        get count() { return count; },
        get logs() { return $state.snapshot(logs); },
        increment() { count++; },
    };
}

// $effect.pre for pre-update effects
export function createPreEffect() {
    let value = $state(0);
    let previousValue = $state<number | null>(null);

    $effect.pre(() => {
        previousValue = value;
    });

    return {
        get value() { return value; },
        get previousValue() { return previousValue; },
        setValue(v: number) { value = v; },
    };
}

// $effect.root for untracked effects
export function createRootEffect() {
    let count = $state(0);

    const cleanup = $effect.root(() => {
        console.log('Root effect running');
        return () => {
            console.log('Root effect cleanup');
        };
    });

    return {
        get count() { return count; },
        increment() { count++; },
        cleanup,
    };
}

// $state.snapshot for getting non-reactive copies
export function createSnapshotable() {
    let data = $state({ name: 'test', value: 42 });

    return {
        get data() { return data; },
        getSnapshot() { return $state.snapshot(data); },
        update(name: string, value: number) {
            data = { name, value };
        },
    };
}

// $inspect for debugging (transforms to void 0)
export function createInspectable() {
    let count = $state(0);

    $inspect(count);

    return {
        get count() { return count; },
        increment() { count++; },
    };
}

// Combined usage
export function createFullFeatured(initial: number = 0) {
    let count = $state(initial);
    let history = $state<number[]>([initial]);
    let doubled = $derived(count * 2);

    let stats = $derived.by(() => ({
        min: Math.min(...history),
        max: Math.max(...history),
        avg: history.reduce((a, b) => a + b, 0) / history.length,
    }));

    $effect(() => {
        if (!history.includes(count)) {
            history = [...history, count];
        }
    });

    $inspect({ count, doubled, history });

    return {
        get count() { return count; },
        get doubled() { return doubled; },
        get history() { return $state.snapshot(history); },
        get stats() { return stats; },
        increment() { count++; },
        decrement() { count--; },
        reset() { count = initial; },
    };
}
