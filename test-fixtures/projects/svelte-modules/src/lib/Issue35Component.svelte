<script lang="ts">
    // Test issue #35: multiline runes with trailing commas in .svelte components
    // This verifies that the fix applies to component scripts, not just .svelte.ts modules

    type SortOrder = 'asc' | 'desc' | 'none';

    // Test case 1: Multiline $state with generic type and trailing comma
    let sortOrder = $state<SortOrder>(
        'asc',
    );

    // Test case 2: Multiline $derived with generic type and trailing comma
    let sortLabel = $derived<string>(
        sortOrder === 'asc' ? 'Ascending' : sortOrder === 'desc' ? 'Descending' : 'None',
    );

    // Test case 3: Multiline $derived.by with trailing comma
    let computedValue = $derived.by<number>(
        () => {
            return sortOrder === 'asc' ? 1 : sortOrder === 'desc' ? -1 : 0;
        },
    );

    function toggleSort() {
        if (sortOrder === 'asc') {
            sortOrder = 'desc';
        } else if (sortOrder === 'desc') {
            sortOrder = 'none';
        } else {
            sortOrder = 'asc';
        }
    }
</script>

<div>
    <p>Sort: {sortLabel} (value: {computedValue})</p>
    <button onclick={toggleSort}>Toggle Sort</button>
</div>
