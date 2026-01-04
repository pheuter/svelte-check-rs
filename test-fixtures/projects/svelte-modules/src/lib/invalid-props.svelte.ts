// This file INCORRECTLY uses $props in a module file
// $props is only valid in .svelte component files, not .svelte.ts modules
// This should generate an error!

export function createInvalidModule() {
    // ERROR: $props is not valid in .svelte.ts files
    let { name } = $props<{ name: string }>();

    return {
        get name() { return name; }
    };
}
