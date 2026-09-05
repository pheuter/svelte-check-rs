<script lang="ts" generics="const T extends { id: string } = { id: string; count: number }">
    let { item }: { item: T } = $props();

    export function current(): T {
        return item;
    }
</script>

<span>{item.id}</span>
