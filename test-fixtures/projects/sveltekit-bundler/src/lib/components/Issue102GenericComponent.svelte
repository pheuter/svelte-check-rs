<script lang="ts" generics="T extends string">
    interface Props {
        items: T[];
        selected?: T;
        onselect?: (item: T) => void;
    }

    let { items, selected, onselect }: Props = $props();
</script>

<ul>
    {#each items as item}
        <li class:selected={item === selected}>
            <button onclick={() => onselect?.(item)}>{item}</button>
        </li>
    {/each}
</ul>
