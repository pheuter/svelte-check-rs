// This file tests issue #35: multiline $state<T>(value) with trailing commas
// See: https://github.com/pheuter/svelte-check-rs/issues/35
//
// When $state<T>(value) spans multiple lines with trailing commas,
// the transformation was producing invalid TypeScript like:
//   eventOrder = (
//       'status-start-title', as 'status-start-title' | 'start-title'
//   );
// instead of:
//   eventOrder = ('status-start-title' as 'status-start-title' | 'start-title');

type Bar = { id: number; name: string };

// Test case 1: Multiline $state with trailing comma (the main issue)
export class PlannerStore {
    eventOrder = $state<'status-start-title' | 'start-title' | 'title' | ((a: Bar, b: Bar) => number)>(
        'status-start-title',
    );
}

// Test case 2: Multiline $state with complex union type and trailing comma
export class ConfigStore {
    sortOrder = $state<'asc' | 'desc' | null>(
        'asc',
    );
}

// Test case 3: Multiline $state with function type and trailing comma
export class CallbackStore {
    handler = $state<(() => void) | null>(
        null,
    );
}

// Test case 4: Multiline $state with array type and trailing comma
export class ListStore {
    items = $state<string[]>(
        [],
    );
}

// Test case 5: Multiline $state with object literal and trailing comma
export class ObjectStore {
    config = $state<{ enabled: boolean; value: number }>(
        { enabled: true, value: 42 },
    );
}

// Test case 6: Nested multiline $state patterns
export class NestedStore {
    data = $state<{
        items: Array<{
            id: number;
            name: string;
        }>;
        count: number;
    }>(
        {
            items: [],
            count: 0,
        },
    );
}

// Test case 7: Multiline with extra whitespace and trailing comma
export class WhitespaceStore {
    value = $state<number>(
        42   ,
    );
}

// Test case 8: Multiline $derived.by with trailing comma (for completeness)
export class DerivedStore {
    count = $state(0);
    doubled = $derived.by<number>(
        () => this.count * 2,
    );
}

// Test case 9: Multiline $state.raw with generic type and trailing comma
export class RawStore {
    items = $state.raw<number[]>(
        [1, 2, 3],
    );
}

// Test case 10: Multiline $derived with generic type and trailing comma
export class DerivedGenericStore {
    count = $state(0);
    doubled = $derived<number>(
        this.count * 2,
    );
}

// Test case 11: $state.raw with complex generic type
export class ComplexRawStore {
    data = $state.raw<Map<string, { id: number; name: string }>>(
        new Map(),
    );
}
