<script lang="ts">
    interface Props {
        element: { name: string; number: number };
        show_symbol?: boolean;
        show_number?: boolean;
        onclick?: () => void;
    }

    let { element, show_symbol, show_number, onclick }: Props = $props();
</script>

<div {onclick}>
    <span class="name">{element.name}</span>
    {#if show_symbol}
        <span class="symbol">symbol</span>
    {/if}
    {#if show_number}
        <span class="number">{element.number}</span>
    {/if}
</div>
