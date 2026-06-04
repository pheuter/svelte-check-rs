// External module living OUTSIDE the sveltekit-bundler workspace root.
//
// Used by the issue #2942 fixture to verify that a relative import reaching
// outside the workspace (`../../../../shared-external/value`) is rewritten so
// it resolves from the generated cache location instead of producing a spurious
// TS2307. The target must EXIST for the positive assertion to be meaningful.

export const sharedValue: number = 42;

export function sharedGreeting(name: string): string {
	return `Hello, ${name}!`;
}

export interface SharedConfig {
	enabled: boolean;
	label: string;
}
