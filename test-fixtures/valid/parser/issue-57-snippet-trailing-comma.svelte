<script lang="ts">
    // Issue #57: {#snippet} with typed destructuring and trailing comma
    interface Props {
        x: number
        y: number
    }
</script>

{#snippet content(
    { x, y }: Props,
)}
    <div>{x}, {y}</div>
{/snippet}

{@render content({ x: 1, y: 2 })}
