<script lang="ts">
    import GenericSelect from "$lib/components/Issue102GenericSelect.svelte";

    // When mode="single", the callback value must be inferred as `string` (not `string | string[]`).
    // If generic inference is broken (e.g. Props<any>), ValueType<any> = string | string[],
    // and calling value.toUpperCase() would produce TS2339.
    function handleChange(value: string): void {
        console.log(value.toUpperCase());
    }
</script>

<GenericSelect mode="single" onchange={handleChange} />
