// `mount()` must accept extra props (matching svelte-check), while still
// type-checking declared props. Passing an extra `tracking` prop here must not
// error; a wrong type for the declared `label` still would.
import { mount } from "svelte";
import Child from "./mount-loosen-child.svelte";

export function start(target: HTMLElement) {
  mount(Child, { target, props: { label: "hello", tracking: 42 } });
}
