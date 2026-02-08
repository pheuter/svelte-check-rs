<script lang="ts" generics="TMode extends 'single' | 'multiple'">
    type ValueType = TMode extends 'single' ? string : string[];

    interface Props {
        mode: TMode;
        value?: ValueType;
        onchange?: (value: ValueType) => void;
    }

    let { mode, value, onchange }: Props = $props();
</script>

<select>
    <slot />
</select>
