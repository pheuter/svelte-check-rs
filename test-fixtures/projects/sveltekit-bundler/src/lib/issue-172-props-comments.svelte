<script lang="ts">
    let {
        // A user's option has no closing quote or brace in this comment.
        label = 'ready',
        ...rest
    }: { label?: string; value?: number } = $props();
    const invalid: number = $derived(label);
</script>
<p>{invalid} {rest.value}</p>
