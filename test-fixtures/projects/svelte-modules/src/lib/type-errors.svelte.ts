// This file contains intentional type errors to test type checking in .svelte.ts files

export function createTypedState() {
    // Correctly typed state
    let count = $state<number>(0);

    // INTENTIONAL ERROR: Assigning string to number
    count = "not a number";

    // $derived with wrong type
    let doubled = $derived(count * 2);

    return {
        get count() { return count; },
        get doubled() { return doubled; },
    };
}

export function createWrongReturn(): number {
    let value = $state(42);

    // INTENTIONAL ERROR: Returning string when number expected
    return "wrong type";
}
