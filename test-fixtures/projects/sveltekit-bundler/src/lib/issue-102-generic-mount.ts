import { mount } from "svelte";
import Issue102GenericComponent from "$lib/components/Issue102GenericComponent.svelte";

// mount() with a generic component should not produce TS2769
void mount(Issue102GenericComponent, {
  target: document.body,
  props: { items: ["a", "b", "c"], selected: "a" },
});
