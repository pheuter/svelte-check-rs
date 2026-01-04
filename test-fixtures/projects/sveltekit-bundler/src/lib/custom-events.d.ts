declare namespace svelteHTML {
	interface HTMLAttributes<T> {
		'on:demo_change'?: (event: CustomEvent<{ value: number }>) => void;
	}
}
