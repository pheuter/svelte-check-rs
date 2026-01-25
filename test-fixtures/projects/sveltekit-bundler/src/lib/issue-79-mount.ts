import { mount } from "svelte";
import Issue79Component from "$lib/components/Issue79Component.svelte";

const counter = mount(Issue79Component, {
  target: document.body,
  props: { count: 5, name: "Test" },
});

void counter.count;
void counter.name;
counter.count = 10;
