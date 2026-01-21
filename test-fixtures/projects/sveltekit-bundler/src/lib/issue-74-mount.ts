import { mount } from "svelte";
import Issue74Component from "$lib/components/Issue74Component.svelte";

const element = { name: "demo", number: 1 };

const cases: Array<[string, string]> = [
  ["show_symbol", "symbol"],
  ["show_number", "number"],
];

for (const [prop, selector] of cases) {
  void selector;
  void mount(Issue74Component, {
    target: document.body,
    props: { element, [prop]: true },
  });
}

const handler_name = "onclick" as const;
const mock_handler = () => {};

void mount(Issue74Component, {
  target: document.body,
  props: { element, [handler_name]: mock_handler },
});
