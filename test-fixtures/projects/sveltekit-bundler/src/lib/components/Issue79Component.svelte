<script lang="ts">
    interface Props {
        count?: number;
        name?: string;
    }

    let { count = $bindable(0), name = $bindable("World") }: Props = $props();

    export { count, name };
</script>

<button>
    Hello {name}! Count: {count}
</button>
