import { mount } from "svelte";
import Page from "../routes/issue-105-page-no-props/+page.svelte";

// Issue #105: mount() on a SvelteKit page component that doesn't use $props()
// should not produce TS2769 "No overload matches this call".
// Previously, the transformer would force PageProps as the render return type
// even when the component doesn't declare props, making mount() require props.
void mount(Page, {
  target: document.body,
});
