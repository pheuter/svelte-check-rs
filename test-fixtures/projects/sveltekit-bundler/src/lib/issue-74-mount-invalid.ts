import { mount } from "svelte";
import Issue74Component from "$lib/components/Issue74Component.svelte";

const prop = "show_symbol";

void mount(Issue74Component, {
  target: document.body,
  // Missing required `element` prop should still be an error.
  props: { [prop]: true },
});
