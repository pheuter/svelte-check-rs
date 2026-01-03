<script lang="ts">
    interface Props {
        variant: 'primary' | 'secondary' | 'danger';
        disabled?: boolean;
        onclick?: () => void;
        children?: import('svelte').Snippet;
    }

    let { variant, disabled = false, onclick, children }: Props = $props();
</script>

<button class="btn btn-{variant}" {disabled} {onclick}>
    {#if children}
        {@render children()}
    {/if}
</button>
