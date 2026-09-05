<script lang="ts" generics="T extends string | number">
	import type { HTMLAttributes } from 'svelte/elements';

	type Props = Omit<HTMLAttributes<HTMLDivElement>, 'id'> & {
		id?: T;
		value: T;
	};

	const uid = $props.id() satisfies string;
	let { id: propId, value, ...props }: Props = $props();

	const uidIsString: string = uid;
	const propIdKeepsItsType: T | undefined = propId;
	const valueKeepsItsType: T = value;

	// @ts-expect-error The generated ID is a string, never the user prop type.
	const uidIsNotANumber: number = uid;
	void uidIsNotANumber;
</script>

<div
	{...props}
	id={uidIsString}
	data-prop-id={String(propIdKeepsItsType)}
	data-value={String(valueKeepsItsType)}
></div>
