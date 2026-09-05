<script lang="ts">
    let { label }: { label: string } = $props();

    export function readLabel(): string {
        return label;
    }
</script>

<button>{label}</button>
